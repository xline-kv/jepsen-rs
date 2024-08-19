use j4rs::errors::Result;
use j4rs::{ClasspathEntry, Instance, InvocationArg, Jvm, JvmBuilder};
fn main() -> Result<()> {
    let entry = ClasspathEntry::new("clj/target/lib1-1.2.2-standalone.jar");
    let jvm: Jvm = JvmBuilder::new().classpath_entry(entry).build()?;
    let empty_arr_instance = jvm
        .create_java_array("java.lang.String", &[] as &[InvocationArg])
        .unwrap();
    let empty_arg = InvocationArg::from(empty_arr_instance);
    let _res = jvm.invoke_static(
        "mytest",     // The Java class to invoke
        "main",       // The static method of the Java class to invoke
        &[empty_arg], // The `InvocationArg`s to use for the invocation - empty for this example
    )?;
    Ok(())
}

pub(crate) fn print(jvm: Jvm, inst: Instance) {
    let system_class = jvm.static_class("java.lang.System").unwrap();
    let system_out_field = jvm.field(&system_class, "out").unwrap();
    jvm.invoke(&system_out_field, "println", &[InvocationArg::from(inst)])
        .unwrap();
}
