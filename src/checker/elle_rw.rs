use j4rs::Instance;
use serde::Serialize;

use super::{CheckOption, SerializableCheckResult};
use crate::{
    history::SerializableHistoryList,
    init_jvm, nsinvoke,
    utils::{historify, FromSerde, ToDe},
    with_jvm, CljNs, CLOJURE,
};

pub struct ElleRwChecker {
    /// The namespace of the generator, default is `elle.rw-register`
    ns: CljNs,
}

impl Default for ElleRwChecker {
    fn default() -> Self {
        with_jvm(|_| Self {
            ns: CLOJURE
                .require("elle.rw-register")
                .expect("elle.rw-register ns should be available"),
        })
    }
}

impl super::Check for ElleRwChecker {
    fn check<F: Serialize, ERR: Serialize>(
        &self,
        history: &SerializableHistoryList<F, ERR>,
        options: Option<CheckOption>,
    ) -> anyhow::Result<SerializableCheckResult> {
        init_jvm();
        let h = historify(Instance::from_ser(history)?)?;
        let res = if let Some(options) = options {
            let op_clj = Instance::from_ser(options)?;
            nsinvoke!(self.ns, "check", op_clj, h)?
        } else {
            nsinvoke!(self.ns, "check", h)?
        };
        res.to_de::<SerializableCheckResult>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::Check;

    #[test]
    fn test_elle_rw_checker() -> anyhow::Result<()> {
        let checker = ElleRwChecker::default();
        let history_str = r#"[
          { "index": 0, "type": "invoke", "f": "txn", "value": [["w", 2, 1]], "time": 3291485317, "process": 0, "error": null }, 
          { "index": 1, "type": "ok", "f": "txn", "value": [["w", 2, 1]], "time": 3767733708, "process": 0, "error": null },
          { "index": 2, "type": "invoke", "f": "txn", "value": [["r", 2, null]], "time": 3891485317, "process": 1, "error": null }, 
          { "index": 3, "type": "ok", "f": "txn", "value": [["r", 2, 1]], "time": 3967733708, "process": 1, "error": null } 
        ]"#;
        let history: SerializableHistoryList = serde_json::from_str(history_str)?;
        let res = checker.check(&history, None)?;
        println!("{:#?}", res);
        // assert!(res.valid);
        Ok(())
    }
}
