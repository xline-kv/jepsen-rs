use std::sync::Mutex;

use anyhow::{anyhow, Context};
use j4rs::{Instance, InvocationArg};

use super::{Generator, GENERATOR_CACHE_SIZE};
use crate::{
    cljeval, cljinvoke, nseval, nsevalstr, nsinvoke, op::Op, utils::JsonSerde, CljCore, CljNs,
};

pub struct ElleGenerator {
    /// The namespace of the generator, default is `elle.rw-register`
    ns: CljNs,
    /// The cached `Op`s of the generator. Because the clojure generator will
    /// generates infinite sequence, we can take some of them to cache. When the
    /// `Op`s run out, fetch new `Op`s from the clojure generator.
    cache: Vec<Op>,
}

impl ElleGenerator {
    pub fn new() -> j4rs::errors::Result<Self> {
        let ns = CljCore::default().require("elle.rw-register")?;
        Ok(Self {
            ns,
            cache: Vec::with_capacity(GENERATOR_CACHE_SIZE),
        })
    }
}

impl Generator for ElleGenerator {
    fn get_op(&mut self) -> anyhow::Result<Op> {
        if let Some(op) = self.cache.pop() {
            return Ok(op);
        }
        let gen = self.ns.var("gen")?;
        let res = cljinvoke!("take", GENERATOR_CACHE_SIZE as i32, gen.invoke0()?)?;
        let res = cljinvoke!("map", cljeval!(#(:value %)), res)?;
        let ser = res.ser()?;
        println!("{}", ser);
        self.cache = Vec::<Op>::de(&ser)?.into_iter().rev().collect();
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
    fn test_elle_gen() -> Result<(), Box<dyn std::error::Error>> {
        let _jvm = JvmBuilder::new().build()?;
        let mut gen = ElleGenerator::new()?;
        let op = gen.get_op()?;
        println!("{:?}", op);
        Ok(())
    }
}
