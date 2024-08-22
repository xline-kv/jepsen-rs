mod context;
mod elle_rw;
use std::{collections::HashMap, sync::Arc};

use context::Global;
use madsim::runtime::NodeHandle;

use crate::op::Op;

/// The id of the generator. Each [`GeneratorId`] corresponds to one thread.
pub type GeneratorId = u64;

/// Cache size for the generator.
pub const GENERATOR_CACHE_SIZE: usize = 100;

/// This trait is for the raw generator (clojure generator), it will only
/// generate ops infinitely.
pub trait RawGenerator {
    fn get_op(&mut self) -> anyhow::Result<Op>;
}

pub struct Generator<T: Iterator<Item = U>, U = anyhow::Result<Op>> {
    /// generator id
    pub id: GeneratorId,
    /// A reference to the global context
    pub global: Arc<Global>,
    /// The generator sequence
    pub seq: T,
}

impl<T: Iterator<Item = anyhow::Result<Op>>> Generator<T> {
    pub fn new(global: Arc<Global>, seq: T) -> Self {
        let id = global.get_next_id();
        Self { id, global, seq }
    }
}
