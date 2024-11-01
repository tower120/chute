//! Write from multiple threads, read from multiple threads using spmc::Queue with mutex.
//! 
//! This is faster than mpmc version, when "writers" do not write simultaneously.

use std::sync::Arc;
use chute::{LendingIterator};

fn main() {
    const WRITERS         : usize = 4;
    const WRITER_MESSAGES : usize = 100;
    const MESSAGES        : usize = WRITERS*WRITER_MESSAGES;
    const READERS         : usize = 4;
    let queue: Arc<spin::Mutex<chute::spmc::Queue<_>>> = Default::default();
    
    std::thread::scope(|s| {
        // READ threads
        for _ in 0..READERS {
            let mut reader = queue.lock().reader();
            s.spawn(move || {
                let mut sum = 0;
                for _ in 0..MESSAGES {
                    // Wait for the next message.
                    let msg = loop {
                        if let Some(msg) = reader.next() {
                            break msg;
                        }
                    };
                    sum += msg;
                }
                
                assert_eq!(sum, (0..MESSAGES).sum());
            });
        }        
        
        // WRITE threads
        for t in 0..WRITERS {
            let mut queue = queue.clone();
            s.spawn(move || {
                for i in 0..WRITER_MESSAGES {
                    queue.lock().push(t*WRITER_MESSAGES + i);
                }             
            });
        }
    });
}