//! Multi-producer multi-consumer lock-free multicast[^multicast] queue. 
//! 
//! [^multicast]: Each consumer gets every message sent to queue, from the moment of subscription.
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
//! than `Arc<mpmc::Queue<T>>` from writer perspective.
//! But! Writing simultaneously from several threads is faster with [mpmc].
//! 
//! Read performance identical.

mod block;

pub mod mpmc;
pub mod spmc;

mod lending_iterator;
pub use lending_iterator::*;

mod reader;
pub use reader::*;