/// Simple GAT lending iterator trait.
/// 
/// We expect it to be mainly used with reader in this way: 
/// ```
/// # let queue: chute::spmc::Queue<usize> = Default::default();
/// # let mut reader = queue.reader();
/// # use chute::LendingIterator;
/// while let Some(value) = reader.next(){
///     // Do something
/// }
/// ``` 
/// 
/// # Design choices
/// 
/// We're aware of GAT-style LendingIterator problems and limitations.
/// We decided to stick with it for the sake of simplicity.
pub trait LendingIterator {
    type Item<'a> where Self: 'a;
    
    fn next(&mut self) -> Option<Self::Item<'_>>;
}