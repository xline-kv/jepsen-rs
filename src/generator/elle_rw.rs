use std::sync::Mutex;

use anyhow::{anyhow, Context};
use j4rs::{Instance, InvocationArg};

use super::{Generator, GENERATOR_CACHE_SIZE};
use crate::{
    cljeval, cljinvoke, init_jvm, nseval, nsevalstr, nsinvoke,
    op::Op,
    utils::{pre_serialize, JsonSerde},
    CljNs, CLOJURE,
};

pub struct ElleRwGenerator {
    /// The namespace of the generator, default is `elle.rw-register`
    ns: CljNs,
    /// The clojure generator Instance.
    gen: Mutex<Option<Instance>>,
    /// The cached `Op`s of the generator. Because the clojure generator will
    /// generates infinite sequence, we can take some of them to cache. When the
    /// `Op`s run out, fetch new `Op`s from the clojure generator.
    cache: Vec<Op>,
}

impl ElleRwGenerator {
    pub fn new() -> j4rs::errors::Result<Self> {
        let ns = CLOJURE.require("elle.rw-register")?;
        Ok(Self {
            ns,
            gen: Mutex::new(None),
            cache: Vec::with_capacity(GENERATOR_CACHE_SIZE),
        })
    }
}

impl Generator for ElleRwGenerator {
    /// It generates a batch of ops in one time, and reserves the gen `Instance`
    /// for next time to use.
    fn get_op(&mut self) -> anyhow::Result<Op> {
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
        self.cache = Vec::<Op>::de(&first_seq.ser()?)?
            .into_iter()
            .rev()
            .collect();

        let second_seq = CLOJURE.var("second")?.invoke(&two_seqs)?;
        // update the elle gen
        gen.replace(second_seq);
        Ok(self
            .cache
            .pop()
            .unwrap_or_else(|| unreachable!("cache should not be empty after supplement")))
    }
}

#[cfg(test)]
mod test {
    use j4rs::JvmBuilder;

    use super::*;
    use crate::generator::Generator;

    #[test]
    fn elle_gen_should_work() -> Result<(), Box<dyn std::error::Error>> {
        init_jvm();
        let mut gen = ElleRwGenerator::new()?;
        for _ in 0..GENERATOR_CACHE_SIZE * 2 + 10 {
            gen.get_op()?;
        }
        Ok(())
    }
}
