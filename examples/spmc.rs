//! Write from one thread, read from multiple threads.

use chute::LendingReader;

fn main() {
    const MESSAGES : usize = 400;
    const READERS  : usize = 4;
    let mut queue = chute::spmc::Queue::new();
    
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
        
        // WRITE thread
        s.spawn(|| {
            for i in 0..MESSAGES {
                queue.push(i);
            }             
        });
    });
}