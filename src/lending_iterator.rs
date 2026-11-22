// TODO: next_slice()

/// Iterator-like trait, that returns `&T` with `&mut self` lifetime.
///
/// This means you should deal with item BEFORE consuming the next one.
///
/// # Example
///
/// ```
/// # let queue: chute::spmc::Queue<usize> = Default::default();
/// # let mut reader = queue.reader();
/// # use chute::LendingIterator;
/// while let Some(value) = reader.next() {
///     // Do something
/// }
/// ```
///
/// # Design choices
///
/// The value returned by the reader lives as long as the block where it is stored.
/// Each `Reader` holds one currently read block - as soon as it reaches its end -
/// it releases the block, and takes the next one (non-active blocks may be destructed).
/// This can happen at each `next()` call.
/// So we can only guarantees that item lives only until the next `next()` call.
pub trait LendingIterator{
    type Item;
    fn next(&mut self) -> Option<&Self::Item>;
}