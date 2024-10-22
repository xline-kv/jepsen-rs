pub mod context;
pub mod controller;
pub mod elle_rw;
pub mod raw_gen;
pub mod traits;

use core::panic;
use std::{fmt, pin::Pin, sync::Arc};

use context::GeneratorId;
pub use context::Global;
use controller::{DelayStrategy, GeneratorGroupStrategy};
use log::{debug, trace};
pub use raw_gen::RawGenerator;
use tap::Tap;
use tokio_stream::{Stream, StreamExt as _};
pub use traits::*;

use crate::{
    history::ErrorType,
    op::Op,
    utils::{Counter, ExtraStreamExt},
};

/// The content of a generator, a tuple of [`Op`] and [`DelayStrategy`].
pub type GeneratorContent<U> = (U, DelayStrategy);

/// The builder of generator.
pub struct GeneratorBuilder<'a, U: Send + fmt::Debug = Op, ERR: Send + 'a = ErrorType> {
    /// A reference to the global context
    global: Arc<Global<'a, U, ERR>>,
    /// user provided sequence
    raw_seq: Option<Box<dyn Stream<Item = U> + Send + Unpin + 'a>>,
    /// seq which is gotten from other generators. The wrapped_seq is the
    /// combination of `raw_seq` and `delay_strategy` seq.
    wrapped_seq: Option<Pin<Box<dyn Stream<Item = GeneratorContent<U>> + Send + 'a>>>,
    /// user provided delay strategy for all elements of this generator
    delay_strategy_one: Option<DelayStrategy>,
    /// generator id. If not provided, a new id will be generated
    id: Option<GeneratorId>,
}

impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> GeneratorBuilder<'a, U, ERR> {
    /// Create a new generator builder.
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
    /// Set the id of the generator.
    #[inline]
    pub fn id(mut self, id: GeneratorId) -> Self {
        self.id = Some(id);
        self
    }
    /// Set the sequence of the generator.
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
    /// function `pinned_seq` is used for the inner StreamExt functions, and
    /// may not be used by the user.
    #[inline]
    pub fn pinned_seq(
        mut self,
        seq: Pin<Box<dyn Stream<Item = GeneratorContent<U>> + Send + 'a>>,
    ) -> Self {
        self.wrapped_seq = Some(seq);
        self
    }
    /// Set the delay strategy of the generator
    #[inline]
    pub fn delay_strategy(mut self, delay: DelayStrategy) -> Self {
        self.delay_strategy_one = Some(delay);
        self
    }

    /// Build the generator.
    ///
    /// If `wrapped_seq` is provided, the `raw_seq` will be ignored. If
    /// `wrapped_seq` is none, the `raw_seq` must be provided.
    /// If `delay_strategy` is not provided, use default delay strategy.
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

/// The generator. Each generator is a sequence of each the combination of Op
/// and [`DelayStrategy`]. When generating each Op (calling `next`), the
/// generator will take an element, returns the [`Op`] after delaying for the
/// corresponding [`DelayStrategy`].
pub struct Generator<'a, U: Send + fmt::Debug = Op, ERR: Send + 'a = ErrorType> {
    /// generator id
    pub id: GeneratorId,
    /// A reference to the global context
    pub global: Arc<Global<'a, U, ERR>>,
    /// The sequence (stream) of generator, each element is (Op, DelayStrategy).
    pub seq: Pin<Box<dyn Stream<Item = GeneratorContent<U>> + Send + 'a>>,
}

impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> Generator<'a, U, ERR> {
    /// Create an empty generator
    pub fn empty(global: Arc<Global<'a, U, ERR>>) -> Self {
        GeneratorBuilder::new(global)
            .seq(tokio_stream::empty())
            .build()
    }

    /// Create a generator with a single element
    pub fn once(global: Arc<Global<'a, U, ERR>>, u: U) -> Self {
        GeneratorBuilder::new(global)
            .seq(tokio_stream::iter(std::iter::once(u)))
            .build()
    }

    /// Map the sequence.
    /// Note: the element type cannot be changed after mapping because the
    /// Global type is not changable.
    pub fn map(self, f: impl Fn(U) -> U + Send + 'a) -> Self {
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .wrapped_seq(self.seq.map(move |(u, d)| (f(u), d)))
            .build()
    }

    /// Filter the sequence.
    pub async fn filter(self, f: impl Fn(&U) -> bool + Send + 'a) -> Self {
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .wrapped_seq(self.seq.filter(move |(u, _)| f(u)))
            .build()
    }

    /// Keep the first `n` elements from the sequence and drop the rest.
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
    /// will alloc for a new id.
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

    /// Chain two generators together, use the first generator's id.
    pub fn chain(self, other: Self) -> Self {
        let out_seq = self.seq.chain(other.seq);
        GeneratorBuilder::new(self.global)
            .id(self.id)
            .wrapped_seq(out_seq)
            .build()
    }
}

#[async_trait::async_trait]
impl<'a, ERR: 'a + Send, U: Send + fmt::Debug + 'a> GeneratorItemGetter for Generator<'a, U, ERR> {
    type G = Global<'a, U, ERR>;
    fn id(&self) -> u64 {
        self.id.get()
    }
    fn global(&self) -> &Arc<Self::G> {
        &self.global
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
}

// use default impl
impl<'a, ERR: 'a + Send, U: Send + fmt::Debug + 'a> GeneratorIter for Generator<'a, U, ERR> {}

#[async_trait::async_trait]
impl<'a, ERR: 'a + Send, U: Send + fmt::Debug + 'a> DelayAsyncIter for Generator<'a, U, ERR> {
    type DelayType = DelayStrategy;
    async fn get_without_delay(&mut self) -> Option<GeneratorContent<Self::Item>> {
        self.seq
            .next()
            .await
            .tap(|x| trace!("generator {} yields {:?} without delay", self.id.get(), x))
    }
}

// Group

/// The dyn type of the sequence item in a generator group.
pub type GeneratorGroupSeq<'a, U, ERR> =
    dyn DelayAsyncIter<Item = U, DelayType = DelayStrategy, G = Global<'a, U, ERR>> + Send + 'a;

/// Generator with its ratio. The [`Counter`] indicates how many generations
/// left until the next generator exchange event.
pub type GeneratorGroupContent<'a, U, ERR> = (Box<GeneratorGroupSeq<'a, U, ERR>>, Counter);

/// A group of generators. It provides the flexibility to combine multiple
/// generators into one.
///
/// A [`GeneratorGroup`] is built with sequence of any dyn [`DelayAsyncIter`].
pub struct GeneratorGroup<'a, U: Send + fmt::Debug = Op, ERR: 'a + Send = ErrorType> {
    /// The global context.
    global: Arc<Global<'a, U, ERR>>,

    /// stores all generators and its ratio.
    gens: Vec<GeneratorGroupContent<'a, U, ERR>>,

    /// the strategy indicates how to choose a generator
    strategy: GeneratorGroupStrategy,

    /// the currently selected generator
    selected: usize,
}

impl<'a, ERR: 'a + Send, U: Send + fmt::Debug + 'a> GeneratorGroup<'a, U, ERR> {
    /// Create a new generator group with the given generators.
    pub fn new<T>(gens: impl IntoIterator<Item = T>) -> Self
    where
        T: DelayAsyncIter<Item = U, DelayType = DelayStrategy, G = Global<'a, U, ERR>> + Send + 'a,
    {
        Self::new_with_count_inner(gens.into_iter().map(|x| (Box::new(x) as _, 1)))
    }

    ///  Create a new generator group with the given generators and its ratio.
    pub fn new_with_count<T>(gens: impl IntoIterator<Item = (T, usize)>) -> Self
    where
        T: DelayAsyncIter<Item = U, DelayType = DelayStrategy, G = Global<'a, U, ERR>> + Send + 'a,
    {
        Self::new_with_count_inner(gens.into_iter().map(|(x, c)| (Box::new(x) as _, c)))
    }

    fn new_with_count_inner(
        gens: impl IntoIterator<Item = (Box<GeneratorGroupSeq<'a, U, ERR>>, usize)>,
    ) -> Self {
        let gens: Vec<_> = gens
            .into_iter()
            .map(|(g, c)| (g, Counter::new(c)))
            .collect();
        assert!(
            !gens.is_empty(),
            "cannot build a Generator group without item"
        );
        debug!("generator group created with {} generators", gens.len());
        let ids: Vec<_> = gens.iter().map(|x| x.0.id()).collect();
        debug!("ids: {:?}", ids);
        Self {
            global: Arc::clone(gens[0].0.global()),
            gens,
            strategy: GeneratorGroupStrategy::default(),
            selected: 0,
        }
    }

    /// Set the strategy of the generator group
    #[inline]
    pub fn with_strategy(mut self, strategy: GeneratorGroupStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Add a new generator to the group
    #[inline]
    pub fn push_generator(&mut self, gen: Box<GeneratorGroupSeq<'a, U, ERR>>) {
        self.gens.push((gen, Counter::new(1)));
    }

    /// Add a new generator with ratio to the group
    #[inline]
    pub fn push_generator_with_ratio(
        &mut self,
        gen: Box<GeneratorGroupSeq<'a, U, ERR>>,
        total: usize,
    ) {
        self.gens.push((gen, Counter::new(total)));
    }

    /// Remove a generator from the group
    #[inline]
    pub fn remove_generator(&mut self, index: usize) {
        self.gens.remove(index);
    }

    /// Remove the current generator and select the next one by the strategy
    #[inline]
    pub fn remove_current_and_reselect(&mut self) {
        let is_last = self.selected == self.gens.len() - 1;
        self.remove_generator(self.selected);
        // do not select when the group is empty.
        if self.gens.is_empty() {
            return;
        }
        // If strategy is round-robin or chain, it's no need to choose a new
        // one. The `self.selected` will automatically point to the next one.

        // But if the `self.selected` is the last, we must select a new one, otherwise
        // there will be a stack overflow.
        if is_last || matches!(self.strategy, GeneratorGroupStrategy::Random) {
            self.select_current();
        }
    }

    /// Get the current generator
    #[inline]
    pub fn current(&self) -> &GeneratorGroupContent<'a, U, ERR> {
        self.gens
            .get(self.selected)
            .expect("selected index should in the range")
    }

    /// select to the next generator
    #[inline]
    fn select(&mut self, index: usize) {
        self.selected = index;
    }

    /// choose the next generator and select it
    #[inline]
    fn select_current(&mut self) {
        let next_g = self.strategy.choose(0..self.gens.len());
        self.select(next_g);
    }

    /// Convert a [`GeneratorGroup`] to a [`Generator`]. The delay_strategy of
    /// the [`Generator`] will be kept.
    ///
    /// This method is mainly used for combining multiple [`GeneratorGroup`]s
    /// together. If you want to combine multiple [`GeneratorGroup`]s together,
    /// you need to convert them to [`Generator`]s first.
    pub async fn to_generator(self) -> Generator<'a, U, ERR> {
        assert!(!self.gens.is_empty(), "group should not be empty");
        let global = self.global.clone();
        let output = self.collect_all().await;
        GeneratorBuilder::new(global)
            .wrapped_seq(tokio_stream::iter(output))
            .build()
    }
}

#[async_trait::async_trait]
impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> GeneratorItemGetter
    for GeneratorGroup<'a, U, ERR>
{
    type G = Global<'a, U, ERR>;
    fn id(&self) -> u64 {
        unimplemented!("generator group do not have single id")
    }
    /// Get the global.
    ///
    /// # Panic
    ///
    /// Panics if the group is empty.
    fn global(&self) -> &Arc<Global<'a, U, ERR>> {
        if self.gens.is_empty() {
            panic!("cannot get global of an empty generator group");
        } else {
            self.gens[0].0.global()
        }
    }
}

// The proc macro `#[async_trait]` expand first, and do not take effect to this
// impl. So I expand it manually.
macro_rules! impl_generator_group {
    ($func:ident, $ret: ty) => {
        /// If current generator is not empty, use it.
        /// Otherwise, select next generator to generate `Op` by group strategy. If
        /// it's empty, drop it and try to use another. If all [`Generator`]s in
        /// the group are empty, returns None.
        fn $func<'life0, 'async_trait>(
            &'life0 mut self,
        ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = $ret> + Send + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                loop {
                    if self.gens.is_empty() {
                        return None;
                    }
                    let s = self
                        .gens
                        .get_mut(self.selected)
                        .expect("selected index should in the range");
                    if s.1.over() {
                        s.1.reset();
                        self.select_current();
                        continue;
                    }
                    s.1.count().expect("counter should not be over");
                    match s.0.$func().await {
                        x @ Some(_) => return x,
                        None => {
                            self.remove_current_and_reselect();
                        }
                    }
                }
            })
        }
    };
}

#[async_trait::async_trait]
impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> AsyncIter for GeneratorGroup<'a, U, ERR> {
    type Item = U;
    impl_generator_group!(next, Option<Self::Item>);
}

#[async_trait::async_trait]
impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> GeneratorIter for GeneratorGroup<'a, U, ERR> {
    impl_generator_group!(next_with_id, Option<(Self::Item, u64)>);
}

impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> DelayAsyncIter for GeneratorGroup<'a, U, ERR> {
    type DelayType = DelayStrategy;
    impl_generator_group!(get_without_delay, Option<(Self::Item, Self::DelayType)>);
}

/// Convert a [`Generator`] to a [`GeneratorGroup`].
impl<'a, U: Send + fmt::Debug + 'a, ERR: 'a + Send> From<Generator<'a, U, ERR>>
    for GeneratorGroup<'a, U, ERR>
{
    fn from(value: Generator<'a, U, ERR>) -> Self {
        Self::new([value])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::log_init;

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
        let _gen_merge = gen_group.to_generator().await;
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
    async fn test_generator_group_strategy() {
        let global = Arc::new(Global::<_, String>::new(1..));

        // Test Chain
        let gen1 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let gen2 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();

        let gen_group =
            GeneratorGroup::new([gen1, gen2]).with_strategy(GeneratorGroupStrategy::Chain);
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
            GeneratorGroup::new([gen1, gen2]).with_strategy(GeneratorGroupStrategy::default());
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
            GeneratorGroup::new([gen1, gen2]).with_strategy(GeneratorGroupStrategy::Random);
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
        let gen_group =
            GeneratorGroup::new([gen1, gen2]).with_strategy(GeneratorGroupStrategy::Chain);
        let gen: Generator<_, i32> = gen_group.to_generator().await;
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
    async fn test_generator_group_with_custom_counter() {
        log_init();
        let global = Arc::new(Global::<_, String>::new(1..));
        let g1 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let g2 = GeneratorBuilder::new(Arc::clone(&global))
            .seq(tokio_stream::iter(global.take_seq(5)))
            .build();
        let gen_group = GeneratorGroup::new_with_count([(g1, 2), (g2, 3)]);
        let ret = gen_group.collect().await;

        // g1 gens 2 elements, and g2 gens 3, and so on.
        assert_eq!(ret, vec![1, 2, 6, 7, 8, 3, 4, 9, 10, 5]);
    }
}
