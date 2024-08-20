//! NOTE: Requires java 21 due to https://github.com/jepsen-io/jepsen/issues/585

mod checker;
mod context;
mod generator;
mod history;
mod jtests;
mod op;
pub mod utils;
use std::{borrow::Borrow, cell::RefCell};
#[macro_use]
pub mod macros;

use j4rs::{Instance, InvocationArg, Jvm};

thread_local! {
    static JVM: RefCell<Option<Jvm>> = const { RefCell::new(None) };
}

pub fn with_jvm<F, R>(f: F) -> R
where
    F: FnOnce(&Jvm) -> R,
{
    JVM.with(|cell| {
        if let Ok(mut jvm) = cell.try_borrow_mut() {
            if jvm.is_none() {
                jvm.replace(Jvm::attach_thread().unwrap());
            }
        }
        f(cell.borrow().as_ref().unwrap())
    })
}

pub fn read(arg: &str) -> Instance {
    cljinvoke_java_api!("read", arg).unwrap()
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

impl CljCore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn require(&self, ns: &str) -> j4rs::errors::Result<CljNs> {
        CljNs::var_inner(self.ns, "require")?.invoke1(read(ns))?;
        Ok(CljNs { ns: ns.to_string() })
    }

    pub fn var(&self, name: &str) -> j4rs::errors::Result<IFn> {
        CljNs::var_inner(self.ns, name)
    }
}

impl Default for CljCore {
    fn default() -> Self {
        Self { ns: "clojure.core" }
    }
}

#[cfg(test)]
mod test {
    use j4rs::JvmBuilder;

    use self::utils::print_clj;
    use super::*;
    use crate::utils::print;

    #[test]
    fn test_elle_analysis() -> Result<(), Box<dyn std::error::Error>> {
        let _jvm = JvmBuilder::new().build()?;
        let clj = CljCore::new();
        let r = clj.require("elle.rw-register")?;
        let h = clj.require("jepsen.history")?;
        let history = cljeval!(
           [{:index 0 :time 0 :type :invoke :process 0 :f :txn :value [[:r 1 nil] [:w 1 2]]}
            {:index 1 :time 1 :type :invoke :process 1 :f :txn :value [[:r 1 nil] [:w 1 3]]}
            {:index 2 :time 2 :type :ok :process 0 :f :txn :value [[:r 1 2] [:w 1 2]]}
            {:index 3 :time 3 :type :ok :process 1 :f :txn :value [[:r 1 2] [:w 1 3]]}]
        )?;
        let jh = h.var("history")?.invoke1(history)?;
        let res = r.var("check")?.invoke1(jh)?;
        print(res);
        Ok(())
    }

    #[test]
    fn test_elle_gen() -> Result<(), Box<dyn std::error::Error>> {
        let _jvm = JvmBuilder::new().build()?;
        let clj = CljCore::new();
        let r = clj.require("elle.rw-register")?;
        let gen = nsinvoke!(r, "gen")?;
        let take = nsinvoke!(clj, "take", 5, gen)?;
        let value = cljinvoke!("map", cljeval!(#(:value %)), take)?;
        print_clj(value);
        Ok(())
    }

    #[test]
    fn elle_gen_analysis() -> Result<(), Box<dyn std::error::Error>> {
        let _jvm = JvmBuilder::new().build()?;
        let clj = CljCore::new();
        let r = clj.require("elle.rw-register")?;
        // let g = clj.require("jepsen.generator")?;
        let h = clj.require("jepsen.history")?;
        let gen = r.var("gen")?.invoke0();
        let history = nsinvoke!(clj, "take", 10, gen)?;
        let res = nsinvoke!(r, "check", nsinvoke!(h, "history", history)?)?;
        print(res);
        Ok(())
    }

    /// We can define a function in namespace, and call it later.
    #[test]
    fn test_defn_in_ns() -> Result<(), Box<dyn std::error::Error>> {
        let _jvm = JvmBuilder::new().build()?;
        let _x = cljeval!((defn test [] (str "hello" "world")))?;
        let y = cljeval!((test))?;
        print_clj(y);
        Ok(())
    }
}
