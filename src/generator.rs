use std::collections::HashMap;

use j4rs::Instance;
use madsim::runtime::NodeHandle;

use crate::op::Op;

pub type GeneratorId = u64;

/// A group of Generators
pub struct Generators {
    inner: HashMap<GeneratorId, Generator>,
}

/// Generator.
pub struct Generator {
    id: GeneratorId,
    node: NodeHandle,
    create_time: std::time::Instant,
    elle_gen: Instance,
}

/// Gen trait. Only has op function and no context provided, the context should
/// be in [`Generator`].
#[async_trait::async_trait]
pub trait Gen {
    async fn op(&self) -> Option<Op>;
}

#[cfg(test)]
mod test {}
