use std::sync::atomic::Ordering;
use branch_hints::unlikely;
use crate::block::{BlockArc, BLOCK_SIZE};
use crate::LendingReader;
use crate::mpmc::Queue;

/// Unordered queue consumer.
/// 
/// TODO: description. Low latency.
/// 
/// Constructed by [Queue::unordered_reader()].
pub struct UnorderedReader<T>{
    pub(crate) block: BlockArc<T>,
    pub(crate) bitblock: u64,
    pub(crate) bitblock_mask : u64,
    pub(crate) bitblock_index: usize,
}

impl<T> LendingReader for UnorderedReader<T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<&T> {
        let mut bitblock = self.bitblock & self.bitblock_mask;
        if bitblock == 0 {
            if self.bitblock_mask == 0 {
                // we completely read all bitblock associated items.
                
                if unlikely(self.bitblock_index == BLOCK_SIZE/64 - 1) {
                    // try fetch next block, release current
                    if let Some(next_block) = self.block.try_load_next(Ordering::Acquire) {
                        self.block = next_block;
                        self.bitblock_index = 0;
                    } else {
                        // Leave bitblock_mask == and bitblock_index = BLOCK_SIZE/64 - 1
                        return None;
                    }
                } else {
                    self.bitblock_index += 1;
                }
                self.bitblock_mask = u64::MAX;
            }

            // read bitblock
            self.bitblock = unsafe {
                self.block.bit_blocks.get_unchecked(self.bitblock_index)
            }.load(Ordering::Acquire);
        
            bitblock = self.bitblock & self.bitblock_mask;
            if bitblock == 0 {
                // still nothing
                return None;
            }
        }

        let bitblock_index = bitblock.trailing_zeros() as usize;
        let index = self.bitblock_index*64 + bitblock_index;
        
        // zero consumed bit
        self.bitblock_mask &= !(1 << bitblock_index);
        
        unsafe{
            let value = &*self.block.mem().add(index);
            Some(value)
        }
    }
}

// TODO: remove - use mod::mpmc tests 
#[cfg(test)]
mod test{
    use std::sync::Arc;
    use itertools::assert_equal;
    use crate::block::BLOCK_SIZE;
    use crate::LendingReader;
    use crate::mpmc::Queue;

    #[test]
    fn smoke_test() {
        let queue: Arc<Queue<usize>> = Default::default();
        let mut reader = queue.unordered_reader();
        let mut writer = queue.writer();
        
        const COUNT: usize = BLOCK_SIZE*40; 
        for i in 0..COUNT {
            //queue.blocking_push(i);
            writer.push(i);    
        }
        
        let mut vec = Vec::new();
        while let Some(value) = reader.next() {
            vec.push(value.clone());
        }

        {
            let t = reader.next();
            queue.blocking_push(500);
            let t = reader.next();
            assert_eq!(t.unwrap(), &500);
        }
        
        assert_equal(vec, 0..COUNT);
    } 
    
    #[test]
    fn mt_test() {
        let rt = 1;
        let wt = 1;
        let len = 20000;
        
        let queue: Arc<Queue<usize>> = Default::default();

        let mut joins = Vec::new();

        // Readers
        let control_sum = (0..len).sum();        
        for _ in 0..rt { 
            // TODO: test both Reader and UnorderedReader 
            let mut reader = queue./*reader()*/unordered_reader();
            joins.push(std::thread::spawn(move || {
                let mut sum: usize = 0;
                let mut i = 0;
                loop {
                    if let Some(value) = reader.next() {
                        sum += *value;
                        
                        i += 1;
                        if i == len {
                            break;
                        }
                    }
                }
                assert_eq!(sum, control_sum);
            }));
        }
        
        // Writers
        for t in 0..wt {
            let messages = len/wt;
            let mut writer = queue.writer();
            joins.push(std::thread::spawn(move || {
                for i in t*messages..(t+1)*messages {
                    writer.push(i.into());
                }
            }));
        }
        
        for join in joins{
            join.join().unwrap();    
        }
    }    
    
}