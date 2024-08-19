use j4rs::{errors::Result, Instance, InvocationArg};

use crate::{cljinvoke, with_jvm, CljCore};

pub(crate) fn print(inst: Instance) {
    with_jvm(|jvm| {
        let system_class = jvm.static_class("java.lang.System").unwrap();
        let system_out_field = jvm.field(&system_class, "out").unwrap();
        jvm.invoke(&system_out_field, "println", &[InvocationArg::from(inst)])
            .unwrap();
    })
}

pub(crate) fn print_clj(inst: Instance) {
    println!("{}", clj_to_string(inst).die());
}

pub fn clj_to_string(inst: Instance) -> Result<String> {
    with_jvm(|jvm| -> Result<_> {
        let res = cljinvoke!("pr-str", inst)?;
        let instance = jvm.cast(&res, "java.lang.String")?;
        jvm.to_rust(instance)
    })
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
