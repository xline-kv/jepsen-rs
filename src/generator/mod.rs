pub mod context;
pub mod controller;
pub mod elle_rw;
#[cfg(test)]
use std::ops::{AddAssign, RangeFrom};
use std::{fmt, ops::SubAssign, pin::Pin, sync::Arc};

use context::GeneratorId;
pub use context::Global;
use controller::{DelayStrategy, GeneratorGroupStrategy};
use log::{debug, trace};
use tokio_stream::{Stream, StreamExt as _};

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
    seq: Option<Pin<Box<dyn Stream<Item = U> + Send + 'a>>>,
    delay_strategy: Option<Pin<Box<dyn Stream<Item = DelayStrategy> + Send + 'a>>>,
    delay_strategy_one: Option<DelayStrategy>,
    id: Option<GeneratorId>,
    size: Option<usize>,
}

impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> GeneratorBuilder<'a, U, ERR> {
    #[inline]
    pub fn new(global: Arc<Global<'a, U, ERR>>) -> Self {
        Self {
            global,
            seq: None,
            delay_strategy: None,
            delay_strategy_one: None,
            id: None,
            size: None,
        }
    }
    #[inline]
    pub fn id(mut self, id: GeneratorId) -> Self {
        self.id = Some(id);
        self
    }
    #[inline]
    pub fn size(mut self, size: usize) -> Self {
        self.size = Some(size);
        self
    }
    #[inline]
    pub fn seq(self, seq: impl Stream<Item = U> + Send + 'a) -> Self {
        self.pinned_seq(Box::pin(seq))
    }
    #[inline]
    pub fn pinned_seq(mut self, seq: Pin<Box<dyn Stream<Item = U> + Send + 'a>>) -> Self {
        self.seq = Some(seq);
        self
    }

    /// use a given [`DelayStrategy`] for all times.
    ///
    /// note that the function must be called after `seq` or `pinned_seq`.
    #[inline]
    pub fn delay(mut self, delay_strategy: DelayStrategy) -> Self {
        self.delay_strategy_one = Some(delay_strategy);
        self
    }

    /// delays for a given sequence of [`DelayStrategy`]s
    ///
    /// note that the function must be called after `seq` or `pinned_seq`.
    #[inline]
    pub fn delay_stream(
        self,
        delay_strategy: impl Stream<Item = DelayStrategy> + Send + 'a,
    ) -> Self {
        self.pinned_delay_stream(Box::pin(delay_strategy))
    }

    /// delays for a given pinned sequence of [`DelayStrategy`]s
    ///
    /// note that the function must be called after `seq` or `pinned_seq`.
    #[inline]
    pub fn pinned_delay_stream(
        mut self,
        delay_strategy: Pin<Box<dyn Stream<Item = DelayStrategy> + Send + 'a>>,
    ) -> Self {
        self.delay_strategy = Some(delay_strategy);
        self
    }

    #[inline]
    pub fn build(self) -> Generator<'a, U, ERR> {
        let id = self.id.unwrap_or_else(|| self.global.get_id());
        debug!("build generator: {}", id.get());
        let size = self.size.unwrap_or_else(|| {
            let (size, validate) = self.seq.as_ref().expect("self.seq must be set").size_hint();
            assert_eq!(
                size,
                validate.expect("size hint must be an exact number"),
                "size hint must be an exact number"
            );
            size
        });

        let delay_strategy = self.delay_strategy.unwrap_or_else(|| {
            // TODO: use repeat_n when https://github.com/rust-lang/rust/issues/104434 stablized
            Box::pin(tokio_stream::iter(
                std::iter::repeat(self.delay_strategy_one.unwrap_or_default()).take(size),
            ))
        });

        let seq = self.seq.unwrap_or_else(|| Box::pin(tokio_stream::empty()));
        Generator {
            id,
            global: self.global,
            seq,
            delay_strategy,
            size,
        }
    }
}

/// The generator. Each generator is a **FINITE** Op sequence with a same size
/// sequence of [`DelayStrategy`]s. When generating each Op, the generator will
/// take an element from both the sequence and the [`DelayStrategy`] sequence,
/// returns the [`Op`] after delay for the corresponding [`DelayStrategy`].
pub struct Generator<'a, U: Send + fmt::Debug = Op, ERR: Send + 'a = ErrorType> {
    /// generator id
    pub id: GeneratorId,
    /// A reference to the global context
    pub global: Arc<Global<'a, U, ERR>>,
    /// The sequence (stream) of generator. Note that the seq is finite.
    pub seq: Pin<Box<dyn Stream<Item = U> + Send + 'a>>,
    /// The delay strategy stream, delays between every `next()` function
    pub delay_strategy: Pin<Box<dyn Stream<Item = DelayStrategy> + Send + 'a>>,
    /// The size of `seq` and `delay_strategy`.
    pub size: usize,
}

impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> Generator<'a, U, ERR> {
    pub fn map(self, f: impl Fn(U) -> U + Send + 'a) -> Self {
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .pinned_delay_stream(self.delay_strategy)
            .seq(self.seq.map(f))
            .size(self.size)
            .build()
    }

    /// The seq is finite, so we can collect it and calculate its size.
    /// We cannot use `size_hint` here, because filter will break the hint.
    pub async fn filter(self, f: impl Fn(&U) -> bool + Send + 'a) -> Self {
        let zipped =
            futures_util::StreamExt::zip(self.seq, self.delay_strategy).filter(|(x, _)| f(x));
        let (seq, delay): (Vec<_>, Vec<_>) = futures_util::StreamExt::unzip(zipped).await;
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .delay_stream(tokio_stream::iter(delay))
            .seq(tokio_stream::iter(seq))
            .build()
    }

    pub fn take(self, n: usize) -> Self {
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .delay_stream(self.delay_strategy.take(n))
            .seq(self.seq.take(n))
            .size(n)
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
        let first_delay = self.delay_strategy.as_mut().split_at(n).await;
        (
            GeneratorBuilder::new(Arc::clone(&self.global))
                .id(self.id)
                .delay_stream(tokio_stream::iter(first_delay))
                .size(first_seq.len())
                .seq(tokio_stream::iter(first_seq))
                .build(),
            GeneratorBuilder::new(self.global)
                .pinned_seq(self.seq)
                .pinned_delay_stream(self.delay_strategy)
                .build(),
        )
    }

    /// Chain two generators together.
    pub fn chain(self, other: Self) -> Self {
        let out_seq = self.seq.chain(other.seq);
        let out_delay = self.delay_strategy.chain(other.delay_strategy);
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .seq(out_seq)
            .delay_stream(out_delay)
            .size(self.size + other.size)
            .build()
    }
}

#[async_trait::async_trait]
impl<'a, ERR: 'a + Send, U: Send + fmt::Debug + 'a> AsyncIter for Generator<'a, U, ERR> {
    type Item = U;
    async fn next(&mut self) -> Option<Self::Item> {
        let item = self.seq.next().await;
        if item.is_none() {
            trace!("generator {} yields None", self.id.get());
            return None;
        }
        let delay = self
            .delay_strategy
            .next()
            .await
            .expect("delay strategy must be no less than seq");
        delay.delay().await;
        self.size.sub_assign(1);
        trace!(
            "generator {} yields an item: {:?}",
            self.id.get(),
            item.as_ref().unwrap()
        );
        item
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
        let out: Vec<_> = gen.seq.collect().await;
        assert_eq!(out, vec![3, 6, 9, 12, 15]);
    }

    #[madsim::test]
    async fn test_generator_split_at() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let seq = tokio_stream::iter(global.take_seq(5));
        let gen = GeneratorBuilder::new(Arc::clone(&global)).seq(seq).build();
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

    #[madsim::test]
    async fn test_generator_delay_strategy_and_size() {
        let global = Arc::new(Global::<_, String>::new(1..));
        let g1 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        assert_eq!(g1.size, 5);
        // size is correct
        let g2 = g1.map(|x| x).filter(|x| x % 2 == 0).await;
        assert_eq!(g2.size, 2);
        let g3 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let g3 = g2.chain(g3);
        assert_eq!(g3.size, 7);
        let (g3, g4) = g3.split_at(3).await;
        assert_eq!(g3.size, 3);
        assert_eq!(g4.size, 4);
    }
}
