//! Multi-producer multi-consumer lock-free multicast queue[^multicast].
//!
//! [^multicast]: Or broadcast queue. Each consumer gets every message sent
//! to queue, from the moment of subscription.
//!
//! Memory-wise all consumers work with the same shared data blocks, so there
//! is no duplication. Queue is unbounded - it grows and shrinks dynamically as needed.
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
//! But! Writing simultaneously from several threads is much faster with [mpmc].
//!
//! The read performance is almost equal, with a slight advantage for [spmc].
//!
//! # [`Sync`] / [`Send`] + [`Clone`] requirements
//!
//! Your `T` is required to be [`Sync`] to be usable with [`LendingReader`]
//! in other threads.
//!
//! Or [`Send`] + [`Clone`] to be usable with [`CloningReader`].
//! You can get [`CloningReader`] from [`LendingReader`] before sending
//! it to another thread:
//! ```
//! # use chute::spmc::*;
//! let queue: Queue<usize> = Default::default();
//! let lending_reader: LendingReader<_> = queue.reader();
//! let cloning_reader: CloningReader<_> = lending_reader.cloned();
//! ```
//!
//! Also `T` must be [`Send`] in order for writers to be [`Send`]/[`Sync`]-able.
//!
//! # [`LendingIterator`] -> [`Iterator`]
//!
//! [`LendingReader`] is [`LendingIterator`], while [`CloningReader`] is [`Iterator`].
//! You may want to use [`CloningReader`] purely for ergonomic reasons.
//!
//! [`LendingReader`]: spmc::LendingReader
//! [`CloningReader`]: spmc::CloningReader
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

mod lending_iterator;
pub use lending_iterator::*;

#[cfg(test)]
mod tests;