//! Write from multiple threads, read from multiple threads.
//! 
//! Example from readme

use chute::LendingReader;

fn main() {
    const WRITERS         : usize = 4;
    const WRITER_MESSAGES : usize = 100;
    const MESSAGES        : usize = WRITERS*WRITER_MESSAGES;
    const READERS         : usize = 4;
    let queue = chute::mpmc::Queue::new();
    
    std::thread::scope(|s| {
        // READ threads
        for _ in 0..READERS {
            let mut reader = queue.reader();
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
            let mut writer = queue.writer();
            s.spawn(move || {
                for i in 0..WRITER_MESSAGES {
                    writer.push(t*WRITER_MESSAGES + i);
                }             
            });
        }
    });
}