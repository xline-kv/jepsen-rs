pub mod context;
mod elle_rw;
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use anyhow::Result;
pub use context::Global;
use madsim::runtime::NodeHandle;

use crate::{op::Op, utils::IteratorExt};

/// The id of the generator. Each [`GeneratorId`] corresponds to one thread.
pub type GeneratorId = u64;

/// Cache size for the generator.
pub const GENERATOR_CACHE_SIZE: usize = 200;

/// This trait is for the raw generator (clojure generator), which will only
/// generate ops infinitely.
pub trait RawGenerator {
    type Item;
    fn get_op(&mut self) -> Self::Item;
}

/// The generator. It's a wrapper for the clojure seq and global context.
pub struct Generator<'a, T: Iterator<Item = U>, U = Result<Op>> {
    /// generator id
    pub id: GeneratorId,
    /// A reference to the global context
    pub global: Arc<Global<'a, U>>,
    /// The generator sequence
    pub seq: T,
}

impl<T: Iterator<Item = U>, U> Deref for Generator<'_, T, U> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.seq
    }
}

impl<T: Iterator<Item = U>, U> DerefMut for Generator<'_, T, U> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.seq
    }
}
impl<T: Iterator<Item = U>, U> Iterator for Generator<'_, T, U> {
    type Item = T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.seq.next()
    }
}

impl<'a, T: Iterator<Item = U>, U: 'a> Generator<'a, T, U> {
    pub fn new(global: Arc<Global<'a, U>>, seq: T) -> Self {
        let id = global.get_next_id();
        Self { id, global, seq }
    }

    pub fn new_with_id(id: GeneratorId, global: Arc<Global<'a, U>>, seq: T) -> Self {
        Self { id, global, seq }
    }

    pub fn map(self, f: impl Fn(U) -> U) -> Generator<'a, impl Iterator<Item = U>, U> {
        Generator::new_with_id(self.id, self.global, self.seq.map(f))
    }

    pub fn filter(self, f: impl Fn(&U) -> bool) -> Generator<'a, impl Iterator<Item = U>, U> {
        Generator::new_with_id(self.id, self.global, self.seq.filter(f))
    }

    pub fn take(self, n: usize) -> Generator<'a, impl Iterator<Item = U>, U> {
        Generator::new_with_id(self.id, self.global, self.seq.take(n))
    }

    pub fn split_at(mut self, n: usize) -> (Generator<'a, impl Iterator<Item = U>, U>, Self) {
        let first = self.seq.split_at(n);
        (
            Generator::new_with_id(self.id, Arc::clone(&self.global), first),
            Generator::new_with_id(self.id + 1, self.global, self.seq),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generator_transform() {
        let global = Global::new(1..);
        let seq = global.take_seq(50);
        let gen = Generator::new(Arc::new(global), seq);
        let gen = gen.map(|x| x + 2).filter(|x| x % 3 == 0).take(5);
        let out: Vec<_> = gen.collect();
        assert_eq!(out, vec![3, 6, 9, 12, 15]);
    }
}
