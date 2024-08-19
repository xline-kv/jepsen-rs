/// Reads data in the edn format
#[macro_export]
macro_rules! cljread {
    ($($char:tt)*) => {
        cljinvoke_java_api!("read", stringify!($($char)*)).unwrap()
    };
}

/// Invoke a clojure function.
/// ```
/// use j4rs::{JvmBuilder, Instance, InvocationArg};
/// use jepsen_rs::{cljinvoke, CljCore};
/// let _jvm = JvmBuilder::new().build();
/// cljinvoke!("println", "hello").unwrap();
/// ```
#[macro_export]
macro_rules! cljinvoke {
    ($name:expr) => {
        $crate::CljCore::default().var($name).invoke0()
    };
    ($name:expr, $($args:expr),*) => {
        || -> j4rs::errors::Result<Instance> {
            $crate::CljCore::default().var($name)?.invoke(&[$(j4rs::InvocationArg::try_from($args)?),*])
        } ()
    };
}

/// Evaluate the Clojure string
/// ```
/// use j4rs::{JvmBuilder, Instance, InvocationArg};
/// use jepsen_rs::{cljinvoke, cljeval, CljCore};
/// let _jvm = JvmBuilder::new().build();
/// cljeval!((println "hello")).unwrap();
/// ```
#[macro_export]
macro_rules! cljeval {
    ($($char:tt)*) => {
        $crate::cljinvoke!("load-string", stringify!($($char)*))
    };
}

/// Invoke a clojure from clojure.java.api.Clojure
/// https://clojure.github.io/clojure/javadoc/clojure/java/api/Clojure.html
macro_rules! cljinvoke_java_api {
    ($name:expr) => {
        $crate::invoke_clojure_java_api($name, &[])
    };
    ($name:expr, $($args:expr),*) => {
        || -> j4rs::errors::Result<Instance> {
            $crate::invoke_clojure_java_api($name, &[$(j4rs::InvocationArg::try_from($args)?),*])
        } ()
    };
}

/// Invoke a clojure class method from namespace
/// ```
/// use j4rs::{JvmBuilder, Instance, InvocationArg};
/// use jepsen_rs::{nsinvoke, cljeval, CljCore};
/// let _jvm = JvmBuilder::new().build();
/// let g = CljCore::default().require("jepsen.generator").unwrap();
/// let res = nsinvoke!(g, "phases", cljeval!({:f :write, :value 3} {:f :read}).unwrap()).unwrap();
/// ```
#[macro_export]
macro_rules! nsinvoke {
    ($ns:expr, $var:expr) => {
        || -> j4rs::errors::Result<Instance> {
            $ns.var($var)?.invoke(&[] as &[j4rs::InvocationArg])
        } ()
    };
    ($ns:expr, $var:expr, $($args:expr),*) => {
        || -> j4rs::errors::Result<Instance> {
            $ns.var($var)?.invoke(&[$(j4rs::InvocationArg::try_from($args)?),*])
        } ()
    };
}

// WARNING: load-string already refers to: #'clojure.core/load-string in
// namespace: jepsen.generator, being replaced by:
// #'jepsen.generator/load-string
