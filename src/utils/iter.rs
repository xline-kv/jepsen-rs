use std::{fmt::Debug, pin::Pin, sync::Arc};

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
