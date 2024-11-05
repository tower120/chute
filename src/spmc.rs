//! Single-producer, multi-consumer.
//! 
//! Thread-safe lockless readers.
//! 
//! Wrapping it in `Arc<Mutex>` will make it multi-producer. 

use std::sync::atomic::Ordering;
use std::ops::Deref;
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
        let mut new_block = Block::with_counter(2);
        
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
        
        // Take & instead of &mut to make MIRI happy about shared access.
        // Thou, we write with Unique access.
        let last_block = self.last_block.deref();
        unsafe{
            let mem = last_block.mem().cast_mut();
            mem.add(len).write(value);
        }
        
        last_block.len.store(len+1, Ordering::Release);
    }
    
    #[must_use]
    #[inline]
    pub fn reader(&self) -> Reader<T> {
        let last_block = self.last_block.clone();
        let block_len  = last_block.len.load(Ordering::Acquire);
        Reader {
            block: last_block,
            index: block_len,
            len:   block_len,
        }
    }
}

#[cfg(test)]
mod test{
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use crate::block::BLOCK_SIZE;
    use crate::spmc::Queue;
    use crate::lending_iterator::LendingIterator;

    // TODO: fuzzy version, same as with mpmc.
    #[test]
    fn test_spmc_mt() {
        let queue: Arc<spin::Mutex<Queue<usize>>> = Default::default();
        let mut reader0 = queue.lock().reader();
        let mut reader1 = queue.lock().reader();
        
        let mut joins = Vec::new();
        
        const COUNT: usize = BLOCK_SIZE * 8;
        
        joins.push(std::thread::spawn(move || {
            for i in 0..COUNT{
                queue.lock().push(i);
                //std::thread::yield_now();
            }
        }));
        
        let rs0: Arc<AtomicUsize> = Default::default();
        {
            let rs0 = rs0.clone();
            joins.push(std::thread::spawn(move || {
                let mut sum = 0;
                let mut i = 0;
                loop {
                    if let Some(value) = reader0.next() {
                        sum += value;
                        
                        i += 1;
                        if i == COUNT {
                            break;
                        }
                    }
                }
                rs0.store(sum, Ordering::Release);
            }));
        }

        let rs1: Arc<AtomicUsize> = Default::default();
        {
            let rs1 = rs1.clone();
            joins.push(std::thread::spawn(move || {
                let mut sum = 0;
                let mut i = 0;
                loop {
                    if let Some(value) = reader1.next() {
                        sum += value;
                        
                        i += 1;
                        if i == COUNT {
                            break;
                        }
                    }
                }
                rs1.store(sum, Ordering::Release);
            }));
        }
        
        for join in joins{
            join.join().unwrap();    
        }
        
        let sum = (0..COUNT).sum();
        assert_eq!(rs0.load(Ordering::Acquire), sum);
        assert_eq!(rs1.load(Ordering::Acquire), sum);
    }
}