//! Unbounded unicast spmc.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use branch_hints::{likely, unlikely};
use super::block::{BLOCK_SIZE, Block};

#[derive(Default)]
struct QueueSharedData<T>{
    read_block : spin::Mutex<Arc<Block<T>>>,
}

pub struct Queue<T> {
    shared_data: Arc<QueueSharedData<T>>,
    write_block: Arc<Block<T>>,              // aka "last_block"
}

unsafe impl<T> Send for Queue<T> {}
unsafe impl<T> Sync for Queue<T> {}

impl<T: Default> Default for Queue<T> {
    fn default() -> Self {
        let block = Arc::new(Block::default());
        Self{
            shared_data: Arc::new(QueueSharedData { read_block: block.clone().into() }),
            write_block: block,
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
            self.write_block = new_block;
            block = self.write_block.as_ref();
            len = 0;
        }
        
        unsafe{
            // TODO: cache mem pointer
            let mem = block.mem_unchecked().cast_mut();
            mem.add(len).write(value);
        }
        
        block.write_counter.store(len+1, Ordering::Release);
    }
    
    pub fn reader(&self) -> Reader<T>{
        let queue_shared_data = self.shared_data.clone(); 
        let block = queue_shared_data.read_block.lock().clone();
        Reader{
            write_counter: block.write_counter.load(Ordering::Acquire),
            block,
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
    value: *mut T,
    block_to_drop: Option<&'a Block<T>>
}
impl<'a, T> ReadGuard<'a, T>{
    #[inline]
    pub fn take(self) -> T {
        unsafe{ std::ptr::read(self.value) }
    }
}
impl<'a, T> Deref for ReadGuard<'a, T>{
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe{ &*self.value }
    }
}
impl<'a, T> DerefMut for ReadGuard<'a, T>{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe{ &mut*self.value }
    }
}
impl<'a, T> Drop for ReadGuard<'a, T>{
    #[inline]
    fn drop(&mut self) {
        // 1. Drop value
        unsafe {
            std::ptr::drop_in_place(self.value);
        }
        
        // 2. Drop block's mem, if needed.
        if likely(self.block_to_drop.is_none()){
            return;
        }
        unsafe{ 
            self.block_to_drop.unwrap_unchecked()
                .dealloc_destructed_mem(); 
        }
    }
}

pub struct Reader<T> {
    write_counter: usize,
    block: Arc<Block<T>>,
    queue_shared_data: Arc<QueueSharedData<T>>,    
}
unsafe impl<T> Send for Reader<T> {}

impl<T: Default + Clone> Reader<T> {
    #[inline]
    pub fn next(&mut self) -> Option<ReadGuard<'_, T>>{
        let mut read_counter = self.block.read_counter.load(Ordering::Acquire);
        loop{
            if read_counter == self.write_counter {
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
                    self.block = next_block;
                    continue;
                } else {
                    let write_counter = self.block.write_counter.load(Ordering::Acquire);
                    if write_counter == self.write_counter {
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
                    // TODO: Cache mem pointer
                    let value = unsafe{ self.block.mem_unchecked().cast_mut().add(index) };
                    
                    return Some(
                        ReadGuard{
                            value,
                            block_to_drop: 
                                // Only one thread can get here, and only once per block.
                                if unlikely(index == BLOCK_SIZE-1) { 
                                    Some(self.block.as_ref()) 
                                } else {
                                    None
                                },
                        }
                    );    
                }
                Err(new_read_counter) => { read_counter = new_read_counter }
            };
        }
    }
}

#[cfg(test)]
mod test {
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
    
    
    pub mod message {
        use std::fmt;
    
        const LEN: usize = 4;
    
        #[derive(Default, Clone, Copy)]
        pub struct Message(#[allow(dead_code)] [usize; LEN]);
        
        impl fmt::Debug for Message {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.pad("Message")
            }
        }
        
        #[inline]
        pub fn new(num: usize) -> Message {
            Message([num; LEN])
        }    
    }    
    
    #[test]
    fn spsc_test(){
        let mut queue = Queue::default();
        let mut reader = queue.reader();
        const COUNT: usize = BLOCK_SIZE*2 + 31;
        
        let wt = std::thread::spawn(move || {
            for i in 0..COUNT {
                queue.push(/*message::new(i)*/String::from(format!("{i}")));
            }
        });
    
        let rt = std::thread::spawn(move || {
            for _ in 0..COUNT {
                loop{
                    if let None = reader.next(){
                        std::thread::yield_now();
                    } else {
                        break;
                    }
                }
            }
        });
        
        wt.join().unwrap();
        rt.join().unwrap();
    }    
}