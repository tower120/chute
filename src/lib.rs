//! Multi-producer multi-consumer lock-free FIFO queue. Each consumer see its
//! own queue.
//!
//! Memory-wise all consumers works with the same shared data blocks, so there 
//! is no duplication.
//!
//! Read performance is **stellar** - there is only one atomic read per block.
//! Most of the time - this is just plain continuous data reading.
//! 
//! # [spmc] vs [mpmc]
//!
//! Both, [spmc] and [mpmc] can work in multi-producer mode.
//! 
//! In general, using `Arc<spin::Mutex<spmc::Queue<T>>>` is more performant
//! then `Arc<mpmc::Queue<T>>` from writer perspective.
//!
//! But! Writing simultaneously from several threads is faster with [mpmc].
//! TODO: See benchmarks
//! 
//! Read performance identical.
//! 
//! # How it works
//! 
//! TODO: move section to readme and copy from rc_event_queue.

mod block;

pub mod mpmc;
pub mod spmc;

mod lending_iterator;
pub use lending_iterator::*;

mod reader;
pub use reader::*;