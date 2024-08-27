pub mod iter;
use anyhow::Result;
pub use iter::*;
use j4rs::{errors::Result as jResult, Instance, InvocationArg};
use serde::Serialize;

use crate::{cljeval, cljinvoke, nsinvoke, with_jvm, CLOJURE};

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
    with_jvm(|_| java_to_string(&cljinvoke!("pr-str", inst)?))
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
pub fn pre_serialize(i: Instance) -> jResult<Instance> {
    cljinvoke!("map", cljeval!(#(:value %)), i)
}

/// Convert a clojure instance to json string
pub fn clj_jsonify(inst: Instance) -> jResult<String> {
    with_jvm(|_| {
        let json = CLOJURE.require("clojure.data.json")?;
        java_to_string(&nsinvoke!(json, "write-str", inst)?)
    })
}

/// Convert a json string to clojure instance
pub fn clj_from_json(s: &str) -> jResult<Instance> {
    with_jvm(|_| {
        let json = CLOJURE.require("clojure.data.json")?;
        nsinvoke!(json, "read-str", s)
    })
}

/// Convert any rust struct which impl Serialize to clojure instance
pub trait FromSerde {
    fn from_ser<T: Serialize>(s: T) -> Result<Self>
    where
        Self: Sized;
}

impl FromSerde for Instance {
    fn from_ser<T: Serialize>(s: T) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(clj_from_json(&serde_json::to_string(&s)?)?)
    }
}

/// Convert clojure instance to any rust struct which impl Serialize
pub trait ToDe {
    fn to_de<T: for<'de> serde::Deserialize<'de>>(self) -> Result<T>;
}

impl ToDe for Instance {
    fn to_de<T: for<'de> serde::Deserialize<'de>>(self) -> Result<T> {
        Ok(serde_json::from_str(&clj_jsonify(self)?)?)
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;
    use crate::init_jvm;

    #[test]
    fn test_convertion_between_clojure_and_rust() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct TestSer {
            a: i32,
            b: String,
        }
        init_jvm();

        let s = cljeval!((assoc {:a 1} :b "hello")).unwrap();
        let res: TestSer = s.to_de().unwrap();
        assert_eq!(
            res,
            TestSer {
                a: 1,
                b: "hello".to_string()
            }
        );

        let s = TestSer {
            a: 1,
            b: "hello".to_string(),
        };
        let res: Instance = Instance::from_ser(&s).unwrap();
        print_clj(res);
    }
}
