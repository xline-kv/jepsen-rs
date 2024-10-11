pub mod context;
mod elle_rw;
use std::{collections::HashMap, sync::Arc};

pub use context::Global;
use log::trace;
use madsim::runtime::NodeHandle;

use crate::op::Op;

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
        trace!("takes {} items out from RawGenerator", n);
        out
    }
}
