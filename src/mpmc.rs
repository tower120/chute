//! Multi-producer, multi-consumer.
//! 
//! Thread-safe lockless writers and readers.

use std::ops::Deref;
use std::ptr::{null_mut, NonNull};
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};
use crate::block::{Block, BlockArc, BLOCK_SIZE};
use crate::lending_iterator::LendingIterator;
use crate::reader::LendingReader;

pub struct Queue<T> {
    last_block: AtomicPtr<Block<T>>,
}

impl<T> Default for Queue<T> {
    #[inline]
    fn default() -> Self {
        Self {
            last_block: AtomicPtr::new(Block::new().into_raw().as_ptr()),
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
    /// Latest block SHOULD be non-full. But the statement could fail.
    /// 
    /// Blocking.
    #[must_use]
    #[inline]
    fn insert_block(&self) -> (BlockArc<T>, bool) {
        // 1. Lock
        let last_block = unsafe{ self.lock_last_block().as_mut() };
        
        //if last_block.len.load(Ordering::Acquire) < BLOCK_SIZE {
        if last_block.load_packed(Ordering::Acquire).occupied_len < BLOCK_SIZE as _ {
            let last_block = NonNull::from(last_block);
            
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
        let mut new_block = Block::with_counter(3).into_raw();

        // 3. Connect new block with old
        last_block.next.store(new_block.as_ptr(), Ordering::Release);
        
        // 4. Arc -- old block
        unsafe{
            Block::dec_use_count(last_block.into());
        }
        
        // 5. Set new block as last, and release lock.
        self.unlock_last_block(new_block);

        (unsafe{ BlockArc::from_raw(new_block) }, true)
    }
    
    /// Push value to queue.
    /// 
    /// This is a blocking operation - you can't [push()] simultaneously from
    /// different threads, but most of the time writers can push in parallel
    /// with this call. 
    /// 
    /// A little bit faster than a
    /// constructing writer just to push a single value
    /// `writer().push(..)`. But slower than [Writer::push] itself.
    /// 
    /// Use it if you need to occasionally push a single value.
    #[inline]
    pub fn push(&self, value: T){
        let block = self.lock_last_block();
        if let Err(value) = unsafe{ block.as_ref() }.try_push(value) {
            #[cold]
            #[inline(never)]
            fn insert_block_and_push<T>(this: &Queue<T>, value: T){
                let (block, inserted) = this.insert_block();
                if !inserted{
                    unsafe{ std::hint::unreachable_unchecked() }
                }
                let result = block.try_push(value);
                if result.is_err(){
                    unsafe{ std::hint::unreachable_unchecked() }
                }
            }
            insert_block_and_push(self, value);
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
    
    #[must_use]
    #[inline]
    pub fn lending_reader(&self) -> LendingReader<T> {
        let last_block = self.load_last_block();
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
impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        let last_block = self.last_block.load(Ordering::Acquire);
        unsafe{
            let arc = BlockArc::from_raw(NonNull::new_unchecked(last_block));
            drop(arc);
        }
    }
}

/// Queue writer.
///
/// Same as reader, writer internally keeps a block pointer. 
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
    /// Moves writer's internal block pointer to the latest in queue.
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

    // TODO: return something, signaling that queue len was increased.
    #[inline]
    pub fn push(&mut self, value: T) {
        let inserted = self.block.try_push(value);
        if let Err(value) = inserted {
            self.insert_block_and_push(value);
        }
    }
}

#[cfg(test)]
mod test_mpmc{
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use crate::lending_iterator::LendingIterator;
    use crate::mpmc::Queue;

    #[test]
    fn test_event_queue() {
        let queue: Arc<Queue<usize>> = Default::default();
        let mut reader = queue.lending_reader();
        let mut writer = queue.writer();
        
        for i in 0..16{
            writer.push(i);    
        }
        
        while let Some(value) = reader.next() {
            println!("{value}");
        }
    }
    
    #[test]
    fn test_event_queue_mt() {
        let queue: Arc<Queue<usize>> = Default::default();
        let mut reader0 = queue.lending_reader();
        let mut reader1 = queue.lending_reader();
        let mut writer0 = queue.writer();
        let mut writer1 = queue.writer();
        
        let mut joins = Vec::new();
        
        joins.push(std::thread::spawn(move || {
            for i in 0..2000{
                writer0.push(i);
                //std::thread::yield_now();
            }
        }));
        
        joins.push(std::thread::spawn(move || {
            for i in 2000..4000{
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
                        if i == 4000 {
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
                        if i == 4000 {
                            break;
                        }
                    }
                }
                rs1.store(sum, Ordering::Release);
            }));
        }
        
        for join in joins{
            join.join();    
        }
        
        println!("s0 = {:}", rs0.load(Ordering::Acquire));
        println!("s1 = {:}", rs1.load(Ordering::Acquire));
    }    
}