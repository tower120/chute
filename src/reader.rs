use branch_hints::unlikely;
use std::sync::atomic::Ordering;
use crate::block::{BlockArc, BLOCK_SIZE};
use crate::lending_iterator::LendingIterator;

// TODO: Clone
/// Lending reader return `&T`.
///
/// This is faster than [EventReader], since it does not clone `T` before return.
/// 
/// # Design choices
/// 
/// The value returned by the reader lives as long as the block where it is stored.
/// From the reader's point of view, we can guarantee that the value remains valid
/// as long as the block does not change. However, in Rust, we cannot make such 
/// granular guarantees. Instead, we guarantee that the value remains valid until the
/// reader is mutated. This means the value is guaranteed to live until the next 
/// read operation, at which point the block may change, and the old block could 
/// be destructed.
pub struct LendingReader<T>{
    pub(crate) block: BlockArc<T>,
    pub(crate) index: usize,
    pub(crate) len  : usize,
}

impl<T> LendingIterator for LendingReader<T>{
    type Item<'a> = &'a T where Self: 'a;

    #[inline]
    fn next(&mut self) -> Option<Self::Item<'_>> {
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