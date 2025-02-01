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
    pub(crate) index_offset: usize,
    pub(crate) bitblock_index: usize,
}

impl<T> LendingReader for UnorderedReader<T> {
    type Item = T;

    #[inline(always)]
    fn next(&mut self) -> Option<&T> {
        let mut bitblock = self.bitblock & self.bitblock_mask;
        
        if bitblock == 0 {
            if unlikely(self.bitblock_index == BLOCK_SIZE/64) {
                // fetch next block, release current
                if let Some(next_block) = self.block.try_load_next(Ordering::Acquire) {
                    bitblock = unsafe {
                        next_block.bit_blocks.get_unchecked(0)
                    }.load(Ordering::Acquire);
                    
                    self.block = next_block;
                    self.bitblock      = bitblock;
                    self.bitblock_mask = u64::MAX;
                    self.index_offset  = 0;
                    self.bitblock_index = (bitblock == u64::MAX) as usize;

                    // yet empty block?
                    if bitblock == 0 {
                        return None;
                    }
                } else {
                    return None;
                }
            } else {
                // Reread bitblock.
                let next_bitblock = unsafe {
                    self.block.bit_blocks.get_unchecked(self.bitblock_index)
                }.load(Ordering::Acquire);
                
                if next_bitblock == 0 {
                    // have nothing.
                    return None;
                }
                
                if self.bitblock_index*64 == self.index_offset
                && next_bitblock == self.bitblock {
                    // nothing changed.
                    return None;
                }
                
                self.index_offset = self.bitblock_index*64; 
                self.bitblock     = next_bitblock;
                
                if self.bitblock_mask == 0 {
                    self.bitblock_mask = u64::MAX;
                }
                
                bitblock = next_bitblock & self.bitblock_mask;
                
                // Switch to next bitblock.
                if next_bitblock == u64::MAX {
                    self.bitblock_index += 1;
                }
            }
        }

        let bitblock_index = bitblock.trailing_zeros() as usize;
        let index = self.index_offset + bitblock_index;
        
        /*if bitblock_index >= 64{
            panic!("Unordered Reader index out of bounds = {bitblock_index}");
        }*/

        // zero consumed bit
        self.bitblock_mask &= !(1 << bitblock_index);
        
        unsafe{
            let value = &*self.block.mem().add(index);
            Some(value)
        }
    }
}

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
        //let mut writer = queue.writer();
        
        const COUNT: usize = BLOCK_SIZE*4; 
        for i in 0..COUNT {
            queue.blocking_push(i);
            //writer.push(i);    
        }
        
        let mut vec = Vec::new();
        while let Some(value) = reader.next() {
            vec.push(value.clone());
        }
        assert_equal(vec, 0..COUNT);
    } 
    
}