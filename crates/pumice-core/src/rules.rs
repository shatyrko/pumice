//! Declarative cleanup rules, shipped as TOML inside the binary and checked
//! by a unit test (every pattern has to pass the safety policy).

use crate::safety::{self, Policy};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    #[default]
    Delete,
    Trash,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub risk: Risk,
    pub paths: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub min_age_days: u64,
    /// Process names that must not be running (a browser holding its cache).
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub action: Action,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Deserialize)]
struct RuleFile {
    rule: Vec<Rule>,
}

const FILES: [(&str, &str); 6] = [
    ("caches", include_str!("../rules/caches.toml")),
    ("developer", include_str!("../rules/developer.toml")),
    ("browsers", include_str!("../rules/browsers.toml")),
    ("ai", include_str!("../rules/ai.toml")),
    ("privacy", include_str!("../rules/privacy.toml")),
    ("logs", include_str!("../rules/logs.toml")),
];

pub fn validate(rule: &Rule) -> Result<(), String> {
    if rule.paths.is_empty() {
        return Err(format!("{}: no paths", rule.id));
    }
    for p in rule.paths.iter().chain(rule.exclude.iter()) {
        safety::check_pattern(p, Policy::Rule).map_err(|e| format!("rule {}: {}", rule.id, e))?;
    }
    glob::Pattern::new("").ok();
    for p in &rule.paths {
        glob::Pattern::new(&safety::expand(p)).map_err(|e| format!("rule {}: {}", rule.id, e))?;
    }
    Ok(())
}

pub fn all() -> Vec<Rule> {
    let mut out = Vec::new();
    let mut ids = HashSet::new();
    for (name, text) in FILES {
        let file: RuleFile = match toml::from_str(text) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("pumice: rules/{name}.toml is broken: {e}");
                continue;
            }
        };
        for rule in file.rule {
            if !ids.insert(rule.id.clone()) {
                eprintln!("pumice: duplicate rule id {}", rule.id);
                continue;
            }
            if let Err(e) = validate(&rule) {
                eprintln!("pumice: rule refused: {e}");
                continue;
            }
            out.push(rule);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_rule_is_valid() {
        let mut ids = HashSet::new();
        for (name, text) in FILES {
            let file: RuleFile = toml::from_str(text).unwrap_or_else(|e| panic!("{name}.toml: {e}"));
            assert!(!file.rule.is_empty());
            for rule in file.rule {
                validate(&rule).unwrap();
                assert!(ids.insert(rule.id.clone()), "duplicate id {}", rule.id);
                assert!(!rule.description.is_empty());
            }
        }
        assert!(ids.len() > 30, "only {} rules", ids.len());
    }
}
