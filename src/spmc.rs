//! Single-producer, multi-consumer.
//! 
//! Thread-safe lockless readers.
//! 
//! Wrapping it in `Arc<Mutex>` will make it multi-producer. 

use std::sync::atomic::Ordering;
use crate::block::{Block, BlockArc};
use crate::reader::LendingReader;

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
    
    #[inline]
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
        let result = self.last_block.try_push(value);
        if let Err(value) = result {
            #[cold]
            #[inline(never)]
            fn insert_block_and_push<T>(this: &mut Queue<T>, value: T) {
                this.insert_block();
                let result = this.last_block.try_push(value);
                if let Err(_) = result {
                    unsafe{ std::hint::unreachable_unchecked() }
                }
            }
            insert_block_and_push(self, value);
        }
    }
    
    #[must_use]
    #[inline]
    pub fn lending_reader(&self) -> LendingReader<T> {
        let last_block = self.last_block.clone();
        let block_len = unsafe {
            last_block.len.load(Ordering::Acquire)  
        };
        LendingReader {
            block: last_block,
            index: block_len,
            len:   block_len,
        }
    }
}