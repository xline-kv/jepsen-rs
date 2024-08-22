mod context;
mod elle_rw;
use std::{collections::HashMap, sync::Arc};

use context::Global;
use madsim::runtime::NodeHandle;

use crate::op::Op;

pub type GeneratorId = u64;

/// Cache size for the generator.
pub const GENERATOR_CACHE_SIZE: usize = 100;

/// This trait is for the raw generator (clojure generator), it will only
/// generate ops infinitely.
pub trait RawGenerator {
    fn get_op(&mut self) -> anyhow::Result<Op>;
}

pub struct Generator {
    /// generator id
    pub id: GeneratorId,
    /// The raw generator
    pub gen: Box<dyn RawGenerator>,
    /// A reference to the global context
    pub global: Arc<Global>,
}
