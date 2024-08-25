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
    fn get_op(&mut self) -> Result<Op>;
}

/// The generator. It's a wrapper for the clojure seq and global context.
pub struct Generator<'a, T: Iterator<Item = U>, U = Result<Op>> {
    /// generator id
    pub id: GeneratorId,
    /// A reference to the global context
    pub global: Arc<Global<'a>>,
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

impl<'a, T: Iterator<Item = Result<Op>>> Generator<'a, T> {
    pub fn new(global: Arc<Global<'a>>, seq: T) -> Self {
        let id = global.get_next_id();
        Self { id, global, seq }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generator_transform() {
        todo!()
    }
}
