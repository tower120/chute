//! Unbounded unicast spmc.

use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::{cmp, mem};
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{fence, Ordering};
use branch_hints::{likely, unlikely};
use crate::unicast::read_guard::{ReadGuard, ReadSessionGuard/*, SliceReadGuard, SliceReadSessionGuard*/};
use super::block::{Block, BLOCK_SIZE};

struct QueueSharedData<T>{
    read_block: spin::Mutex<Arc<Block<T>>>,
}

pub struct Queue<T> {
    shared_data: Arc<QueueSharedData<T>>,
    write_block: Arc<Block<T>>,              // aka "last_block"
    write_block_mem: *mut T,
}

unsafe impl<T> Send for Queue<T> {}
unsafe impl<T> Sync for Queue<T> {}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        let write_block: Arc<Block<T>> = Default::default();
        let write_block_mem = unsafe{ write_block.mem_unchecked().cast_mut() };
        Self{
            shared_data: Arc::new(QueueSharedData { 
                read_block : write_block.clone().into(),
            }),
            write_block,
            write_block_mem
        } 
    }
}

impl<T> Queue<T> {
    #[inline]
    pub fn new() -> Self{
        Self::default()    
    }
    
    #[inline]
    pub fn push(&mut self, value: T) {
        let mut block = self.write_block.deref();
        let mut len = block.write_counter.load(Ordering::Relaxed);
        if unlikely(len == BLOCK_SIZE) {
            // Cold function has no effect here.
            let new_block = Arc::new(Block::default());
            *self.write_block.next.lock() = Some(new_block.clone());
            self.write_block_mem = unsafe{ new_block.mem_unchecked().cast_mut() };
            self.write_block = new_block;
            block = self.write_block.as_ref();
            len = 0;
        }
        
        unsafe{ self.write_block_mem.add(len).write(value); }

        // This is necessary for reader to see changes in block data.
        block.write_counter.store(len+1, Ordering::Release);
    }
    
    #[inline]
    pub fn reader(&self) -> Reader<T> {
        let queue_shared_data = self.shared_data.clone(); 
        let block = queue_shared_data.read_block.lock().clone();
        let block_mem = unsafe{ block.mem_unchecked() }; 
        Reader{
            write_counter: block.write_counter.load(Ordering::Acquire),
            block,
            block_mem,
            queue_shared_data,
        }
    }
}

pub struct Reader<T> {
    write_counter: usize,
    block: Arc<Block<T>>,
    block_mem: *const T,
    queue_shared_data: Arc<QueueSharedData<T>>,    
}

impl<T> Clone for Reader<T> {
    fn clone(&self) -> Self {
        Self{
            write_counter: self.write_counter,
            block: self.block.clone(),
            block_mem: self.block_mem,
            queue_shared_data: self.queue_shared_data.clone(),
        }
    }
}

unsafe impl<T> Send for Reader<T> {}

impl<T> Reader<T> {
    #[inline]
    fn flush_read_succ(&mut self, read_succ: &mut usize) {
        let read_succ_value = *read_succ;
        if likely(read_succ_value > 0) {
        // TODO: better comparison op?
        if unlikely(self.block.read_succ.fetch_add(read_succ_value, Ordering::Release) == BLOCK_SIZE - read_succ_value) {
            // See Arc::drop implementation, for this fence rationale.
            fence(Ordering::Acquire);
            unsafe{self.block.dealloc_destructed_mem()};
        }
        }
        *read_succ = 0;
    }    
    
    #[inline(always)]
    fn read_next_impl(&mut self, n: Option<usize>, read_succ: Option<NonNull<usize>>) -> Option<(NonNull<T>, usize)> {
        let mut exp = 1;
        let mut read_counter = self.block.read_counter.load(Ordering::Acquire);
        loop{
            if read_counter >= self.write_counter {
                if unlikely(read_counter == BLOCK_SIZE) {
                    let next_block = {
                        // It is highly unlikely that queue will stop EXACTLY at the last element of the block.
                        // So it is OK just to lock spin::mutex every time we get here.
                        //
                        // As an alternative, we can cache read_block_ptr: AtopmicPtr in queue,
                        // and has_next: AtomicBool in block for fast-path checks.                        
                        let mut read_block = self.queue_shared_data.read_block.lock();
                        if Arc::as_ptr(&read_block) != Arc::as_ptr(&self.block) {
                            // Use last read block from the queue
                            read_block.clone()
                        } else {
                            // try to get block from "next"
                            let next_block = self.block.next.lock().take(); 
                            if let Some(next_block) = next_block{
                                // update read_block in queue
                                *read_block = next_block.clone();
                                next_block
                            } else {
                                // Have no more blocks
                                return None;  
                            }
                        }
                    };
                    
                    // Try flushing active read_succ counter, if requested.
                    // Will happen only on the first block switch.
                    if /*constexpr*/ let Some(mut read_succ) = read_succ {
                        self.flush_read_succ(unsafe{ read_succ.as_mut() });
                    }
                    
                    // just read everything again for the new block.
                    read_counter       = next_block.read_counter.load(Ordering::Acquire);
                    self.write_counter = next_block.write_counter.load(Ordering::Acquire);
                    self.block_mem = unsafe{ next_block.mem_unchecked() }; 
                    self.block = next_block;
                    continue;
                } else {
                    let write_counter = self.block.write_counter.load(Ordering::Acquire);
                    if write_counter <= read_counter {
                        return None;
                    }
                    self.write_counter = write_counter;
                }
            }
            
            let read_count = /*constexpr*/ match n {
                None => 1,
                Some(n) => cmp::min(n, self.write_counter - read_counter)
            };
            let result = self.block.read_counter.compare_exchange(read_counter, read_counter+read_count, 
                Ordering::AcqRel,
                Ordering::Acquire);
            match result {
                Ok(_) => {
                    let index = read_counter;
                    let value = unsafe{
                        let ptr = self.block.mem_unchecked().add(index);
                        NonNull::new_unchecked(ptr.cast_mut())
                    };
                    return Some((value, read_count));
                }
                Err(new_read_counter) => {
                    read_counter = new_read_counter 
                }
            };

            for _ in 0..1<<exp {
                core::hint::spin_loop();    
            }
            if exp<10 {
                exp += 1;    
            }
        }
    }
    
    #[inline]
    pub fn session(&mut self) -> ReadSession<'_, T>{
        ReadSession{
            reader: self,
            read_succ: 0,
            phantom_data: PhantomData,
        }
    }
    
    #[inline]
    pub fn next(&mut self) -> Option<ReadGuard<'_, T>>{
        self.read_next_impl(None, None)
            .map(|(value, _)|ReadGuard{ 
                value, 
                block: self.block.as_ref(), 
                phantom_data: PhantomData 
            })
    }
    
    /*#[inline]
    pub fn next_n(&mut self, n: usize) -> Option<SliceReadGuard<'_, T>>{
        self.read_next_impl(Some(n), None)
            .map(|(start, len)|SliceReadGuard{ 
                start, 
                len,
                block: self.block.as_ref(), 
                phantom_data: PhantomData 
            })
    }*/
}

pub struct ReadSession<'a, T>{
    reader: &'a mut Reader<T>,
    read_succ: usize,
    phantom_data: PhantomData<T>
}
impl<'a, T> ReadSession<'a, T>{
    #[inline]
    pub fn next(&mut self) -> Option<ReadSessionGuard<'_, T>>{
        self.reader.read_next_impl(None, Some(NonNull::from(&mut self.read_succ)))
            .map(|(value, _)|ReadSessionGuard{ 
                value, 
                read_succ: &mut self.read_succ, 
                phantom_data: PhantomData 
            })
    }
    
    /*#[inline]
    pub fn next_n(&mut self, n: usize) -> Option<SliceReadSessionGuard<'_, T>>{
        self.reader.read_next_impl(Some(n), Some(NonNull::from(&mut self.read_succ)))
            .map(|(start, len)|SliceReadSessionGuard{ 
                start, 
                len,
                read_succ: &mut self.read_succ, 
                phantom_data: PhantomData 
            })
    }*/
}
impl<'a, T> Drop for ReadSession<'a, T>{
    #[inline]
    fn drop(&mut self) {
        self.reader.flush_read_succ(&mut self.read_succ);
    }
}

#[cfg(test)]
mod test {
    use arrayvec::ArrayVec;
    use super::*;
    use itertools::assert_equal;
    
    #[test]
    fn smoke_test(){
        let mut queue: Queue<usize> = Default::default();
        let mut reader = queue.reader();
        //let mut reader = reader.session();
        
        const COUNT: usize = BLOCK_SIZE*20;
        for i in 0..COUNT{
            queue.push(i);    
        }
        
        let mut vec = Vec::new();
        for _ in 0..COUNT{
            let value = reader.session().next().unwrap().clone();
            vec.push(value);
        }

        assert_equal(vec, 0..COUNT);
    }
    
    #[test]
    fn spsc_test(){
        let mut queue: Queue<String> = Queue::default();
        
        const COUNT: usize = BLOCK_SIZE*2 + 32;
        const THREAD_LEFTOVERS: usize = 1;
        const READ_THREADS: usize = 4;
        
        let mut out: Arc<spin::Mutex<Vec<String>>> = Default::default();
        let mut joins: ArrayVec<_, 64> = Default::default();
        for _ in 0..READ_THREADS {
            let mut out = out.clone();
            let mut reader = queue.reader();
            let rt = std::thread::spawn(move || {
                let mut vec = Vec::new();
                let mut read_session = reader.session();
                for _ in 0..COUNT/READ_THREADS - THREAD_LEFTOVERS {
                    let msg = loop{                         
                        if let Some(msg) = read_session.next(){
                            break msg;
                        } else {
                            std::thread::yield_now();
                        }
                    };
                    vec.push(msg.take());
                }
                out.lock().extend(vec);
            });
            joins.push(rt);
        }
        
        let wt = std::thread::spawn(move || {
            for i in 0..COUNT {
                queue.push(String::from(format!("{i}")));
            }
        });
        
        wt.join().unwrap();
        for join in joins{
            join.join().unwrap();
        }

        /* assert */ {
            let mut out = mem::take(out.lock().deref_mut());
            out.sort_by_key(|s|s.parse::<usize>().unwrap());
            
            let control = (0..COUNT - THREAD_LEFTOVERS*READ_THREADS)
                .map(|i|String::from(format!("{i}")));
            
            assert_equal(out, control);
        }
    }    
}