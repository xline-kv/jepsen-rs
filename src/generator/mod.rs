pub mod context;
pub mod controller;
pub mod elle_rw;
#[cfg(test)]
use std::ops::{AddAssign, RangeFrom};
use std::{pin::Pin, sync::Arc};

use anyhow::Result;
use context::GeneratorId;
pub use context::Global;
use controller::{DelayStrategy, GeneratorGroupStrategy};
use tokio_stream::{Stream, StreamExt};

use crate::{
    history::ErrorType,
    op::Op,
    utils::{AsyncIter, ExtraStreamExt},
};

/// Cache size for the generator.
pub const GENERATOR_CACHE_SIZE: usize = 200;

/// This trait is for the raw generator (clojure generator), which will only
/// generate items *infinitely*.
pub trait RawGenerator {
    type Item;
    fn gen(&mut self) -> Self::Item;
    fn gen_n(&mut self, n: usize) -> Vec<Self::Item> {
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.gen());
        }
        out
    }
}

impl<U> Iterator for dyn RawGenerator<Item = U> {
    type Item = <Self as RawGenerator>::Item;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.gen())
    }
}

#[cfg(test)]
impl RawGenerator for RangeFrom<i32> {
    type Item = i32;
    fn gen(&mut self) -> Self::Item {
        let temp = self.start;
        self.start.add_assign(1);
        temp
    }
}

/// The generator. It's a wrapper for the clojure seq and global context.
pub struct Generator<'a, U: Send = Result<Op>, ERR: Send + 'a = ErrorType> {
    /// generator id
    pub id: GeneratorId,
    /// A reference to the global context
    pub global: Arc<Global<'a, U, ERR>>,
    /// The sequence (stream) of generator. Note that the seq is finite.
    pub seq: Pin<Box<dyn Stream<Item = U> + Send + 'a>>,
    /// The delay strategy between every `next()` function
    pub delay_strategy: DelayStrategy,
}

impl<'a, U: Send + 'a, ERR: 'a + Send> Generator<'a, U, ERR> {
    pub async fn new(
        global: Arc<Global<'a, U, ERR>>,
        seq: impl Stream<Item = U> + Send + 'a,
    ) -> Self {
        let id = global.get_id().await;
        Self {
            id,
            global,
            seq: Box::pin(seq),
            delay_strategy: DelayStrategy::default(),
        }
    }

    pub fn new_with_id(
        id: GeneratorId,
        global: Arc<Global<'a, U, ERR>>,
        seq: impl Stream<Item = U> + Send + 'a,
    ) -> Self {
        Self {
            id,
            global,
            seq: Box::pin(seq),
            delay_strategy: DelayStrategy::default(),
        }
    }

    pub fn new_with_pined_seq(
        id: GeneratorId,
        global: Arc<Global<'a, U, ERR>>,
        seq: Pin<Box<dyn Stream<Item = U> + Send + 'a>>,
    ) -> Self {
        Self {
            id,
            global,
            seq,
            delay_strategy: DelayStrategy::default(),
        }
    }

    pub fn set_delay(&mut self, delay: DelayStrategy) {
        self.delay_strategy = delay;
    }

    pub fn map(self, f: impl Fn(U) -> U + Send + 'a) -> Self {
        Generator::new_with_id(self.id, self.global, self.seq.map(f))
    }

    pub fn filter(self, f: impl Fn(&U) -> bool + Send + 'a) -> Self {
        Generator::new_with_id(self.id, self.global, self.seq.filter(f))
    }

    pub fn take(self, n: usize) -> Self {
        Generator::new_with_id(self.id, self.global, self.seq.take(n))
    }

    pub async fn split_at(mut self, n: usize) -> (Self, Self) {
        let first = self.seq.as_mut().split_at(n).await;
        (
            Generator::new_with_id(self.id, Arc::clone(&self.global), tokio_stream::iter(first)),
            Generator::new_with_pined_seq(self.global.get_id().await, self.global, self.seq),
        )
    }

    pub fn chain(self, other: Self) -> Self {
        let out = self.seq.chain(other.seq);
        Generator::new_with_id(self.id, self.global, out)
    }
}

impl<'a, ERR: 'a + Send, U: Send + 'a> AsyncIter for Generator<'a, U, ERR> {
    type Item = U;
    async fn next(&mut self) -> Option<Self::Item> {
        self.delay_strategy.delay().await;
        self.seq.next().await
    }
    async fn next_with_id(&mut self) -> Option<(Self::Item, u64)> {
        self.delay_strategy.delay().await;
        self.seq.next().await.map(|x| (x, self.id.get()))
    }
}

/// A group of generators.
#[derive(Default)]
pub struct GeneratorGroup<'a, U: Send = Result<Op>, ERR: 'a + Send = ErrorType> {
    gens: Vec<Generator<'a, U, ERR>>,
    strategy: GeneratorGroupStrategy,
}

impl<'a, ERR: 'a + Send, U: Send + 'a> GeneratorGroup<'a, U, ERR> {
    pub fn new(gens: impl IntoIterator<Item = Generator<'a, U, ERR>>) -> Self {
        Self {
            gens: gens.into_iter().collect(),
            strategy: GeneratorGroupStrategy::default(),
        }
    }

    pub fn with_strategy(mut self, strategy: GeneratorGroupStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn push_generator(&mut self, gen: Generator<'a, U, ERR>) {
        self.gens.push(gen);
    }

    pub fn remove_generator(&mut self, index: usize) -> Generator<'a, U, ERR> {
        self.gens.remove(index)
    }
}

macro_rules! impl_async_iter_for_generator_group {
    ($func: ident, $return_type: ty) => {
        /// Select one generator to generate `Op` by group strategy. If it's empty,
        /// drop it and try to use another. If all [`Generator`]s in the group
        /// are empty, returns None.
        async fn $func(&mut self) -> $return_type {
            loop {
                if self.gens.is_empty() {
                    return None;
                }
                let selected = self.strategy.choose(0..self.gens.len());
                match self
                    .gens
                    .get_mut(selected)
                    .expect("selected index should be in the vec")
                    .$func()
                    .await
                {
                    x @ Some(_) => return x,
                    None => {
                        self.remove_generator(selected);
                    }
                }
            }
        }
    };
}

impl<'a, U: Send + 'a, ERR: 'a + Send> AsyncIter for GeneratorGroup<'a, U, ERR> {
    type Item = U;
    impl_async_iter_for_generator_group!(next, Option<Self::Item>);
    impl_async_iter_for_generator_group!(next_with_id, Option<(Self::Item, u64)>);
}

/// Convert a [`Generator`] to a [`GeneratorGroup`].
impl<'a, U: Send + 'a, ERR: 'a + Send> From<Generator<'a, U, ERR>> for GeneratorGroup<'a, U, ERR> {
    fn from(value: Generator<'a, U, ERR>) -> Self {
        Self {
            gens: Vec::from([value]),
            strategy: GeneratorGroupStrategy::default(),
        }
    }
}

/// Convert a [`GeneratorGroup`] to a [`Generator`]. Note that the delay
/// strategy of the first generator in group will be used as the new delay
/// strategy.
impl<'a, U: Send + 'a, ERR: 'a + Send> From<GeneratorGroup<'a, U, ERR>> for Generator<'a, U, ERR> {
    fn from(mut value: GeneratorGroup<'a, U, ERR>) -> Self {
        assert!(!value.gens.is_empty(), "group should not be empty");
        let mut strategy = value.strategy;
        let selected = strategy.choose(0..value.gens.len());
        let mut origin = value.gens.remove(selected);
        while !value.gens.is_empty() {
            let selected = strategy.choose(0..value.gens.len());
            let pop = value.gens.remove(selected);
            origin = origin.chain(pop);
        }
        origin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_generator() {
        let mut gen = 0..;
        let mut out = gen.gen_n(10);
        out.sort();
        assert_eq!(out, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[madsim::test]
    async fn generators_and_groups_id_should_be_correct() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let gen =
            Generator::new(Arc::clone(&global), tokio_stream::iter(global.take_seq(10))).await;
        assert_eq!(gen.id.get(), 0);
        let (g0, g1) = gen.split_at(5).await; // 0 1
        assert_eq!(g0.id.get(), 0);
        assert_eq!(g1.id.get(), 1);
        let g2 = Generator::new(Arc::clone(&global), tokio_stream::iter(global.take_seq(10))).await;
        assert_eq!(g2.id.get(), 2);
        let gen_group = GeneratorGroup::new([g0, g1]);
        assert_eq!(global.id_set.lock().unwrap().len(), 3); // 0 1 2
        let _gen_merge = Generator::from(gen_group);
        assert_eq!(global.id_set.lock().unwrap().len(), 2); // 0 2
        let g1 = Generator::new(Arc::clone(&global), tokio_stream::iter(global.take_seq(10))).await;
        assert_eq!(g1.id.get(), 1);
    }

    #[madsim::test]
    async fn test_generator_transform() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let seq = tokio_stream::iter(global.take_seq(50));
        let gen = Generator::new(global, seq).await;
        let gen = gen.map(|x| x + 2).filter(|x| x % 3 == 0).take(5);
        let out: Vec<_> = gen.seq.collect().await;
        assert_eq!(out, vec![3, 6, 9, 12, 15]);
    }

    #[madsim::test]
    async fn test_generator_split_at() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let seq = tokio_stream::iter(global.take_seq(5));
        let gen = Generator::new(global, seq).await;
        let (first, second) = gen.split_at(3).await;
        let first: Vec<_> = first.seq.collect().await;
        let second: Vec<_> = second.seq.collect().await;
        assert_eq!(first, vec![1, 2, 3]);
        assert_eq!(second, vec![4, 5]);
    }

    #[madsim::test]
    async fn test_generator_group() {
        let global = Arc::new(Global::<_, String>::new(1..));
        // Test Chain
        let gen1 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5))).await;
        let gen2 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5))).await;
        let gen_group =
            GeneratorGroup::new(vec![gen1, gen2]).with_strategy(GeneratorGroupStrategy::Chain);
        let res = gen_group.collect().await;
        assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        // Test RoundRobin
        let gen1 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5))).await;
        let gen2 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5))).await;
        let gen_group =
            GeneratorGroup::new(vec![gen1, gen2]).with_strategy(GeneratorGroupStrategy::default());
        let res = gen_group.collect().await;
        assert_eq!(res, vec![11, 16, 12, 17, 13, 18, 14, 19, 15, 20]);
        // Test Random
        let gen1 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5))).await;
        let gen2 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5))).await;
        let gen_group =
            GeneratorGroup::new(vec![gen1, gen2]).with_strategy(GeneratorGroupStrategy::Random);
        let res = gen_group.collect().await;
        assert!(res.into_iter().all(|x| (21..=30).contains(&x)));
    }

    #[madsim::test]
    async fn test_generator_group_into_generator() {
        let global = Arc::new(Global::new(1..));
        let gen1 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5))).await;
        let gen2 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5))).await;
        let gen_group = GeneratorGroup::new(vec![gen1, gen2]);
        let gen: Generator<_, i32> = gen_group.into();
        let res = gen.collect().await;
        assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[madsim::test]
    async fn test_generator_group_get_next_with_id() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let g1 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5))).await;
        let g2 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5))).await;
        let mut gen_group = GeneratorGroup::new([g1, g2]);
        assert_eq!(gen_group.next_with_id().await.unwrap(), (1, 0));
        assert_eq!(gen_group.next_with_id().await.unwrap(), (6, 1));
        assert_eq!(gen_group.next_with_id().await.unwrap(), (2, 0));
        assert_eq!(gen_group.next_with_id().await.unwrap(), (7, 1));
    }
}
