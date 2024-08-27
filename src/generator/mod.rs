pub mod context;
pub mod controller;
pub mod elle_rw;
#[cfg(test)]
use std::ops::{AddAssign, RangeFrom};
use std::{pin::Pin, sync::Arc};

use anyhow::Result;
pub use context::Global;
use controller::{DelayStrategy, GeneratorGroupStrategy};
use tokio_stream::{Stream, StreamExt};

use crate::{
    op::Op,
    utils::{AsyncIter, ExtraStreamExt},
};

/// The id of the generator. Each [`GeneratorId`] corresponds to one thread.
pub type GeneratorId = u64;

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
pub struct Generator<'a, U: Send = Result<Op>> {
    /// generator id
    pub id: GeneratorId,
    /// A reference to the global context
    pub global: Arc<Global<'a, U>>,
    /// The sequence (stream) of generator. Note that the seq is finite.
    pub seq: Pin<Box<dyn Stream<Item = U> + Send + 'a>>,
    /// The delay strategy between every `next()` function
    pub delay_strategy: DelayStrategy,
}

impl<'a, U: Send + 'a> Generator<'a, U> {
    pub fn new(global: Arc<Global<'a, U>>, seq: impl Stream<Item = U> + Send + 'a) -> Self {
        let id = global.get_next_id();
        Self {
            id,
            global,
            seq: Box::pin(seq),
            delay_strategy: DelayStrategy::default(),
        }
    }

    pub fn new_with_id(
        id: GeneratorId,
        global: Arc<Global<'a, U>>,
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
        global: Arc<Global<'a, U>>,
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
            Generator::new_with_pined_seq(self.id, self.global, self.seq),
        )
    }

    pub fn chain(self, other: Self) -> Self {
        let out = self.seq.chain(other.seq);
        Generator::new_with_id(self.id, self.global, out)
    }
}

impl<'a, U: Send + 'a> AsyncIter for Generator<'a, U> {
    type Item = U;
    async fn next(&mut self) -> Option<Self::Item> {
        self.delay_strategy.delay().await;
        self.seq.next().await
    }
}

/// A group of generators.
#[derive(Default)]
pub struct GeneratorGroup<'a, U: Send = Result<Op>> {
    gens: Vec<Generator<'a, U>>,
    strategy: GeneratorGroupStrategy,
}

impl<'a, U: Send + 'a> GeneratorGroup<'a, U> {
    pub fn new(gens: impl Into<Vec<Generator<'a, U>>>) -> Self {
        Self {
            gens: gens.into(),
            strategy: GeneratorGroupStrategy::default(),
        }
    }

    pub fn with_strategy(mut self, strategy: GeneratorGroupStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    pub fn push_generator(&mut self, gen: Generator<'a, U>) {
        self.gens.push(gen);
    }

    pub fn remove_generator(&mut self, index: usize) {
        let g = self.gens.remove(index);
        g.global.free_id(g.id);
    }
}

impl<'a, U: Send + 'a> From<Generator<'a, U>> for GeneratorGroup<'a, U> {
    fn from(value: Generator<'a, U>) -> Self {
        Self {
            gens: Vec::from([value]),
            strategy: GeneratorGroupStrategy::default(),
        }
    }
}

impl<'a, U: Send + 'a> AsyncIter for GeneratorGroup<'a, U> {
    type Item = U;
    /// Select one generator to generate `Op` by group strategy. If it's empty,
    /// drop it and try to use another. If all [`Generator`]s in the group
    /// are empty, returns None.
    async fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.gens.is_empty() {
                return None;
            }
            let selected = self.strategy.choose(0..self.gens.len());
            match self
                .gens
                .get_mut(selected)
                .expect("selected index should be in the vec")
                .next()
                .await
            {
                Some(op) => return Some(op),
                None => {
                    self.remove_generator(selected);
                }
            }
        }
    }
}

/// Convert a [`GeneratorGroup`] to a [`Generator`].
impl<'a, U: Send + 'a> From<GeneratorGroup<'a, U>> for Generator<'a, U> {
    fn from(mut value: GeneratorGroup<'a, U>) -> Self {
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
    async fn test_generator_transform() {
        let global = Global::new(1..);
        let seq = tokio_stream::iter(global.take_seq(50));
        let gen = Generator::new(Arc::new(global), seq);
        let gen = gen.map(|x| x + 2).filter(|x| x % 3 == 0).take(5);
        let out: Vec<_> = gen.seq.collect().await;
        assert_eq!(out, vec![3, 6, 9, 12, 15]);
    }

    #[madsim::test]
    async fn test_generator_split_at() {
        let global = Global::new(1..);
        let seq = tokio_stream::iter(global.take_seq(5));
        let gen = Generator::new(Arc::new(global), seq);
        let (first, second) = gen.split_at(3).await;
        let first: Vec<_> = first.seq.collect().await;
        let second: Vec<_> = second.seq.collect().await;
        assert_eq!(first, vec![1, 2, 3]);
        assert_eq!(second, vec![4, 5]);
    }

    #[madsim::test]
    async fn test_generator_group() {
        let global = Arc::new(Global::new(1..));
        // Test Chain
        let gen1 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5)));
        let gen2 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5)));
        let gen_group =
            GeneratorGroup::new(vec![gen1, gen2]).with_strategy(GeneratorGroupStrategy::Chain);
        let res = gen_group.collect().await;
        assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        // Test RoundRobin
        let gen1 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5)));
        let gen2 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5)));
        let gen_group =
            GeneratorGroup::new(vec![gen1, gen2]).with_strategy(GeneratorGroupStrategy::default());
        let res = gen_group.collect().await;
        assert_eq!(res, vec![11, 16, 12, 17, 13, 18, 14, 19, 15, 20]);
        // Test Random
        let gen1 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5)));
        let gen2 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5)));
        let gen_group =
            GeneratorGroup::new(vec![gen1, gen2]).with_strategy(GeneratorGroupStrategy::Random);
        let res = gen_group.collect().await;
        assert!(res.into_iter().all(|x| (21..=30).contains(&x)));
    }

    #[madsim::test]
    async fn test_generator_group_into_generator() {
        let global = Arc::new(Global::new(1..));
        let gen1 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5)));
        let gen2 = Generator::new(global.clone(), tokio_stream::iter(global.take_seq(5)));
        let gen_group = GeneratorGroup::new(vec![gen1, gen2]);
        let gen: Generator<_> = gen_group.into();
        let res = gen.collect().await;
        assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }
}
