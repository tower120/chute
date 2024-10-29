//! Single-producer, multi-consumer.
//! 
//! Thread-safe lockless readers.
//! 
//! Wrapping it in `Arc<Mutex>` will make it multi-producer. 

use std::ops::Deref;
use std::sync::atomic::Ordering;
use branch_hints::unlikely;
use crate::block::{Block, BlockArc, BLOCK_SIZE};
use crate::reader::Reader;

pub struct Queue<T>{
    last_block: BlockArc<T>
}

impl<T> Default for Queue<T>{
    #[inline]
    fn default() -> Self {
        Self{
            last_block: Block::new(),
        }
    }
}

impl<T> Queue<T> {
    #[inline]
    pub fn new() -> Self{
        Default::default()
    }
    
    #[cold]
    #[inline(never)]
    fn insert_block(&mut self) {
        // 1. Make new block
        //    +1 counter for EventQueue::last_block
        //    +1 counter for Block::next
        let new_block = Block::with_counter(2);
        
        // 2. Connect new block with old
        self.last_block.next.store(new_block.as_non_null().as_ptr(), Ordering::Release);
        
        // 3. Set new block
        self.last_block = new_block;
    }
    
    #[inline]
    pub fn push(&mut self, value: T) {
        let mut len = self.last_block.len.load(Ordering::Relaxed);
        if unlikely(len == BLOCK_SIZE) {
            self.insert_block();
            len = 0;
        }
        
        let last_block = unsafe{ self.last_block.as_non_null().as_mut() };
        unsafe{
            let mem = &mut *last_block.mem.get();
            mem.get_unchecked_mut(len).write(value);
        }
        
        last_block.len.store(len+1, Ordering::Release);
    }
    
    #[must_use]
    #[inline]
    pub fn reader(&self) -> Reader<T> {
        let last_block = self.last_block.clone();
        let block_len = unsafe {
            last_block.len.load(Ordering::Acquire)  
        };
        Reader {
            block: last_block,
            index: block_len,
            len:   block_len,
        }
    }
}