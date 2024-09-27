use std::pin::Pin;

use tokio_stream::{Stream, StreamExt};

/// The extra methods on async iterators (Stream).
#[async_trait::async_trait]
pub trait ExtraStreamExt: Stream {
    /// Splits the iterator at `n`, returns the splited iterators.
    async fn split_at(mut self: Pin<&mut Self>, n: usize) -> Vec<Self::Item>
    where
        Self: Send,
        Self::Item: Send,
    {
        let mut buffer = Vec::with_capacity(n);
        for _ in 0..n {
            if let Some(x) = self.next().await {
                buffer.push(x);
            } else {
                break;
            }
        }
        buffer
    }
}

impl<S: ?Sized + Stream> ExtraStreamExt for S {}

/// A trait of `async fn next()`, implements to Generator(Group).
#[async_trait::async_trait]
pub trait AsyncIter {
    type Item;
    async fn next(&mut self) -> Option<Self::Item>;
    async fn next_with_id(&mut self) -> Option<(Self::Item, u64)>;
    async fn collect(mut self) -> Vec<Self::Item>
    where
        Self: Send + Sized,
        Self::Item: Send,
    {
        let mut items = Vec::new();
        while let Some(item) = self.next().await {
            items.push(item);
        }
        items
    }
}

#[cfg(test)]
mod tests {
    use std::pin::pin;

    use super::*;

    #[madsim::test]
    async fn test_split_at() {
        let v = pin!(tokio_stream::iter(1..=5));
        let a = v.split_at(3).await;
        assert_eq!(a, vec![1, 2, 3]);
    }
}
