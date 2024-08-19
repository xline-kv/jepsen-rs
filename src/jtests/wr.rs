use j4rs::Instance;

use crate::{cljeval, generator::Generator, CljCore};

struct Gen;

impl Gen {
    fn new() -> Self {
        let clj = CljCore::new();
        let r = clj.require("elle.rw-register").unwrap();
        let gen = r.var("gen").unwrap().invoke0().unwrap();

        Self {}
    }
}
