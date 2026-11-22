mod mpmc;
mod spmc;

#[derive(Clone, PartialEq)]
pub struct StringWrapper(String);
impl From<usize> for StringWrapper{
    fn from(value: usize) -> Self {
        Self(String::from(format!("{value}")))
    }
}
impl From<StringWrapper> for usize{
    fn from(value: StringWrapper) -> Self {
        value.0.parse().unwrap()
    }
}