use j4rs::Instance;
use log::{info, trace};
use serde::Serialize;

use super::{CheckOption, SerializableCheckResult};
use crate::{
    ffi::{historify, FromSerde, ToDe},
    history::SerializableHistoryList,
    nsinvoke, with_jvm, CljNs, CLOJURE,
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
            info!("check with option: {:?}", &option);
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
        ffi::read_edn,
        utils::log_init,
    };

    #[test]
    fn test_elle_rw_checker() -> anyhow::Result<()> {
        log_init();
        let checker = ElleRwChecker::default();
        let history = read_edn(include_str!("../../assets/ex_history.edn"))?;
        let history: SerializableHistoryList = history.to_de()?;
        let res = checker.check(
            &history,
            CheckOption::default()
                .consistency_models([ConsistencyModel::Serializable])
                .analyzer("wr-graph"),
        )?;
        println!("{:#?}", res);
        // assert!(res.valid);
        Ok(())
    }
}
