/// Reads data in edn format
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
        || -> j4rs::errors::Result<j4rs::Instance> {
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
        || -> j4rs::errors::Result<j4rs::Instance> {
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
        || -> j4rs::errors::Result<j4rs::Instance> {
            $ns.var($var)?.invoke(&[] as &[j4rs::InvocationArg])
        } ()
    };
    ($ns:expr, $var:expr, $($args:expr),*) => {
        || -> j4rs::errors::Result<j4rs::Instance> {
            $ns.var($var)?.invoke(&[$(j4rs::InvocationArg::try_from($args)?),*])
        } ()
    };
}

/// Invoke a clojure class method from namespace
/// ```
/// use j4rs::{JvmBuilder, Instance, InvocationArg};
/// use jepsen_rs::{nseval, cljeval, CljCore};
/// let _jvm = JvmBuilder::new().build();
/// let g = CljCore::default().require("jepsen.generator").unwrap();
/// let res = nseval!(g, (phases {:f :write, :value 3} {:f :read})).unwrap();
/// ```
#[macro_export]
macro_rules! nseval {
    ($ns:expr, ($($char:tt)*)) => {
        $crate::nsevalstr!($ns, stringify!($($char)*))
    };
}

#[macro_export]
macro_rules! nsevalstr {
    ($ns:expr, $str:expr) => {
        || -> j4rs::errors::Result<j4rs::Instance> {
            let s = $str;
            let first_space_pos = s.find(' ').unwrap_or(s.len());
            let (x, y) = s.split_at(first_space_pos);
            let (first, rest) = (x.trim(), y.trim());
            let ns_ifn = $ns.var(first)?;
            let load_str_ifn = $crate::Clojure::default().var("load-string")?;
        }()
    };
}

#[cfg(test)]
mod tests {
    use j4rs::JvmBuilder;

    use super::*;
    use crate::{utils::print_clj, CljCore};

    #[test]
    fn mytest() -> Result<(), Box<dyn std::error::Error>> {
        let _jvm = JvmBuilder::new().build()?;
        let ns = CljCore::default().require("elle.rw-register")?;
        let gen = ns.var("gen")?;
        let res = cljinvoke!("take", 100, gen.invoke0()?)?;
        print_clj(res);
        Ok(())
    }
}
