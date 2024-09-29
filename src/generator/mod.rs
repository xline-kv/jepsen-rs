pub mod context;
pub mod controller;
pub mod elle_rw;
#[cfg(test)]
use std::ops::{AddAssign, RangeFrom};
use std::{fmt, pin::Pin, sync::Arc};

use context::GeneratorId;
pub use context::Global;
use controller::{DelayStrategy, GeneratorGroupStrategy};
use log::{debug, trace};
use tap::Tap;
use tokio_stream::{Stream, StreamExt as _};

use crate::{
    history::ErrorType,
    op::Op,
    utils::{AsyncIter, ExtraStreamExt},
};

/// The content of a generator, a tuple of [`Op`] and [`DelayStrategy`].
pub type GeneratorContent<U> = (U, DelayStrategy);

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
        trace!("takes {} items out from RawGenerator", n);
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

/// The builder of generator.
pub struct GeneratorBuilder<'a, U: Send + fmt::Debug = Op, ERR: Send + 'a = ErrorType> {
    global: Arc<Global<'a, U, ERR>>,
    /// user provided sequence
    raw_seq: Option<Box<dyn Stream<Item = U> + Send + Unpin + 'a>>,
    /// seq which is gotten from other generators
    wrapped_seq: Option<Pin<Box<dyn Stream<Item = GeneratorContent<U>> + Send + 'a>>>,
    /// user provided delay strategy for all elements of this generator
    delay_strategy_one: Option<DelayStrategy>,
    id: Option<GeneratorId>,
}

impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> GeneratorBuilder<'a, U, ERR> {
    #[inline]
    pub fn new(global: Arc<Global<'a, U, ERR>>) -> Self {
        Self {
            global,
            raw_seq: None,
            wrapped_seq: None,
            delay_strategy_one: None,
            id: None,
        }
    }
    #[inline]
    pub fn id(mut self, id: GeneratorId) -> Self {
        self.id = Some(id);
        self
    }
    #[inline]
    pub fn seq(mut self, seq: impl Stream<Item = U> + Send + Unpin + 'a) -> Self {
        self.raw_seq = Some(Box::new(seq));
        self
    }
    /// function `wrapped_seq` is used for the inner StreamExt functions, and
    /// may not be used by the user.
    #[inline]
    pub fn wrapped_seq(self, seq: impl Stream<Item = GeneratorContent<U>> + Send + 'a) -> Self {
        self.pinned_seq(Box::pin(seq))
    }
    #[inline]
    pub fn pinned_seq(
        mut self,
        seq: Pin<Box<dyn Stream<Item = GeneratorContent<U>> + Send + 'a>>,
    ) -> Self {
        self.wrapped_seq = Some(seq);
        self
    }
    #[inline]
    pub fn delay_strategy(mut self, delay: DelayStrategy) -> Self {
        self.delay_strategy_one = Some(delay);
        self
    }

    /// Build the generator.
    ///
    /// If `wrapped_seq` is provided, the `raw_seq` will be ignored. If
    /// `wrapped_seq` is none, the `raw_seq` must be provided, and if
    /// `delay_strategy` is not provided, use default delay strategy.
    #[inline]
    pub fn build(self) -> Generator<'a, U, ERR> {
        let id = self.id.unwrap_or_else(|| self.global.get_id());
        debug!("build generator: {}", id.get());
        let seq = self
            .wrapped_seq
            .or_else(|| {
                let delay_strategy = self
                    .delay_strategy_one
                    .unwrap_or_default();
                self.raw_seq.map(|x| {
                    let pinned_stream = Pin::new(x);
                    let mapped_stream =
                        pinned_stream.map(move |item| (item, delay_strategy.clone()));
                    Box::pin(mapped_stream)
                        as Pin<Box<dyn Stream<Item = GeneratorContent<U>> + Send + 'a>>
                })
            })
            .expect("cannot construct a generator: you must provide a `wrapped_seq`, or a `raw_seq` with a `delay_strategy`");

        Generator {
            id,
            global: self.global,
            seq,
        }
    }
}

/// The generator. Each generator is a Op sequence with a same size sequence of
/// [`DelayStrategy`]s. When generating each Op, the generator will take an
/// element from both the sequence and the [`DelayStrategy`] sequence, returns
/// the [`Op`] after delay for the corresponding [`DelayStrategy`].
pub struct Generator<'a, U: Send + fmt::Debug = Op, ERR: Send + 'a = ErrorType> {
    /// generator id
    pub id: GeneratorId,
    /// A reference to the global context
    pub global: Arc<Global<'a, U, ERR>>,
    /// The sequence (stream) of generator, each element is (Op, DelayStrategy).
    pub seq: Pin<Box<dyn Stream<Item = GeneratorContent<U>> + Send + 'a>>,
}

impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> Generator<'a, U, ERR> {
    pub fn map(self, f: impl Fn(U) -> U + Send + 'a) -> Self {
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .wrapped_seq(self.seq.map(move |(u, d)| (f(u), d)))
            .build()
    }

    pub async fn filter(self, f: impl Fn(&U) -> bool + Send + 'a) -> Self {
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .wrapped_seq(self.seq.filter(move |(u, _)| f(u)))
            .build()
    }

    pub fn take(self, n: usize) -> Self {
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .wrapped_seq(self.seq.take(n))
            .build()
    }

    /// Split the [`Generator`] into two generators, the first generator will
    /// take the first `n` elements from the seq and the second generator
    /// will keep the rest.
    ///
    /// First generator will keep the generator id, and the second [`Generator`]
    /// will alloc a new id.
    pub async fn split_at(mut self, n: usize) -> (Self, Self) {
        let first_seq = self.seq.as_mut().split_at(n).await;
        (
            GeneratorBuilder::new(Arc::clone(&self.global))
                .id(self.id)
                .wrapped_seq(tokio_stream::iter(first_seq))
                .build(),
            GeneratorBuilder::new(self.global)
                .pinned_seq(self.seq)
                .build(),
        )
    }

    /// Chain two generators together.
    pub fn chain(self, other: Self) -> Self {
        let out_seq = self.seq.chain(other.seq);
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .wrapped_seq(out_seq)
            .build()
    }

    /// Collect the generator into a vector, without [`DelayStrategy`].
    pub async fn collect(self) -> Vec<U> {
        self.seq.map(|x| x.0).collect().await
    }
}

#[async_trait::async_trait]
impl<'a, ERR: 'a + Send, U: Send + fmt::Debug + 'a> AsyncIter for Generator<'a, U, ERR> {
    type Item = U;
    async fn next(&mut self) -> Option<Self::Item> {
        let (item, delay) = self
            .seq
            .next()
            .await
            .tap(|x| trace!("generator {} yields {:?}", self.id.get(), x))?;
        delay.delay().await;
        Some(item)
    }
    async fn next_with_id(&mut self) -> Option<(Self::Item, u64)> {
        self.next().await.map(|x| (x, self.id.get()))
    }
}

/// A group of generators.
#[derive(Default)]
pub struct GeneratorGroup<'a, U: Send + fmt::Debug = Op, ERR: 'a + Send = ErrorType> {
    gens: Vec<Generator<'a, U, ERR>>,
    strategy: GeneratorGroupStrategy,
}

impl<'a, ERR: 'a + Send, U: Send + fmt::Debug + 'a> GeneratorGroup<'a, U, ERR> {
    pub fn new(gens: impl IntoIterator<Item = Generator<'a, U, ERR>>) -> Self {
        let gens: Vec<_> = gens.into_iter().collect();
        debug!("generator group created with {} generators", gens.len());
        let ids: Vec<_> = gens.iter().map(|x| x.id.get()).collect();
        debug!("ids: {:?}", ids);
        Self {
            gens,
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

#[async_trait::async_trait]
impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> AsyncIter for GeneratorGroup<'a, U, ERR> {
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
                x @ Some(_) => return x,
                None => {
                    self.remove_generator(selected);
                }
            }
        }
    }
    /// Select one generator to generate `Op` by group strategy. If it's empty,
    /// drop it and try to use another. If all [`Generator`]s in the group
    /// are empty, returns None.
    async fn next_with_id(&mut self) -> Option<(Self::Item, u64)> {
        loop {
            if self.gens.is_empty() {
                return None;
            }
            let selected = self.strategy.choose(0..self.gens.len());
            match self
                .gens
                .get_mut(selected)
                .expect("selected index should be in the vec")
                .next_with_id()
                .await
            {
                x @ Some(_) => return x,
                None => {
                    self.remove_generator(selected);
                }
            }
        }
    }
}

/// Convert a [`Generator`] to a [`GeneratorGroup`].
impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> From<Generator<'a, U, ERR>>
    for GeneratorGroup<'a, U, ERR>
{
    fn from(value: Generator<'a, U, ERR>) -> Self {
        Self {
            gens: Vec::from([value]),
            strategy: GeneratorGroupStrategy::default(),
        }
    }
}

/// Convert a [`GeneratorGroup`] to a [`Generator`]. The delay_strategy of the
/// [`Generator`] will be kept.
impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> From<GeneratorGroup<'a, U, ERR>>
    for Generator<'a, U, ERR>
{
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
    async fn chain_generator_will_free_the_id() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let gen = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(10)))
            .build();
        let (g0, g1) = gen.split_at(5).await;
        assert_eq!(g0.id.get(), 0);
        assert_eq!(g1.id.get(), 1);
        let g2 = g0.chain(g1);
        assert_eq!(g2.id.get(), 0);
    }

    #[madsim::test]
    async fn generators_and_groups_id_should_be_correct() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let gen = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(10)))
            .build();
        assert_eq!(gen.id.get(), 0);
        let (g0, g1) = gen.split_at(5).await; // 0 1
        assert_eq!(g0.id.get(), 0);
        assert_eq!(g1.id.get(), 1);
        let g2 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(10)))
            .build();
        assert_eq!(g2.id.get(), 2);
        let gen_group = GeneratorGroup::new([g0, g1]);
        assert_eq!(global.id_set.lock().unwrap().len(), 3); // 0 1 2
        let _gen_merge = Generator::from(gen_group);
        assert_eq!(global.id_set.lock().unwrap().len(), 2); // 0 2
        let g1 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(10)))
            .build();
        assert_eq!(g1.id.get(), 1);
    }

    #[madsim::test]
    async fn test_generator_transform() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let seq = tokio_stream::iter(global.take_seq(50));
        let gen = GeneratorBuilder::new(global).seq(seq).build();
        let gen = gen.map(|x| x + 2).filter(|x| x % 3 == 0).await.take(5);
        let out: Vec<_> = gen.collect().await;
        assert_eq!(out, vec![3, 6, 9, 12, 15]);
    }

    #[madsim::test]
    async fn test_generator_split_at() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let seq = tokio_stream::iter(global.take_seq(5));
        let gen = GeneratorBuilder::new(Arc::clone(&global)).seq(seq).build();
        let (first, second) = gen.split_at(3).await;
        let first: Vec<_> = first.collect().await;
        let second: Vec<_> = second.collect().await;
        assert_eq!(first, vec![1, 2, 3]);
        assert_eq!(second, vec![4, 5]);
    }

    #[madsim::test]
    async fn test_generator_group() {
        let global = Arc::new(Global::<_, String>::new(1..));
        // Test Chain
        let gen1 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let gen2 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();

        let gen_group =
            GeneratorGroup::new(vec![gen1, gen2]).with_strategy(GeneratorGroupStrategy::Chain);
        let res = gen_group.collect().await;
        assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

        // Test RoundRobin
        let gen1 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let gen2 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let gen_group =
            GeneratorGroup::new(vec![gen1, gen2]).with_strategy(GeneratorGroupStrategy::default());
        let res = gen_group.collect().await;
        assert_eq!(res, vec![11, 16, 12, 17, 13, 18, 14, 19, 15, 20]);

        // Test Random
        let gen1 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let gen2 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let gen_group =
            GeneratorGroup::new(vec![gen1, gen2]).with_strategy(GeneratorGroupStrategy::Random);
        let res = gen_group.collect().await;
        assert!(res.into_iter().all(|x| (21..=30).contains(&x)));
    }

    #[madsim::test]
    async fn test_generator_group_into_generator() {
        let global = Arc::new(Global::new(1..));
        let gen1 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let gen2 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let gen_group = GeneratorGroup::new(vec![gen1, gen2]);
        let gen: Generator<_, i32> = gen_group.into();
        let res = gen.collect().await;
        assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[madsim::test]
    async fn test_generator_group_get_next_with_id() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let g1 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let g2 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let mut gen_group = GeneratorGroup::new([g1, g2]);
        assert_eq!(gen_group.next_with_id().await.unwrap(), (1, 0));
        assert_eq!(gen_group.next_with_id().await.unwrap(), (6, 1));
        assert_eq!(gen_group.next_with_id().await.unwrap(), (2, 0));
        assert_eq!(gen_group.next_with_id().await.unwrap(), (7, 1));
    }
}
