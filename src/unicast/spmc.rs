//! Unbounded unicast spmc.

use std::marker::PhantomData;
use std::mem::{ManuallyDrop};
use std::ops::{Deref, DerefMut};
use std::{mem, ptr};
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{fence, Ordering};
use branch_hints::{unlikely};
use super::block::{BLOCK_SIZE, Block};

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

impl<T: Default> Default for Queue<T> {
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
    
    pub fn reader(&self) -> Reader<T>{
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

/// Owning [Queue] message wrapper.
/// 
/// # Design choices
/// 
/// We could just always return a value, but that would require mempcy for
/// every read message. With ReadGuard we basically give you a reference, that 
/// can be [take]n. 
pub struct ReadGuard<'a, T>{
    value: NonNull<T>,
    block: &'a Block<T>,
    // We own T
    phantom_data: PhantomData<T>
}
impl<'a, T> ReadGuard<'a, T>{
    #[inline]
    pub fn mark_readed(&mut self) {
        if unlikely(self.block.read_succ.fetch_add(1, Ordering::Release) == BLOCK_SIZE-1) {
            // See Arc::drop implementation, for this fence rationale.
            fence(Ordering::Acquire);
            unsafe{self.block.dealloc_destructed_mem()};
        }
    }
    
    #[inline]
    pub fn take(self) -> T {
        let mut this = ManuallyDrop::new(self);
        let value = unsafe{ this.value.read() };
        this.mark_readed();
        value
    }
}
impl<'a, T> Deref for ReadGuard<'a, T>{
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe{ self.value.as_ref() }
    }
}
impl<'a, T> DerefMut for ReadGuard<'a, T>{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe{ self.value.as_mut() }
    }
}
impl<'a, T> Drop for ReadGuard<'a, T>{
    #[inline]
    fn drop(&mut self) {
        // 1. Drop value
        unsafe {
            ptr::drop_in_place(self.value.as_ptr());
        }
        
        // 2. Drop block's mem, if needed.
        self.mark_readed();
    }
}

pub struct Reader<T> {
    write_counter: usize,
    block: Arc<Block<T>>,
    block_mem: *const T,
    queue_shared_data: Arc<QueueSharedData<T>>,    
}
unsafe impl<T> Send for Reader<T> {}

impl<T: Default + Clone> Reader<T> {
    #[inline]
    pub fn next(&mut self) -> Option<ReadGuard<'_, T>>{
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

            let result = self.block.read_counter.compare_exchange(read_counter, read_counter+1, 
                Ordering::AcqRel,
                Ordering::Acquire);
            match result {
                Ok(_) => {
                    let index = read_counter;
                    let value = unsafe{
                        let ptr = self.block.mem_unchecked().add(index);
                        NonNull::new_unchecked(ptr.cast_mut())
                    };
                    return Some(ReadGuard{ value, block: self.block.as_ref(), phantom_data: Default::default() });
                }
                Err(new_read_counter) => {
                    read_counter = new_read_counter 
                }
            };

            for _ in 0..1<<exp {
                std::hint::spin_loop();    
            }
            if exp<10 {
                exp += 1;    
            }
        }
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
        
        const COUNT: usize = BLOCK_SIZE*20;
        for i in 0..COUNT{
            queue.push(i);    
        }
        
        let mut vec = Vec::new();
        for _ in 0..COUNT{
            let value = reader.next().unwrap().clone();
            vec.push(value);
        }

        assert_equal(vec, 0..COUNT);
    }
    
    #[test]
    fn spsc_test(){
        let mut queue: Queue<String> = Queue::default();
        
        const COUNT: usize = BLOCK_SIZE *2 + 32;
        const THREAD_LEFTOVERS: usize = 1;
        const READ_THREADS: usize = 4;
        
        let mut out: Arc<spin::Mutex<Vec<String>>> = Default::default();
        let mut joins: ArrayVec<_, 64> = Default::default();
        for _ in 0..READ_THREADS {
            let mut out = out.clone();
            let mut reader = queue.reader();
            let rt = std::thread::spawn(move || {
                let mut vec = Vec::new();
                for _ in 0..COUNT/READ_THREADS - THREAD_LEFTOVERS {
                    loop{
                        if let Some(msg) = reader.next(){
                            vec.push(msg.take());
                            break;
                        } else {
                            std::thread::yield_now();
                        }
                    }
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