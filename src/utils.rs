use anyhow::Result;
use j4rs::{errors::Result as jResult, Instance, InvocationArg};

use crate::{cljeval, cljinvoke, nsinvoke, with_jvm};

/// print a java instance
pub fn print(inst: Instance) {
    with_jvm(|jvm| {
        let system_class = jvm.static_class("java.lang.System").unwrap();
        let system_out_field = jvm.field(&system_class, "out").unwrap();
        jvm.invoke(&system_out_field, "println", &[InvocationArg::from(inst)])
            .unwrap();
    })
}

/// print a clojure instance
pub fn print_clj(inst: Instance) {
    println!("{}", clj_to_string(inst).die());
}

/// Convert a Clojure instance `j4rs::Instance` to a rust String
/// ```
/// use j4rs::{JvmBuilder, Instance};
/// use jepsen_rs::{cljeval, utils::{clj_to_string}};
/// let _jvm = JvmBuilder::new().build();
/// let res = clj_to_string(cljeval!((assoc {:a 1} :b "hello")).unwrap()).unwrap();
/// assert_eq!(res, "{:a 1, :b \"hello\"}".to_string());
/// ```
pub fn clj_to_string(inst: Instance) -> jResult<String> {
    java_to_string(&cljinvoke!("pr-str", inst)?)
}

/// Convert a java instance `j4rs::Instance` to a rust String
pub fn java_to_string(inst: &Instance) -> jResult<String> {
    with_jvm(|jvm| -> jResult<_> { jvm.to_rust(jvm.cast(inst, "java.lang.String")?) })
}

/// This trait is for printing error messages better than `unwrap()`
pub trait J4rsDie<T> {
    fn die(self) -> T;
}

impl<T> J4rsDie<T> for j4rs::errors::Result<T> {
    fn die(self) -> T {
        match self {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Error: {:?}", e);
                std::process::exit(1);
            }
        }
    }
}

/// This fn is to extract the value of generated ops from elle generator.
/// This function should be called before serialize the Instance.
pub fn pre_serialize(i: Instance) -> j4rs::errors::Result<Instance> {
    cljinvoke!("map", cljeval!(#(:value %)), i)
}
