use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::{cmp, mem, ptr};
use std::cell::UnsafeCell;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::ops::Deref;
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use branch_hints::unlikely;

pub(crate) const BLOCK_SIZE: usize = 4096;

#[derive(Default, Clone, Debug, Eq, PartialEq)]
pub(crate) struct Packed {
    /// `len` occupied by data + currently written data.
    pub(crate) occupied_len: u32,
    pub(crate) writers     : u32
}
impl From<Packed> for u64 {
    #[inline]
    fn from(value: Packed) -> Self {
        value.occupied_len as u64
        + ((value.writers as u64) << 32) 
    }
}
impl From<u64> for Packed {
    #[inline]
    fn from(value: u64) -> Self {
        Self{
            occupied_len: value as u32,
            writers: (value >> 32) as u32,
        }
    }
}

#[test]
fn test_pack(){
    let pack = Packed{ occupied_len: 12, writers: 600 };
    let packed: u64 = pack.clone().into();
    let unpacked: Packed = packed.into();
    assert_eq!(pack, unpacked);
}

#[test]
fn test_pack_add(){
    let pack: u64 = Packed{ occupied_len: 12, writers: 600 }.into();
    let add : u64 = Packed{ occupied_len: 1, writers: 1 }.into();
    let res : u64 = pack + add;
    
    let Packed{occupied_len, writers} = res.into();
    assert_eq!(occupied_len, 13);
    assert_eq!(writers, 601);
}

#[test]
fn test_pack_sub(){
    let pack: u64 = Packed{ occupied_len: 1, writers: 1 }.into();
    let sub : u64 = Packed{ occupied_len: 0, writers: 1 }.into();
    let res : u64 = pack - sub;
    
    let Packed{occupied_len, writers} = res.into();
    assert_eq!(occupied_len, 1);
    assert_eq!(writers, 0);
}

pub(crate) struct Block<T> {
    pub mem : UnsafeCell<[MaybeUninit<T>; BLOCK_SIZE]>,
    /// `len` is synchronization point for `mem`.
    /// After we write to `mem`, we store `len` with "Release".
    /// Before we read from `mem`, we load `len` with "Acquire".
    /// In analogy with spin-lock synchronization.
    pub len : AtomicUsize,
    packed: AtomicU64,
    use_count : AtomicUsize,           // When decreases to 0 - frees itself
    pub next  : AtomicPtr<Self>,
    
    // TODO: remove
    /// Purely for debug purposes
    pub id: usize,
}



impl<T> Block<T>{
    #[must_use]
    pub fn with_counter(counter: usize) -> BlockArc<T> {
        unsafe{
            let layout = Layout::new::<Self>();
            let ptr = alloc(layout) as *mut Self;
            if ptr.is_null() {
                handle_alloc_error(layout);
            }

            (*ptr).len = AtomicUsize::new(0);
            (*ptr).packed = Default::default();
            (*ptr).use_count = AtomicUsize::new(counter);
            (*ptr).next = AtomicPtr::new(null_mut());
            
            (*ptr).id = 0;
        
            BlockArc::from_raw(NonNull::new_unchecked(ptr))
        }
    }
    
    #[must_use]
    pub fn new() -> BlockArc<T> {
        Self::with_counter(1)
    }
    
    #[inline]
    pub unsafe fn inc_use_count(this: NonNull<Self>) {
        this.as_ref().use_count.fetch_add(1, Ordering::Relaxed);
    }
    
    #[inline(never)]
    #[cold]
    unsafe fn drop_this(mut this: NonNull<Self>){
        debug_assert!(this.as_ref().use_count.load(Ordering::Acquire) == 0);
        
        // drop mem
        if mem::needs_drop::<T>() {
            let len = this.as_ref().len.load(Ordering::Acquire);
            let mem = this.as_mut().mem.get_mut();
            for i in 0..len {
                ptr::drop_in_place(mem.get_unchecked_mut(i).assume_init_mut());
            }
        }
        
        // drop next
        let next = this.as_ref().next.load(Ordering::Acquire);
        if let Some(next) = NonNull::new(next) {
            Block::dec_use_count(next);
        }
        
        // dealloc
        let layout = Layout::new::<Self>();
        dealloc(this.as_ptr().cast(), layout);
    }
    
    #[inline]
    pub unsafe fn dec_use_count(this: NonNull<Self>) {
        // Release instead of AcqRel, because we'll drop this at 0
        let prev = this.as_ref().use_count.fetch_sub(1, Ordering::Release);
        if prev == 1 {
            Self::drop_this(this);
        }
    }
    
    #[inline]
    pub fn mem(&self) -> *const T {
        unsafe{
            let mem = &mut *self.mem.get();
            mem.as_ptr().cast()
        }
    }
    
    #[inline]
    pub fn load_packed(&self, ordering: Ordering) -> Packed {
        self.packed.load(ordering).into()
    }
    
    #[must_use]
    #[inline]
    pub fn try_load_next(&self, ordering: Ordering) -> Option<BlockArc<T>> {
        let next = self.next.load(ordering);
        if let Some(ptr) = NonNull::new(next) { 
            let arc = unsafe {
                Block::inc_use_count(ptr);
                BlockArc::from_raw(ptr) 
            };
            Some(arc) 
        } else {
            None
        }
    }
    
    #[inline]
    pub fn try_push(&self, value: T) -> Result<(), T> {
        // Just Acquire?
        let Packed{ occupied_len, .. } = self.packed.fetch_add(
            Packed{ occupied_len: 1, writers: 1 }.into(),
            Ordering::AcqRel
        ).into();
        let occupied_len = occupied_len as usize;
        
        if unlikely(occupied_len >= BLOCK_SIZE) {
            // put counters back.
            {
                // Just Release?
                let Packed{ occupied_len, writers } = self.packed.fetch_sub(
                    Packed{ occupied_len: 1, writers: 1 }.into(),
                    Ordering::AcqRel
                ).into();
                
                if unlikely(writers == 1) {
                    debug_assert_eq!(occupied_len - 1, BLOCK_SIZE as _ );
                    self.len.store(BLOCK_SIZE, Ordering::Release);
                }             
            }          
            
            return Err(value);
        }

        unsafe{
            let mem = &mut *self.mem.get();
            let index = occupied_len;
            mem.get_unchecked_mut(index).write(value);
        }
        
        // Just Release?
        let Packed{ occupied_len, writers } = self.packed.fetch_sub(
            Packed{ occupied_len: 0, writers: 1 }.into(),
            Ordering::AcqRel
        ).into();
        
        if writers == 1 {
            self.len.fetch_max(occupied_len as usize, Ordering::Release);
        } else {
            // TODO: self.len.fetch_add(0, Ordering::Release)
            //       for mem changes visibility?
            //self.len.fetch_add(0, Ordering::Release);
        }
        
        Ok(())
    }
}

pub(crate) struct BlockArc<T> {
    ptr: NonNull<Block<T>>    
}
unsafe impl<T> Send for BlockArc<T> {}
impl<T> BlockArc<T>{
    #[inline]
    pub unsafe fn from_raw(ptr: NonNull<Block<T>>) -> Self {
        Self{ptr}
    }
    
    #[inline]
    pub fn into_raw(self) -> NonNull<Block<T>> {
        let this = ManuallyDrop::new(self);
        this.ptr
    }
    
    #[inline]
    pub fn as_non_null(&self) -> NonNull<Block<T>> {
        self.ptr
    }
}
impl<T> Deref for BlockArc<T> {
    type Target = Block<T>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe{ self.ptr.as_ref() }
    }
}
impl<T> Clone for BlockArc<T> {
    #[inline]
    fn clone(&self) -> Self {
        unsafe{
            Block::inc_use_count(self.ptr)
        }
        Self{ptr: self.ptr}
    }
}
impl<T> Drop for BlockArc<T> {
    #[inline]
    fn drop(&mut self) {
        unsafe{
            Block::dec_use_count(self.ptr)
        }
    }
}