use j4rs::Instance;

use crate::{cljeval, eval, generator::Generator, CljCore};

struct Gen;

impl Gen {
    fn new() -> Self {
        let clj = CljCore::new();
        let r = clj.require("elle.rw-register").unwrap();
        let gen = r.var("gen").unwrap().invoke0().unwrap();
        cljeval!(
            (println "hello")
        )
        .unwrap();

        Self {}
    }
}

impl Generator for Gen {
    fn op(&self, ctx: Option<()>, test_ctx: Option<()>) -> Instance {
        todo!()
    }
}
