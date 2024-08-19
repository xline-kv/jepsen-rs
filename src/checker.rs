use j4rs::{errors::Result, Instance};

/// Checker
pub trait Checker {
    /// The check function, returns a map like `{:valid? true}`
    fn check(history: Instance) -> Result<Instance>;
}
