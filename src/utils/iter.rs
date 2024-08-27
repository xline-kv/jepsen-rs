use std::pin::Pin;

use tokio_stream::{Stream, StreamExt};
/// The extra methods on iterators.
pub trait ExtraStreamExt: Stream {
    /// Splits the iterator at `n`, returns the splited iterators.
    fn split_at(
        mut self: Pin<&mut Self>,
        n: usize,
    ) -> impl std::future::Future<Output = Vec<Self::Item>> + Send
    where
        Self: Sized + Send,
        Self::Item: Send,
    {
        async move {
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
}

impl<S: Stream> ExtraStreamExt for S {}

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
