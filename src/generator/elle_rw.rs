use std::sync::Mutex;

use j4rs::{Instance, InvocationArg};

use super::{RawGenerator, GENERATOR_CACHE_SIZE};
use crate::{
    cljinvoke, init_jvm, nsinvoke,
    op::{Op, Ops},
    utils::{pre_serialize, ToDe},
    with_jvm, CljNs, CLOJURE,
};

pub struct ElleRwGenerator {
    /// The namespace of the generator, default is `elle.rw-register`
    ns: CljNs,
    /// The clojure generator Instance.
    gen: Mutex<Option<Instance>>,
    /// The cached `Op`s of the generator. Because the clojure generator will
    /// generates infinite sequence, we can take some of them to cache. When the
    /// `Op`s run out, fetch new `Op`s from the clojure generator.
    cache: Ops,
}

impl ElleRwGenerator {
    pub fn new() -> j4rs::errors::Result<Self> {
        with_jvm(|_| {
            let ns = CLOJURE.require("elle.rw-register")?;
            Ok(Self {
                ns,
                gen: Mutex::new(None),
                cache: Ops(Vec::with_capacity(GENERATOR_CACHE_SIZE)),
            })
        })
    }
}

impl RawGenerator for ElleRwGenerator {
    type Item = anyhow::Result<Op>;
    /// It generates a batch of ops in one time, and reserves the gen `Instance`
    /// for next time to use.
    fn gen(&mut self) -> anyhow::Result<Op> {
        init_jvm();
        if let Some(op) = self.cache.pop() {
            return Ok(op);
        }
        let mut gen = self.gen.lock().expect("Failed to lock generator");
        if gen.is_none() {
            gen.replace(nsinvoke!(self.ns, "gen")?);
        }
        let cljgen = gen
            .take()
            .unwrap_or_else(|| unreachable!("gen should not be `None` after replacing it"));

        // avoid consuming the ownership of `two_seqs`
        let two_seqs = [InvocationArg::from(cljinvoke!(
            "split-at",
            GENERATOR_CACHE_SIZE as i32,
            cljgen
        )?)];

        let first_seq = pre_serialize(CLOJURE.var("first")?.invoke(&two_seqs)?)?;
        let ops: Ops = first_seq.to_de()?;
        self.cache = ops.rev();

        let second_seq = CLOJURE.var("second")?.invoke(&two_seqs)?;
        // update the elle gen
        gen.replace(second_seq);
        Ok(self
            .cache
            .pop()
            .unwrap_or_else(|| unreachable!("cache should not be empty after supplement")))
    }
}

impl Iterator for ElleRwGenerator {
    type Item = anyhow::Result<Op>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(self.gen())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{generator::RawGenerator, init_jvm};

    #[test]
    fn elle_gen_should_work() -> Result<(), Box<dyn std::error::Error>> {
        let mut gen = ElleRwGenerator::new()?;
        for _ in 0..GENERATOR_CACHE_SIZE * 2 + 10 {
            gen.gen()?;
        }
        Ok(())
    }
}
