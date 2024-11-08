//! Multi-producer, multi-consumer.
//! 
//! Thread-safe lockless writers and readers.

use std::marker::PhantomData;
use std::ptr::{null_mut, NonNull};
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};
use branch_hints::unlikely;
use crate::block::{Block, BlockArc, BLOCK_SIZE};
use crate::LendingReader;

pub struct Queue<T> {
    last_block: AtomicPtr<Block<T>>,
    phantom_data: PhantomData<T>
}

impl<T> Default for Queue<T> {
    #[inline]
    fn default() -> Self {
        Self {
            last_block: AtomicPtr::new(Block::<T>::new().into_raw().as_ptr()),
            phantom_data: PhantomData
        }   
    }
}

impl<T> Queue<T> {
    #[must_use]
    #[inline]
    pub fn new() -> Arc<Self> {
        Default::default()    
    }
    
    #[inline]
    fn lock_last_block(&self) -> NonNull<Block<T>> {
        loop {
            let ptr = self.last_block.swap(null_mut(), Ordering::Acquire);
            if let Some(ptr) = NonNull::new(ptr) {
                break ptr
            }
        }
    }
    
    #[inline]
    fn unlock_last_block(&self, ptr: NonNull<Block<T>>) {
        self.last_block.store(ptr.as_ptr(), Ordering::Release);
    }
    
    #[must_use]
    #[inline]
    fn load_last_block(&self) -> BlockArc<T> {
        // fetch ptr and "lock"
        let ptr = self.lock_last_block();
        
        let arc = unsafe {
            Block::inc_use_count(ptr);
            BlockArc::from_raw(ptr)
        };
        
        // release "lock"
        self.unlock_last_block(ptr);
        
        arc
    }
    
    /// Returns (latest block, inserted). 
    /// 
    /// The latest block SHOULD be non-full, but can be actually full.
    /// 
    /// Blocking.
    #[must_use]
    #[inline]
    fn insert_block(&self) -> (BlockArc<T>, bool) {
        // 1. Lock
        let last_block = self.lock_last_block();
        let last_block_ref = unsafe{ last_block.as_ref() };
        
        if last_block_ref.len.load(Ordering::Acquire) < BLOCK_SIZE {
            // Arc counter ++
            let arc = unsafe { 
                Block::inc_use_count(last_block);
                BlockArc::from_raw(last_block)
            };
            
            // unlock
            self.unlock_last_block(last_block);
            
            return (arc, false);
        }
        
        // 2. Make new block
        //    +1 counter for EventQueue::last_block (written on unlock_last_block)
        //    +1 counter for Block::next
        //    +1 counter for returned BlockArc 
        let new_block = Block::with_counter(3).into_raw();

        // 3. Connect new block with old
        last_block_ref.next.store(new_block.as_ptr(), Ordering::Release);
        
        // 4. Arc -- old block
        unsafe{
            Block::dec_use_count(last_block);
        }
        
        // 5. Set new block as last, and release lock.
        self.unlock_last_block(new_block);

        (unsafe{ BlockArc::from_raw(new_block) }, true)
    }
    
    /// Push value to queue.
    /// 
    /// This is a blocking operation - you can't `blocking_push` simultaneously 
    /// from different threads, but most of the time writers can push in parallel
    /// with this call. 
    /// 
    /// Faster than constructing writer just to push a single value
    /// `writer().push(..)`. But slower than [Writer::push] itself.
    /// 
    /// Use it if you need to occasionally push a single value.
    #[inline]
    pub fn blocking_push(&self, value: T) {
        // 1. Lock
        let block = self.lock_last_block();
        if let Err(value) = unsafe{ block.as_ref() }.try_push(value) {
            #[cold]
            #[inline(never)]
            fn insert_block_and_push<T>(this: &Queue<T>, last_block: &Block<T>, value: T){
                let mut new_block = {
                    // 2. Make new block
                    //    +1 counter for EventQueue::last_block (written on unlock_last_block)
                    //    +1 counter for Block::next
                    //    +1 counter for returned BlockArc 
                    let new_block = Block::with_counter(3).into_raw();
            
                    // 3. Connect new block with old
                    last_block.next.store(new_block.as_ptr(), Ordering::Release);
                    
                    // 4. Arc -- old block
                    unsafe{
                        Block::dec_use_count(last_block.into());
                    }
                    
                    unsafe{ BlockArc::from_raw(new_block) }                    
                };
                
                let result = new_block.try_push(value);
                if result.is_err(){
                    unsafe{ std::hint::unreachable_unchecked() }
                }
                
                // 5. Set new block as last, and release lock.
                this.unlock_last_block(new_block.as_non_null());
            }
            insert_block_and_push(self, unsafe{block.as_ref()}, value);
            return;
        }
        self.unlock_last_block(block);
    }
    
    #[must_use]
    #[inline]
    pub fn writer(self: &Arc<Self>) -> Writer<T> {
        Writer {
            block: self.load_last_block(),
            event_queue: self.clone(),
        }
    }
    
    /// [Reader] will receive all messages that are pushed AFTER this call.
    #[must_use]
    #[inline]
    pub fn reader(&self) -> Reader<T> {
        let last_block = self.load_last_block();
        let block_len  = last_block.len.load(Ordering::Acquire);
        Reader {
            block: last_block,
            index: block_len,
            len:   block_len,
            bitblock_index: block_len/64
        }
    }
}
impl<T> Drop for Queue<T> {
    #[inline]
    fn drop(&mut self) {
        let last_block = self.last_block.load(Ordering::Acquire);
        unsafe{
            Block::dec_use_count(NonNull::new_unchecked(last_block));
        }
    }
}

/// Queue producer.
///
/// Same as reader, writer internally keeps a block pointer.
/// Which means it also prevents the whole queue after its block form being dropped. 
/// Block pointer updated to the latest one on each [push()] or [update()].
/// You also can just construct a new Writer for each write session.
///
/// Constructed by [Queue::writer()].
///
/// [push()]: Self::push
/// [update()]: Self::update
/// [Queue::writer()]: crate::mpmc::Queue::writer
pub struct Writer<T> {
    block: BlockArc<T>,
    event_queue: Arc<Queue<T>>
} 
impl<T> Writer<T> {
    #[inline]
    fn fast_forward_to_last_block(&mut self, max_jumps: usize) -> Result<(), ()> {
        let mut last = self.block.as_non_null();
        for _ in 0..max_jumps {
            let next = unsafe{ last.as_ref() }.next.load(Ordering::Acquire);
            if let Some(next) = NonNull::new(next){
                last = next;
            } else {
                // update resource counters, change block.
                if last != self.block.as_non_null() {
                    unsafe {
                        Block::inc_use_count(last);
                        self.block = BlockArc::from_raw(last);
                    }
                }
                return Ok(());
            }
        }
        Err(())
    }
    
    /// UNTESTED
    /// 
    /// Moves writer's internal block pointer to the latest in a queue.
    /// This prevents writer from keeping a potentially unused blocks alive. 
    pub fn update(&mut self) {
        if self.fast_forward_to_last_block(5).is_err() {
            self.block = self.event_queue.load_last_block();
        }
    }
    
    /*#[cold]
    #[inline(never)]
    fn fast_forward_and_push(&mut self, mut value: T){
        // TODO: skip fast_forward and just load from event_queue.last_block 
        //       after N tries.
        self.fast_forward_to_last_block();
        
        loop{
            let inserted = self.block.try_push(value);
            if let Err(v) = inserted {
                value = v;
                self.block = self.event_queue.insert_block();
            } else {
                break;
            }
        }
    }*/
    
    
    #[cold]
    #[inline(never)]
    fn insert_block_and_push(&mut self, mut value: T){
        // TODO: try load next first? 
        loop{
            (self.block, _) = self.event_queue.insert_block();
            
            let inserted = self.block.try_push(value);
            if let Err(v) = inserted {
                value = v;
            } else {
                break;
            }
        }
    }    

    #[inline]
    pub fn push(&mut self, value: T) {
        let inserted = self.block.try_push(value);
        if let Err(value) = inserted {
            self.insert_block_and_push(value);
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
    pub(crate) bitblock_index  : usize,
}

impl<T> Clone for Reader<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self{
            block: self.block.clone(),
            index: self.index,
            len  : self.len,
            bitblock_index: self.bitblock_index
        }
    }
}

impl<T> LendingReader for Reader<T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<&T> {
        if self.index == self.len {
            if unlikely(self.len == BLOCK_SIZE) {
                // fetch next block, release current
                if let Some(next_block) = self.block.try_load_next(Ordering::Acquire) {

                    self.index = 0;

                    // 10-15% preformance gain with this on seq read.
                    // Optimization for occasional readers - that traverse
                    // several blocks at once, once in a while.
                    //
                    // Relaxed - because not catching this case is safe. 
                    if !next_block.next.load(Ordering::Relaxed).is_null() {
                        self.block = next_block;
                        self.len = BLOCK_SIZE;
                        self.bitblock_index = 64;
                    } else {
                        let bit_block = unsafe {
                            next_block.bit_blocks.get_unchecked(0)
                        }.load(Ordering::Acquire);
    
                        self.block = next_block;
                        self.len   = bit_block.trailing_ones() as usize;
                        self.bitblock_index = if bit_block == u64::MAX {1} else {0};
                        
                        // TODO: Disallow empty blocks?
                        if self.len == 0 {
                            return None;
                        }
                    }
                } else {
                    return None;
                }
            } else {
                // Reread len.
                // This is a synchronization point. `mem` data should be in 
                // current thread visibility, after an atomic load. 
                    
                let bit_block = unsafe {
                    self.block.bit_blocks.get_unchecked(self.bitblock_index)
                }.load(Ordering::Acquire);
                
                let new_len = self.bitblock_index*64 + bit_block.trailing_ones() as usize;
                
                if self.len == new_len {
                    // nothing changed.
                    return None;
                } 
                
                // Switch to next bitblock.
                // Do not check for >=BLOCK_SIZE. That will happen later.
                if bit_block == u64::MAX {
                    self.bitblock_index = self.bitblock_index + 1;
                }
                
                self.len = new_len;
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
mod test_mpmc{
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use itertools::assert_equal;
    use crate::block::BLOCK_SIZE;
    use crate::LendingReader;
    use crate::mpmc::Queue;

    #[test]
    fn test_mpmc() {
        let queue: Arc<Queue<usize>> = Default::default();
        let mut reader = queue.reader();
        //let mut writer = queue.writer();
        
        const COUNT: usize = BLOCK_SIZE * 4; 
        for i in 0..COUNT {
            queue.blocking_push(i);
            //writer.push(i);    
        }
        
        let mut vec = Vec::new();
        while let Some(value) = reader.next() {
            //println!("{value}");
            vec.push(value.clone());
        }
        assert_equal(vec, 0..COUNT);
    }
    
    // TODO: Fuzzy test version of this with variable readers/writers count.
    //       And variable read/write count as well. 
    #[test]
    fn test_mpmc_mt() {
        let queue: Arc<Queue<usize>> = Default::default();
        let mut reader0 = queue.reader();
        let mut reader1 = queue.reader();
        let mut writer0 = queue.writer();
        let mut writer1 = queue.writer();
        
        let mut joins = Vec::new();
        
        const COUNT: usize = BLOCK_SIZE*8 + 200;
        
        joins.push(std::thread::spawn(move || {
            for i in 0..COUNT /2  {
                writer0.push(i);
            }
        }));
        
        joins.push(std::thread::spawn(move || {
            for i in COUNT/2..COUNT{
                writer1.push(i);
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