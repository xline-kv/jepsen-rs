use std::{fmt, sync::Arc};

use log::trace;

/// Trait to get id and global from [`Generator`] or [`GeneratorGroup`].
///
/// Methods in this trait will be used in [`AsyncIter`] and [`DelayAsyncIter`].
pub trait GeneratorItemGetter {
    type G;
    fn id(&self) -> u64;
    fn global(&self) -> &Arc<Self::G>;
}

/// A trait of `async fn next()`, implements to Generator(Group).
#[async_trait::async_trait]
pub trait AsyncIter {
    type Item;
    async fn next(&mut self) -> Option<Self::Item>;
}

/// Trait to get the next [`Op`] with generator id.
#[async_trait::async_trait]
pub trait GeneratorIter: AsyncIter + GeneratorItemGetter {
    async fn next_with_id(&mut self) -> Option<(Self::Item, u64)> {
        self.next().await.map(|x| (x, self.id()))
    }
}

/// A trait for generator, which allows to get next op and delay strategy
/// separately, without actually wait the delay.
#[async_trait::async_trait]
pub trait DelayAsyncIter: GeneratorIter {
    type DelayType;
    /// Get next op and delay type without delay.
    async fn get_without_delay(&mut self) -> Option<(Self::Item, Self::DelayType)>;
    /// Collect items only without delay.
    async fn collect(mut self) -> Vec<Self::Item>
    where
        Self: Send + Sized,
        Self::Item: Send + fmt::Debug,
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
