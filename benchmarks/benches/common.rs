pub const COUNT: usize = 200_000;

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

#[inline]
pub fn yield_fn() {
    std::thread::yield_now();
}