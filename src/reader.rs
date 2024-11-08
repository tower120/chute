// TODO: next_slice()
/// Lending queue consumer trait.
/// 
/// LendingReader returns `&T` with `&mut self` lifetime. This means you should deal 
/// with message BEFORE consuming the next one. 
/// Because of this, it does not implement [Iterator]. But [ClonedReader] does.
/// 
/// We expect it to be mainly used with reader in this way: 
/// ```
/// # let queue: chute::spmc::Queue<usize> = Default::default();
/// # let mut reader = queue.reader();
/// # use chute::LendingReader;
/// while let Some(value) = reader.next() {
///     // Do something
/// }
/// ``` 
/// 
/// # Design choices
/// 
/// The value returned by the reader lives as long as the block where it is stored.
/// From the reader's point of view, we can guarantee that the value remains valid
/// as long as the block does not change. However, in Rust, we cannot make such 
/// granular guarantees. Instead, we guarantee that the value remains valid until the
/// reader is mutated. This means the value is guaranteed to live until the next 
/// read operation, at which point the block may change, and the old block could 
/// be destructed.
pub trait LendingReader: Sized {
    type Item;
    
    fn next(&mut self) -> Option<&Self::Item>;
    
    #[inline]
    fn cloned(self) -> ClonedReader<Self> {
        ClonedReader{reader: self}
    } 
}

/// Cloning queue consumer.
/// 
/// Reader that clones `T` upon return. Implements [Iterator].
///
/// Constructed by [LendingReader::cloned()]. 
pub struct ClonedReader<R: LendingReader>{
    reader: R   
}
impl<R> Iterator for ClonedReader<R>
where
    R: LendingReader<Item: Clone>
{
    type Item = R::Item;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.reader.next().cloned()
    }
}