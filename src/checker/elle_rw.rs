use super::Checker;
use crate::{with_jvm, CljNs, CLOJURE};

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

impl Checker for ElleRwChecker {
    fn ns(&self) -> &CljNs {
        &self.ns
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        checker::{Check, CheckOption, ConsistencyModel},
        ffi::{read_edn, ToDe},
        history::SerializableHistoryList,
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
