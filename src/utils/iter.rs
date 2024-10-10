use std::{fmt::Debug, pin::Pin};

use log::trace;
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
    fn id(&self) -> u64;
    async fn next(&mut self) -> Option<Self::Item>;
    async fn next_with_id(&mut self) -> Option<(Self::Item, u64)> {
        self.next().await.map(|x| (x, self.id()))
    }
}

/// A trait for generator, which allows to get next op and delay strategy
/// separately, without actually wait the delay.
#[async_trait::async_trait]
pub trait DelayAsyncIter: AsyncIter {
    type DelayType;
    /// Get next op and delay type without delay.
    async fn get_without_delay(&mut self) -> Option<(Self::Item, Self::DelayType)>;
    /// Collect items only without delay.
    async fn collect(mut self) -> Vec<Self::Item>
    where
        Self: Send + Sized,
        Self::Item: Send + Debug,
    {
        let mut items = Vec::new();
        while let Some((item, _delay)) = self.get_without_delay().await {
            trace!("generator yields {:?}", item);
            items.push(item);
        }
        items
    }
    /// Collect (item, delay)
    async fn collect_all(mut self) -> Vec<(Self::Item, Self::DelayType)>
    where
        Self: Send + Sized,
        Self::Item: Send,
        Self::DelayType: Send,
    {
        let mut items = Vec::new();
        while let Some(x) = self.get_without_delay().await {
            items.push(x);
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
