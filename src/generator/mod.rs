mod elle_rw;
use std::collections::HashMap;

use madsim::runtime::NodeHandle;

use crate::op::Op;

pub type GeneratorId = u64;

/// Cache size for the generator.
pub const GENERATOR_CACHE_SIZE: usize = 100;

/// This trait is for the original generator (clojure generator), it will only
/// generate ops infinitely.
pub trait Generator {
    fn get_op(&mut self) -> anyhow::Result<Op>;
}
