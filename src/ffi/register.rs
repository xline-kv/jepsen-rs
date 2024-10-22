//! Every clojure code should be registered in this module.

use std::{collections::HashSet, sync::Mutex};

use once_cell::sync::Lazy as LazyLock;

use crate::{cljevalstr, init_jvm, CljNs, CLOJURE};

/// The global NsRegister singleton
pub static NS_REGISTER: LazyLock<NsRegister> = LazyLock::new(NsRegister::new);

/// The [`NsRegister`]. This is used to execute any clojure code from user.
///
/// If a clojure code is registered, its namespace will be added to clojure
/// global namespace. [`NsRegister`] provides `get()` function to get the
/// namespace.
#[derive(Default, Debug)]
pub struct NsRegister {
    /// The registered namespaces.
    nss: Mutex<HashSet<String>>,
}

impl NsRegister {
    /// Create a new NsRegister
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a namespace
    pub fn register(&self, code: &str) -> anyhow::Result<()> {
        init_jvm();
        cljevalstr!(code)?;
        let ns_name = parse_code_and_get_ns_name(code)
            .expect("Failed to register ns: cannot find ns in clojure code.");
        self.nss.lock().unwrap().insert(ns_name);
        Ok(())
    }

    /// Get the namespace
    pub fn get(&self, ns_name: &str) -> Option<CljNs> {
        init_jvm();
        self.nss.lock().unwrap().get(ns_name).map(|s| {
            CLOJURE
                .require(s)
                .expect("registered ns should be available for requiring")
        })
    }

    /// Hacky way to get or register a namespace which is **already in**
    /// `src/clojure`.
    pub fn get_or_register(&self, ns_name: &str) -> CljNs {
        if let Some(ns) = self.get(ns_name) {
            return ns;
        }
        let code = match ns_name {
            "serde" => include_str!("../clojure/serde.clj"),
            _ => unreachable!("ns not in `src/clojure`"),
        };
        self.register(code).expect("Failed to register ns");
        self.get(ns_name)
            .expect("ns must exists because it's registered")
    }
}

/// Get the namespace from the clojure source code
fn parse_code_and_get_ns_name(s: &str) -> Option<String> {
    if let Some(pos) = s.find("(ns ") {
        let start = pos + 4;
        let end = s[start..]
            .chars()
            .position(|c| c.is_whitespace() || c == ')')
            .unwrap_or_else(|| s.len() - start)
            + start;
        let keyword = &s[start..end];
        Some(keyword.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_code_and_get_ns_name() {
        assert_eq!(parse_code_and_get_ns_name("(ns a)"), Some("a".to_string()));
        assert_eq!(
            parse_code_and_get_ns_name("(ns a.b)"),
            Some("a.b".to_string())
        );
        assert_eq!(
            parse_code_and_get_ns_name("(ns abc)"),
            Some("abc".to_string())
        );
        assert_eq!(
            parse_code_and_get_ns_name("(ns a\n.d)"),
            Some("a".to_string())
        );
        assert_eq!(
            parse_code_and_get_ns_name("(ns a .b)"),
            Some("a".to_string())
        );
    }

    #[test]
    fn test_ns_register() {
        let reg = NsRegister::new();
        reg.register("(ns a)").unwrap();
        assert!(reg.get("a").is_some());
    }
}
