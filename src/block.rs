use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::{mem, ptr};
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::ops::Deref;
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use branch_hints::unlikely;

pub(crate) const BLOCK_SIZE    : usize = if cfg!(miri) { 128 } else { 4096 };
pub(crate) const BITBLOCKS_LEN : usize = BLOCK_SIZE/64;

#[derive(Default)]
#[repr(align(64))]
pub(crate) struct CacheLineAlign<T>(T);
impl<T> CacheLineAlign<T>{
    #[inline]
    pub fn new(value: T) -> Self {
        CacheLineAlign(value)
    }
}
impl<T> Deref for CacheLineAlign<T>{
    type Target = T;
    
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//#[repr(C)]
pub(crate) struct Block<T> {
    /// # spmc 
    /// 
    /// `len` is synchronization point for `mem`.
    /// After we write to `mem`, we store `len` with "Release".
    /// Before we read from `mem`, we load `len` with "Acquire".
    /// In analogy with spin-lock synchronization.
    /// 
    /// # mpmc
    /// 
    /// It's len for writers. Readers use `bit_blocks` for getting
    /// actual block len.
    // Aligning with cache-line size gives us +10% perf.
    pub len : CacheLineAlign<AtomicUsize>,
    use_count : AtomicUsize,           // When decreases to 0 - frees itself
    pub next  : AtomicPtr<Self>,
    
    // This is not used in spmc.
    pub bit_blocks: [AtomicU64; BITBLOCKS_LEN],
    /*pub*/ mem : UnsafeCell<[MaybeUninit<T>; BLOCK_SIZE]>,
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

            (*ptr).len = Default::default();
            (*ptr).use_count = AtomicUsize::new(counter);
            (*ptr).next = AtomicPtr::new(null_mut());
            
            (*ptr).bit_blocks = core::array::from_fn(|_|AtomicU64::new(0)); 
        
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
        self.mem.get().cast()
    }
    
    // TODO: remove ordering param.
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
        let occupied_len = self.len.fetch_add(1, Ordering::AcqRel);
        
        if unlikely(occupied_len >= BLOCK_SIZE) {
            return Err(value);
        }

        // Actually save value.
        let index = occupied_len;
        unsafe{
            let mem = self.mem().cast_mut();
            mem.add(index).write(value);
        }

        // Update bitblock, indicating that value is ready to read.
        {
            let bit_block_index = index / 64;
            let bit_index = index % 64;
            
            let bitmask = 1 << bit_index;
            let atomic_block = unsafe{ self.bit_blocks.get_unchecked(bit_block_index) };
            atomic_block.fetch_or(bitmask, Ordering::Release);
        }
        
        Ok(())
    }    
}

pub(crate) struct BlockArc<T> {
    ptr: NonNull<Block<T>>,
    phantom_data: PhantomData<T>
}
unsafe impl<T> Send for BlockArc<T> {}
impl<T> BlockArc<T>{
    #[inline]
    pub unsafe fn from_raw(ptr: NonNull<Block<T>>) -> Self {
        Self{ptr, phantom_data: PhantomData}
    }
    
    #[inline]
    pub fn into_raw(self) -> NonNull<Block<T>> {
        let this = ManuallyDrop::new(self);
        this.ptr
    }
    
    #[inline]
    pub fn as_non_null(&mut self) -> NonNull<Block<T>> {
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
        Self{ptr: self.ptr, phantom_data: PhantomData}
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