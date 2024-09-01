use j4rs::Instance;
use log::{info, trace};
use serde::Serialize;

use super::{CheckOption, SerializableCheckResult};
use crate::{
    history::SerializableHistoryList,
    nsinvoke,
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
        option: CheckOption,
    ) -> anyhow::Result<SerializableCheckResult> {
        with_jvm(|_| {
            let h = historify(Instance::from_ser(history)?)?;
            trace!("historify done");
            info!("check with option: {:?}", serde_json::to_string(&option));
            let op_clj = Instance::from_ser(option)?;
            let res = nsinvoke!(self.ns, "check", op_clj, h)?;
            trace!("check done");
            res.to_de::<SerializableCheckResult>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        checker::{Check, ConsistencyModel},
        utils::log_init,
    };

    #[test]
    fn test_elle_rw_checker() -> anyhow::Result<()> {
        log_init();
        let checker = ElleRwChecker::default();
        let history_str = r#"[
          { "index": 0, "type": "invoke", "f": "txn", "value": [["w", 2, 1]], "time": 3291485317, "process": 0, "error": null }, 
          { "index": 1, "type": "ok", "f": "txn", "value": [["w", 2, 1]], "time": 3767733708, "process": 0, "error": null },
          { "index": 2, "type": "invoke", "f": "txn", "value": [["r", 2, null]], "time": 3891485317, "process": 1, "error": null }, 
          { "index": 3, "type": "ok", "f": "txn", "value": [["r", 2, 1]], "time": 3967733708, "process": 1, "error": null } 
        ]"#;
        let history: SerializableHistoryList = serde_json::from_str(history_str)?;
        let res = checker.check(
            &history,
            CheckOption::default()
                .consistency_models(ConsistencyModel::Serializable)
                .analyzer("wr-graph"),
        )?;
        println!("{:#?}", res);
        // assert!(res.valid);
        Ok(())
    }
}
