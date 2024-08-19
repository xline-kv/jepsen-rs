use j4rs::Instance;

/// Generator
pub(crate) trait Generator {
    fn op(&self, ctx: Option<()>, test_ctx: Option<()>) -> Instance;
}
