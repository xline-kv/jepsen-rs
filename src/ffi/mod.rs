pub mod register;

use anyhow::Result;
use j4rs::{errors::Result as jResult, Instance, InvocationArg};
use register::NS_REGISTER;
use serde::Serialize;

use crate::{cljinvoke, nsinvoke, with_jvm, CLOJURE};

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

/// Invoke a method of `java.util.Objects`
fn object_method(insts: Vec<Instance>, method_name: &str) -> jResult<Instance> {
    with_jvm(|jvm| -> jResult<_> {
        let object_class = jvm.static_class("java.util.Objects")?;
        jvm.invoke(
            &object_class,
            method_name,
            insts
                .into_iter()
                .map(&InvocationArg::from)
                .collect::<Vec<_>>()
                .as_slice(),
        )
    })
}

/// Check if two java instances are equals
///
/// WARN: two clojure instances may be not equals in java.
pub fn equals_java(a: Instance, b: Instance) -> jResult<bool> {
    with_jvm(|jvm| -> jResult<_> { jvm.to_rust(object_method(vec![a, b], "equals")?) })
}

/// Check if two clojure instances are equals
pub fn equals_clj(a: Instance, b: Instance) -> jResult<bool> {
    with_jvm(|jvm| -> jResult<_> { jvm.to_rust(cljinvoke!("=", a, b)?) })
}

/// Load an edn format data as clojure instance
///
/// ```
/// use j4rs::{JvmBuilder, Instance, InvocationArg};
/// use jepsen_rs::{CljCore, cljeval};
/// use jepsen_rs::utils::ffi::{equals, read_edn};
/// let _jvm = JvmBuilder::new().build();
/// let res = read_edn("(assoc {:a 1} :b \"hello\")").unwrap();
/// assert!(equals(res, cljeval!({:a 1, :b "hello"}).unwrap()).unwrap());
/// ```
pub fn read_edn(arg: &str) -> j4rs::errors::Result<Instance> {
    with_jvm(|_| cljinvoke!("load-string", arg))
}

/// This fn is to extract the value of generated ops from elle generator.
/// This function should be called before serialize the Instance.
pub fn pre_serialize(i: Instance) -> jResult<Instance> {
    // In rust 1.74, we cannot use `cljeval!(#(:value %))` here, otherwise
    // the jvm will panic with Invalid token: `:`
    with_jvm(|_| cljinvoke!("map", cljinvoke!("load-string", "#(:value %)"), i))
}

/// This function converts a clojure edn instance to jepsen history instance.
pub fn historify(i: Instance) -> jResult<Instance> {
    with_jvm(|_| {
        let h = CLOJURE.require("jepsen.history")?;
        nsinvoke!(h, "history", i)
    })
}

/// Convert a clojure instance to json string
pub fn clj_jsonify(inst: Instance) -> jResult<String> {
    with_jvm(|_| {
        let serde = NS_REGISTER.get_or_register("serde");
        java_to_string(&nsinvoke!(serde, "serialize-with-key-type", inst)?)
    })
}

/// Convert a json string to clojure instance
pub fn clj_from_json(s: &str) -> jResult<Instance> {
    with_jvm(|_| {
        let serde = NS_REGISTER.get_or_register("serde");
        nsinvoke!(serde, "deserialize-list-to-vec", s)
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
    use crate::{cljeval, init_jvm};

    #[test]
    fn test_serde_between_clojure_and_rust() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct TestSer {
            #[serde(rename = ":a")]
            a: i32,
            #[serde(rename = ":b")]
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

    #[test]
    fn test_equals() {
        init_jvm();
        let a = cljeval!((assoc {:a 1} :b "hello")).unwrap();
        let b = cljeval!((assoc {:a 1} :b "hello")).unwrap();
        assert!(equals_java(a, b).unwrap());
        let a = cljeval!((assoc {:a 1} :b "hello")).unwrap();
        let b = cljeval!((assoc {:a 1} :b "hello")).unwrap();
        assert!(equals_clj(a, b).unwrap());
    }

    #[test]
    fn json_serde_should_be_consistent() {
        init_jvm();
        let edn_str = r#"{:type :invoke, :f :txn, :value [[:w 2 1]], :time 3291485317, :process 0, :index 0}"#;
        let clj_obj = read_edn(edn_str).die();
        let json_str = clj_jsonify(clj_obj).die();
        println!("{}", json_str);
        let clj_obj2 = clj_from_json(&json_str).die();
        let res = clj_to_string(clj_obj2).die();
        assert_eq!(edn_str, res);
    }

    /// more complex example of json serde, use a real history for testing.
    #[test]
    fn json_serde_should_be_consistent_c() {
        init_jvm();
        let edn_str = include_str!("../../assets/ex_history.edn");
        let clj_obj = read_edn(edn_str).die();
        let clj_obj_clone = read_edn(edn_str).die();
        let json_str = clj_jsonify(clj_obj).die();
        println!("{}", json_str);
        let clj_obj2 = clj_from_json(&json_str).die();
        assert!(equals_clj(clj_obj_clone, clj_obj2).unwrap());
    }
}
