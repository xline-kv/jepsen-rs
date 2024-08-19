//! NOTE: Requires java 21 due to https://github.com/jepsen-io/jepsen/issues/585

mod checker;
mod context;
mod generator;
mod jtests;
pub mod utils;
use std::{borrow::Borrow, cell::RefCell};

use j4rs::{Instance, InvocationArg, Jvm};

/// Reads data in the edn format
#[macro_export]
macro_rules! cljread {
    ($($char:tt)*) => {
        read(stringify!($($char)*))
    };
}

/// Evaluate the string
#[macro_export]
macro_rules! cljeval {
    ($($char:tt)*) => {
        eval(stringify!($($char)*))
    };
}

/// Invoke a clojure class method
#[macro_export]
macro_rules! cljinvoke {
    ($name:expr) => {
        invoke_clojure_class($name, &[])
    };
    ($name:expr, $($args:expr),*) => {
        || -> j4rs::errors::Result<Instance> {
            invoke_clojure_class($name, &[$(InvocationArg::try_from($args)?),*])
        } ()
    };
}

/// Invoke a clojure class method
#[macro_export]
macro_rules! nsinvoke {
    ($ns:expr, $var:expr) => {
        || -> j4rs::errors::Result<Instance> {
            $ns.var($var)?.invoke(&[] as &[InvocationArg])
        } ()
    };
    ($ns:expr, $var:expr, $($args:expr),*) => {
        || -> j4rs::errors::Result<Instance> {
            $ns.var($var)?.invoke(&[$(InvocationArg::try_from($args)?),*])
        } ()
    };
}

thread_local! {
    static JVM: RefCell<Option<Jvm>> = const { RefCell::new(None) };
}

fn with_jvm<F, R>(f: F) -> R
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
    cljinvoke!("read", arg).unwrap()
}

/// eval the given clojure string
pub fn eval(arg: &str) -> j4rs::errors::Result<Instance> {
    let clj = CljCore::new();
    nsinvoke!(clj, "load-string", arg)
}

pub fn invoke_clojure_class(
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

    pub fn into_inner(self) -> Instance {
        self.inner
    }
}

/// Clojure Namespace
pub struct CljNs {
    ns: String,
}

impl CljNs {
    pub fn new(ns: impl Into<String>) -> Self {
        Self { ns: ns.into() }
    }

    pub fn var(&self, name: &str) -> j4rs::errors::Result<IFn> {
        Self::var_inner(&self.ns, name)
    }

    fn var_inner(ns: &str, name: &str) -> j4rs::errors::Result<IFn> {
        Ok(IFn {
            inner: cljinvoke!("var", ns, name)?,
        })
    }
}

pub struct CljCore {
    ns: &'static str,
}

impl CljCore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn require(&self, ns: &str) -> j4rs::errors::Result<CljNs> {
        CljNs::var_inner(self.ns, "require")?.invoke1(read(ns))?;
        Ok(CljNs::new(ns.to_string()))
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
    use utils::J4rsDie;

    use self::utils::print_lazy;
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
        let t = nsinvoke!(clj, "take", 10, gen)?;
        print_lazy(t);

        Ok(())
    }

    #[test]
    fn elle_gen_analysis() -> Result<(), Box<dyn std::error::Error>> {
        let _jvm = JvmBuilder::new().build()?;
        let clj = CljCore::new();
        let r = clj.require("elle.rw-register")?;
        let g = clj.require("jepsen.generator")?;
        let h = clj.require("jepsen.history")?;
        let gen = r.var("gen")?.invoke0();

        let history = nsinvoke!(clj, "take", 2, gen)?;

        let assocfn = cljeval!(
            #(assoc % :new-key :new-value)
        )?;

        let res = nsinvoke!(clj, "map", assocfn, history)?;

        // let assoc = clj.var("assoc");
        // clj.var("map").invoke(&[
        //     InvocationArg::from(assoc.into_inner()),
        //     Clojure.var("clojure.core", "%"),
        // ]);
        print_lazy(res);

        // let res = r.var("check").invoke1(h.var("history").invoke1(history));
        // print(res);

        Ok(())
    }

    #[test]
    fn mytest() -> Result<(), Box<dyn std::error::Error>> {
        let _jvm = JvmBuilder::new().build()?;
        Ok(())
    }
}
