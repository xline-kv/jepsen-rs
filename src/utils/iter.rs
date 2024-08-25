/// The extra methods on iterators.
pub trait IteratorExt: Iterator {
    /// Splits the iterator at `n`, returns the splited iterators.
    fn split_at(&mut self, n: usize) -> std::vec::IntoIter<Self::Item>
    where
        Self: Sized,
    {
        let mut buffer = Vec::with_capacity(n);
        for _ in 0..n {
            if let Some(x) = self.next() {
                buffer.push(x);
            } else {
                break;
            }
        }
        buffer.into_iter()
    }
}
impl<I: Iterator> IteratorExt for I {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_at() {
        let mut v = 1..=5;
        let a = v.split_at(3);
        assert_eq!(a.collect::<Vec<i32>>(), vec![1, 2, 3]);
        assert_eq!(v.collect::<Vec<i32>>(), vec![4, 5]);
    }
}
