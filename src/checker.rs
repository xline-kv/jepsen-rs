use j4rs::Instance;

/// Checker
trait Checker {
    /// The check function, returns a map like `{:valid? true}`
    fn check(history: Instance, test_ctx: Option<()>, checker_opts: Option<()>) -> Instance;
}
