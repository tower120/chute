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
//! Both [spmc] and [mpmc] can work in multi-producer mode.
//! 
//! In general, using `Arc<spin::Mutex<spmc::Queue<T>>>` is more performant
//! than `Arc<mpmc::Queue<T>>` from writer perspective.
//! But! Writing simultaneously from several threads is faster with [mpmc].
//! 
//! Read performance identical.
//! 
//! # Order
//! 
//! [spmc] is fully ordered. 
//! 
//! [mpmc] ordered within writer's messages. Which means that all messages from
//! the same [Writer] will arrive in order.
//! 
//! [Writer]: mpmc::Writer
//! 
//! # Reader memory
//! 
//! All readers have something like Arc for its current block in shared queue.
//! This means that each reader prevents an unread portion of a queue from being dropped.
//! 
//! # target-flags
//! 
//! [mpmc] use [trailing_ones()]. So you want to have hardware support for it.
//! On x86 you need `BMI1`, there is analog on each cpu architecture.
//!
//! [trailing_ones()]: u64::trailing_ones 

mod block;

pub mod mpmc;
pub mod spmc;

mod reader;
pub use reader::*;