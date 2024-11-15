//! Unbounded unicast spmc.

use std::cell::UnsafeCell;
use std::mem;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use branch_hints::unlikely;
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
            let mem = block.mem().cast_mut();
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

pub struct Reader<T> {
    write_counter: usize,
    block: Arc<Block<T>>,
    queue_shared_data: Arc<QueueSharedData<T>>,    
}
unsafe impl<T> Send for Reader<T> {}

impl<T: Default + Clone> Reader<T> {
    // TODO: return wrapper that can take T, but returns &/&mut T by default.
    #[inline]
    pub fn next(&mut self) -> Option<&T>{
        let mut read_counter = self.block.read_counter.load(Ordering::Acquire);
        loop{
            if read_counter == self.write_counter {
                if unlikely(read_counter == BLOCK_SIZE) {
                    // It is highly unlikely that queue will stop EXACTLY at the last element of the block.
                    // So it is OK just to lock spin::mutex every time we get here.
                    //
                    // As an alternative, we can cache read_block_ptr: AtopmicPtr in queue,
                    // and has_next: AtomicBool in block for fast-path checks.
                    let mut read_block = self.queue_shared_data.read_block.lock();
                    if Arc::as_ptr(&read_block) != Arc::as_ptr(&self.block){
                        // Use block from queue
                        let read_block_ptr = read_block.clone();
                        drop(read_block);
                        self.block = read_block_ptr;
                    } else {
                        // try to get block from "next"
                        let next_block = self.block.next.lock().take(); 
                        if let Some(next_block) = next_block{
                            // update read_block in queue
                            *read_block = next_block.clone();
                            drop(read_block);
                            self.block = next_block;
                        } else {
                            // Have no more blocks
                            return None;  
                        }
                    }
                    // just read everything again for new block.
                    read_counter = self.block.read_counter.load(Ordering::Acquire);
                    continue;
                } else {
                    let write_counter = self.block.write_counter.load(Ordering::Acquire);
                    if write_counter == self.write_counter {
                        return None;
                    }
                    self.write_counter = write_counter;
                }
            }

            // Can be weak as well - but looks like there is no difference.
            let result = self.block.read_counter.compare_exchange(read_counter, read_counter+1, 
                Ordering::AcqRel,
                Ordering::Acquire);
            match result {
                Ok(_) => {
                    let index = read_counter;
                    let value = unsafe{ &*self.block.mem().add(index) };
                    return Some(value);    
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
        
        const COUNT: usize = BLOCK_SIZE*2;
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
}