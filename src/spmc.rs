//! Single-producer, multi-consumer.
//!
//! Thread-safe lockless readers.
//!
//! Wrapping it in `Arc<Mutex>` will make it multi-producer.

use std::sync::atomic::Ordering;
use std::ops::Deref;
use branch_hints::unlikely;
use crate::block::{Block, BlockArc, BLOCK_SIZE};
use crate::LendingIterator;

pub struct Queue<T>{
    last_block: BlockArc<T>
}

unsafe impl<T: Send> Send for Queue<T>{}
unsafe impl<T: Send> Sync for Queue<T>{}

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
    pub fn reader(&self) -> LendingReader<T> {
        let last_block = self.last_block.clone();
        let block_len  = last_block.len.load(Ordering::Acquire);
        LendingReader {
            block: last_block,
            index: block_len,
            len:   block_len,
        }
    }
}

/// Queue consumer. [`LendingIterator`].
///
/// Constructed by [Queue::reader()].
pub struct LendingReader<T>{
    pub(crate) block: BlockArc<T>,
    pub(crate) index: usize,
    pub(crate) len  : usize,
}

unsafe impl<T: Sync> Send for LendingReader<T>{}
unsafe impl<T: Sync> Sync for LendingReader<T>{}

impl<T> Clone for LendingReader<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self{
            block: self.block.clone(),
            index: self.index,
            len  : self.len,
        }
    }
}

impl<T> LendingReader<T> {
    #[inline]
    fn next_impl(&mut self) -> Option<&T> {
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

    pub fn cloned(self) -> CloningReader<T>
    where
        T: Clone
    {
        CloningReader{reader: self}
    }
}

impl<T> LendingIterator for LendingReader<T>{
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<&T> {
        self.next_impl()
    }
}

/// Cloning queue consumer. [`Iterator`].
///
/// Requires [`Send`]+[`Clone`] instead of [`Sync`] (in [`LendingReader`]) -
/// to be [`Send`]+[`Sync`].
///
/// Constructed by [`LendingReader::cloned()`].
#[derive(Clone)]
pub struct CloningReader<T: Clone>{
    reader: LendingReader<T>
}
impl<T: Clone> Iterator for CloningReader<T>{
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.reader.next_impl().cloned()
    }
}

unsafe impl<T: Send + Clone> Send for CloningReader<T>{}
unsafe impl<T: Send + Clone> Sync for CloningReader<T>{}