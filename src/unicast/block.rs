use std::cell::UnsafeCell;
use std::hint::unreachable_unchecked;
use std::{mem, ptr};
use std::mem::MaybeUninit;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use branch_hints::unlikely;
use crate::block::CacheLineAlign;

pub const BLOCK_SIZE: usize = if cfg!(miri) { 128 } else { 4096/*1024*/ };

pub type BlockMem<T> = [UnsafeCell<MaybeUninit<T>>; BLOCK_SIZE];

// Lightweight block header. Shared amongst readers and writers.
pub struct Block<T>{
    /// Set to None, when the first reader enters the next block.
    pub next: spin::Mutex<Option<Arc<Block<T>>>>,
    
    pub read_succ: CacheLineAlign<AtomicUsize>,
    
    pub write_counter: AtomicUsize,
    
    // CacheLineAlign is CRUCIAL here for performance.
    pub read_counter : CacheLineAlign<AtomicUsize>,

    /// Freed as soon as read_succ == BLOCK_SIZE.
    mem_ptr: NonNull<BlockMem<T>>,
}

impl<T> Default for Block<T>{
    fn default() -> Self {
        let mem = Box::new(
            [const{ UnsafeCell::new(MaybeUninit::uninit()) }; BLOCK_SIZE]
        );
        Self{
            next: Default::default(),
            read_succ: Default::default(),
            write_counter: Default::default(),
            read_counter : Default::default(),
            mem_ptr: unsafe{ NonNull::new_unchecked(Box::into_raw(mem)) },
        }
    }
}
impl<T> Drop for Block<T>{
    fn drop(&mut self) {
        let read_succ = self.read_succ.load(Ordering::Acquire);
        let mem_deallocated = read_succ == BLOCK_SIZE;
        // This could happen either in the very last block, 
        // or if the whole queue was dropped.
        if unlikely(!mem_deallocated) {
            let mem = self.mem_ptr.as_ptr();
            
            // destruct remains
            if mem::needs_drop::<T>(){
                unsafe{
                    let len = self.write_counter.load(Ordering::Acquire);
                    let mem = self.mem_unchecked().cast_mut();
                    for i in read_succ..len {
                        ptr::drop_in_place(mem.add(i));
                    }
                }
            }
            
            // dealloc
            unsafe{
                drop(Box::from_raw(mem));
            }
        }
    }
}

impl<T> Block<T>{
    /// Should be called ONCE.
    /// All mem elements must be in destructed state.
    pub unsafe fn dealloc_destructed_mem(&self) {
        unsafe{ drop(Box::from_raw(self.mem_ptr.as_ptr())); }
    }
    
    /// `mem` must exists.
    #[inline]
    pub unsafe fn mem_unchecked(&self) -> *const T {
        self.mem_ptr.as_ptr().cast()
    }
}