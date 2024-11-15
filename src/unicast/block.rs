use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use crate::block::CacheLineAlign;

pub const BLOCK_SIZE: usize = 4096;

pub struct Block<T>{
    /// Set to None, when the first reader enters the next block.
    pub next: spin::Mutex<Option<Arc<Block<T>>>>,

    pub write_counter: AtomicUsize,
    // CacheLineAlign is CRUCIAL here for performance.
    pub read_counter : CacheLineAlign<AtomicUsize>,

    mem: UnsafeCell<[MaybeUninit<T>; BLOCK_SIZE]>    
}

impl<T> Default for Block<T>{
    fn default() -> Self {
        Self{
            next: Default::default(),
            write_counter: Default::default(),
            read_counter : Default::default(),
            mem: UnsafeCell::new([const{ MaybeUninit::uninit() }; BLOCK_SIZE]),
        }
    }
}

impl<T> Block<T>{
    #[inline]
    pub fn mem(&self) -> *const T {
        self.mem.get().cast()
    }
}