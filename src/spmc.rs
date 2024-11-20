//! Single-producer, multi-consumer.
//! 
//! Thread-safe lockless readers.
//! 
//! Wrapping it in `Arc<Mutex>` will make it multi-producer. 

use std::sync::atomic::Ordering;
use std::ops::Deref;
use branch_hints::unlikely;
use crate::block::{Block, BlockArc, BLOCK_SIZE};
use crate::LendingReader;

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

/// Queue consumer.
/// 
/// Constructed by [Queue::reader()].
pub struct Reader<T>{
    pub(crate) block: BlockArc<T>,
    pub(crate) index: usize,
    pub(crate) len  : usize,
}

impl<T> Clone for Reader<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self{
            block: self.block.clone(),
            index: self.index,
            len  : self.len,
        }
    }
}

impl<T> LendingReader for Reader<T>{
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<&T> {
        if self.index == self.len {
            if unlikely(self.len == BLOCK_SIZE) {
                // fetch next block, release current
                if let Some(next_block) = self.block.try_load_next(Ordering::Acquire) {
                    self.index = 0;
                    self.len   = next_block.len.load(Ordering::Acquire);
                    self.block = next_block;
                    
                    // TODO: Disallow empty blocks?
                    if self.len == 0 {
                        return None;
                    }
                } else {
                    return None;
                }
            } else {
                // Reread len.
                // This is synchronization point. `mem` data should be in 
                // current thread visibility, after `len` atomic load. 
                // In analogue with spin-lock.
                let block_len = self.block.len.load(Ordering::Acquire);
                if self.len == block_len {
                    // nothing changed.
                    return None;
                } 
                self.len = block_len;
            }
        }
        
        unsafe{
            let value = &*self.block.mem().add(self.index);
            self.index += 1;
            Some(value)
        }
    }
}


#[cfg(test)]
mod test{
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use rand::{Rng, SeedableRng};
    use crate::block::BLOCK_SIZE;
    use crate::spmc::Queue;
    use crate::LendingReader;
    use crate::test::StringWrapper;

    fn test_spmc_mt<Value>(rt: usize, len: usize)
    where
        Value: From<usize> + Into<usize> + Clone + 'static,
    {
        let queue: Arc<spin::Mutex<Queue<Value>>> = Default::default();
        
        let mut joins = Vec::new();
        
        // Readers
        let control_sum = (0..len).sum();        
        for _ in 0..rt { 
            let mut reader = queue.lock().reader();
            joins.push(std::thread::spawn(move || {
                let mut sum = 0;
                let mut i = 0;
                loop {
                    if let Some(value) = reader.next() {
                        sum += value.clone().into();
                        
                        i += 1;
                        if i == len {
                            break;
                        }
                    }
                }
                assert_eq!(sum, control_sum);
            }));
        }
        
        joins.push(std::thread::spawn(move || {
            for i in 0..len{
                queue.lock().push(i.into());
            }
        }));
        
        for join in joins{
            join.join().unwrap();    
        }
    }
    
    #[test]
    fn fuzzy_spmc(){
        const MAX_THREADS: usize = if cfg!(miri) {4 } else {16  };
        const RANGE      : usize = if cfg!(miri) {8 } else {40  } * BLOCK_SIZE;
        const REPEATS    : usize = if cfg!(miri) {10} else {1000};
        
        let mut rng = rand::rngs::StdRng::seed_from_u64(0xe15bb9db3dee3a0f);
        for _ in 0..REPEATS {
            let rt  = rng.gen_range(1..=MAX_THREADS);
            let len = rng.gen_range(0..RANGE);
            test_spmc_mt::<usize>(rt, len);
            test_spmc_mt::<StringWrapper>(rt, len);
        }
    }    
}