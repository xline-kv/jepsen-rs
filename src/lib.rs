//! NOTE: Requires java 21 due to https://github.com/jepsen-io/jepsen/issues/585

mod checker;
mod generator;
mod history;
mod op;
pub mod utils;

use std::{borrow::Borrow, cell::OnceCell};

#[macro_use]
pub mod macros;

use j4rs::{Instance, InvocationArg, Jvm, JvmBuilder};

thread_local! {
    static JVM: OnceCell<Jvm> = const { OnceCell::new() };
}

pub fn init_jvm() {
    JVM.with(|cell| {
        cell.get_or_init(|| {
            let _jvm = JvmBuilder::new().build().expect("Failed to initialize JVM");
            Jvm::attach_thread().expect("Failed to attach JVM to thread")
        });
    })
}

pub fn with_jvm<F, R>(f: F) -> R
where
    F: FnOnce(&Jvm) -> R,
{
    JVM.with(|cell| {
        let jvm = cell.get_or_init(|| {
            let _jvm = JvmBuilder::new().build().expect("Failed to initialize JVM");
            Jvm::attach_thread().expect("Failed to attach JVM to thread")
        });
        f(jvm)
    })
}

pub fn read_edn(arg: &str) -> j4rs::errors::Result<Instance> {
    with_jvm(|_| cljinvoke!("load-string", arg))
}

fn invoke_clojure_java_api(
    method_name: &str,
    inv_args: &[impl Borrow<InvocationArg>],
) -> j4rs::errors::Result<Instance> {
    with_jvm(|jvm| {
        jvm.invoke(
            &with_jvm(|jvm| jvm.static_class("clojure.java.api.Clojure"))?,
            method_name,
            inv_args,
        )
    })
}

pub struct IFn {
    inner: Instance,
}

impl IFn {
    pub fn new(inner: Instance) -> Self {
        Self { inner }
    }

    pub fn invoke0(&self) -> j4rs::errors::Result<Instance> {
        self.invoke(&[] as &[InvocationArg])
    }

    pub fn invoke1(&self, arg: impl Into<InvocationArg>) -> j4rs::errors::Result<Instance> {
        self.invoke(&[arg.into()])
    }

    pub fn invoke(&self, args: &[impl Borrow<InvocationArg>]) -> j4rs::errors::Result<Instance> {
        with_jvm(|jvm| jvm.invoke(&self.inner, "invoke", args))
    }

    pub fn get_cls(&self, name: &str) -> j4rs::errors::Result<Instance> {
        with_jvm(|jvm| jvm.field(&self.inner, name))
    }

    pub fn into_inner(self) -> Instance {
        self.inner
    }
}

/// Clojure Namespace. A namespace should be created by `CljCore::require`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CljNs {
    ns: String,
}

impl CljNs {
    pub fn var(&self, name: &str) -> j4rs::errors::Result<IFn> {
        Self::var_inner(&self.ns, name)
    }

    fn var_inner(ns: &str, name: &str) -> j4rs::errors::Result<IFn> {
        Ok(IFn {
            inner: cljinvoke_java_api!("var", ns, name)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CljCore {
    ns: &'static str,
}

pub static CLOJURE: CljCore = CljCore { ns: "clojure.core" };

impl CljCore {
    pub fn require(&self, ns: &str) -> j4rs::errors::Result<CljNs> {
        CljNs::var_inner(self.ns, "require")?.invoke1(cljinvoke_java_api!("read", ns)?)?;
        Ok(CljNs { ns: ns.to_string() })
    }

    pub fn var(&self, name: &str) -> j4rs::errors::Result<IFn> {
        CljNs::var_inner(self.ns, name)
    }
}

impl Default for CljCore {
    fn default() -> Self {
        CLOJURE.clone()
    }
}

#[cfg(test)]
mod test {
    use utils::pre_serialize;

    use self::utils::print_clj;
    use super::*;
    use crate::utils::print;

    #[test]
    fn test_elle_check() -> Result<(), Box<dyn std::error::Error>> {
        init_jvm();
        let r = CLOJURE.require("elle.rw-register")?;
        let h = CLOJURE.require("jepsen.history")?;
        let history = read_edn(include_str!("../assets/ex_history.edn"))?;
        let history = nsinvoke!(h, "history", history)?;
        let res = nsinvoke!(r, "check", history)?;
        print_clj(res);
        Ok(())
    }

    #[test]
    fn test_elle_gen() -> Result<(), Box<dyn std::error::Error>> {
        init_jvm();
        let r = CLOJURE.require("elle.rw-register")?;
        let gen = nsinvoke!(r, "gen")?;
        let take = cljinvoke!("take", 5, gen)?;
        let value = pre_serialize(take)?;
        print_clj(value);
        Ok(())
    }

    #[test]
    fn elle_gen_analysis() -> Result<(), Box<dyn std::error::Error>> {
        init_jvm();
        let r = CLOJURE.require("elle.rw-register")?;
        let h = CLOJURE.require("jepsen.history")?;
        let gen = r.var("gen")?.invoke0()?;
        let history = cljinvoke!("take", 10, gen)?;
        let res = nsinvoke!(r, "check", nsinvoke!(h, "history", history)?)?;
        print(res);
        Ok(())
    }

    /// We can define a function in namespace, and call it later.
    #[test]
    fn test_defn_in_ns() -> Result<(), Box<dyn std::error::Error>> {
        init_jvm();
        let _x = cljeval!((defn test [] (str "hello" "world")))?;
        let y = cljeval!((test))?;
        print_clj(y);
        Ok(())
    }
}
