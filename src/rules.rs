use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RulesError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}

pub type RulesResult<T> = Result<T, RulesError>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RulesConfig {
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub vip_senders: Vec<VipSender>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    #[serde(rename = "match")]
    pub match_criteria: MatchCriteria,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MatchCriteria {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_exact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_contains: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<MatchCriteria>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    #[serde(rename = "type")]
    pub action_type: ActionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_number: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ActionType {
    Label,
    Trash,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VipSender {
    pub address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Return the default rules config path: ~/.mea/rules.toml
pub fn default_rules_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mea").join("rules.toml")
}

/// Load rules config from a file. Creates default if file doesn't exist.
pub fn load_rules(path: &Path) -> RulesResult<RulesConfig> {
    if !path.exists() {
        return Ok(RulesConfig::default());
    }
    let content = std::fs::read_to_string(path)?;
    let config: RulesConfig = toml::from_str(&content)?;
    Ok(config)
}

/// Save rules config to a file.
pub fn save_rules(path: &Path, config: &RulesConfig) -> RulesResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Check if a sender address matches any VIP sender.
pub fn is_vip(config: &RulesConfig, sender_address: &str) -> bool {
    config
        .vip_senders
        .iter()
        .any(|vip| sender_address.eq_ignore_ascii_case(&vip.address))
}

/// Evaluate a single email against the rules config.
/// Returns the first matching rule's action, or None if no rule matches.
/// VIP senders always match with a "label 1" (Follow Up) action first.
pub fn evaluate_rules(
    config: &RulesConfig,
    sender_address: &str,
    subject: &str,
) -> Option<(String, Action)> {
    // VIP check first — highest priority
    if is_vip(config, sender_address) {
        return Some((
            "VIP Sender".to_string(),
            Action {
                action_type: ActionType::Label,
                label_number: Some(1),
            },
        ));
    }

    // Evaluate rules in order — first match wins
    for rule in &config.rules {
        if matches_criteria(&rule.match_criteria, sender_address, subject) {
            return Some((rule.name.clone(), rule.action.clone()));
        }
    }

    None
}

/// Check if an email matches the given criteria.
fn matches_criteria(criteria: &MatchCriteria, sender_address: &str, subject: &str) -> bool {
    // If any_of is specified, check if ANY sub-criteria matches
    if let Some(ref any_of) = criteria.any_of {
        return any_of
            .iter()
            .any(|c| matches_criteria(c, sender_address, subject));
    }

    let mut has_condition = false;
    let mut all_match = true;

    if let Some(ref pattern) = criteria.sender_contains {
        has_condition = true;
        if !sender_address
            .to_lowercase()
            .contains(&pattern.to_lowercase())
        {
            all_match = false;
        }
    }

    if let Some(ref exact) = criteria.sender_exact {
        has_condition = true;
        if !sender_address.eq_ignore_ascii_case(exact) {
            all_match = false;
        }
    }

    if let Some(ref pattern) = criteria.subject_contains {
        has_condition = true;
        if !subject.to_lowercase().contains(&pattern.to_lowercase()) {
            all_match = false;
        }
    }

    has_condition && all_match
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> RulesConfig {
        RulesConfig {
            rules: vec![
                Rule {
                    name: "Receipts".to_string(),
                    match_criteria: MatchCriteria {
                        subject_contains: Some("receipt".to_string()),
                        ..Default::default()
                    },
                    action: Action {
                        action_type: ActionType::Label,
                        label_number: Some(5),
                    },
                },
                Rule {
                    name: "Food orders".to_string(),
                    match_criteria: MatchCriteria {
                        sender_contains: Some("doordash".to_string()),
                        subject_contains: Some("order confirmed".to_string()),
                        ..Default::default()
                    },
                    action: Action {
                        action_type: ActionType::Trash,
                        label_number: None,
                    },
                },
                Rule {
                    name: "Marketing".to_string(),
                    match_criteria: MatchCriteria {
                        any_of: Some(vec![
                            MatchCriteria {
                                sender_contains: Some("marketing@".to_string()),
                                ..Default::default()
                            },
                            MatchCriteria {
                                subject_contains: Some("unsubscribe".to_string()),
                                ..Default::default()
                            },
                        ]),
                        ..Default::default()
                    },
                    action: Action {
                        action_type: ActionType::Archive,
                        label_number: None,
                    },
                },
            ],
            vip_senders: vec![VipSender {
                address: "boss@company.com".to_string(),
                name: Some("Boss".to_string()),
            }],
        }
    }

    #[test]
    fn test_vip_sender_always_follow_up() {
        let config = sample_config();
        let result = evaluate_rules(&config, "boss@company.com", "anything");
        let (name, action) = result.unwrap();
        assert_eq!(name, "VIP Sender");
        assert_eq!(action.action_type, ActionType::Label);
        assert_eq!(action.label_number, Some(1));
    }

    #[test]
    fn test_vip_case_insensitive() {
        let config = sample_config();
        assert!(is_vip(&config, "Boss@Company.com"));
    }

    #[test]
    fn test_receipt_rule() {
        let config = sample_config();
        let result = evaluate_rules(&config, "store@shop.com", "Your receipt from Shop");
        let (name, action) = result.unwrap();
        assert_eq!(name, "Receipts");
        assert_eq!(action.action_type, ActionType::Label);
        assert_eq!(action.label_number, Some(5));
    }

    #[test]
    fn test_food_order_trash() {
        let config = sample_config();
        let result = evaluate_rules(
            &config,
            "noreply@doordash.com",
            "Your order confirmed #12345",
        );
        let (name, action) = result.unwrap();
        assert_eq!(name, "Food orders");
        assert_eq!(action.action_type, ActionType::Trash);
    }

    #[test]
    fn test_any_of_matching() {
        let config = sample_config();
        // Match by sender
        let result = evaluate_rules(&config, "marketing@bigcorp.com", "New Products");
        assert_eq!(result.unwrap().0, "Marketing");

        // Match by subject
        let result = evaluate_rules(&config, "random@other.com", "Click to unsubscribe now");
        assert_eq!(result.unwrap().0, "Marketing");
    }

    #[test]
    fn test_no_match() {
        let config = sample_config();
        let result = evaluate_rules(&config, "friend@gmail.com", "Dinner tonight?");
        assert!(result.is_none());
    }

    #[test]
    fn test_first_match_wins() {
        let config = sample_config();
        // "receipt" in subject matches Receipts rule before any other
        let result = evaluate_rules(&config, "noreply@doordash.com", "Your receipt");
        assert_eq!(result.unwrap().0, "Receipts");
    }

    #[test]
    fn test_vip_takes_priority() {
        let config = sample_config();
        // VIP sender + receipt subject -> VIP wins
        let result = evaluate_rules(&config, "boss@company.com", "Your receipt");
        assert_eq!(result.unwrap().0, "VIP Sender");
    }

    #[test]
    fn test_load_save_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rules.toml");
        let config = sample_config();

        save_rules(&path, &config).unwrap();
        let loaded = load_rules(&path).unwrap();

        assert_eq!(loaded.rules.len(), 3);
        assert_eq!(loaded.vip_senders.len(), 1);
        assert_eq!(loaded.rules[0].name, "Receipts");
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let config = load_rules(Path::new("/nonexistent/rules.toml")).unwrap();
        assert!(config.rules.is_empty());
        assert!(config.vip_senders.is_empty());
    }

    #[test]
    fn test_sender_exact_match() {
        let config = RulesConfig {
            rules: vec![Rule {
                name: "Exact".to_string(),
                match_criteria: MatchCriteria {
                    sender_exact: Some("exact@test.com".to_string()),
                    ..Default::default()
                },
                action: Action {
                    action_type: ActionType::Label,
                    label_number: Some(2),
                },
            }],
            vip_senders: vec![],
        };
        assert!(evaluate_rules(&config, "exact@test.com", "Hi").is_some());
        assert!(evaluate_rules(&config, "not-exact@test.com", "Hi").is_none());
    }
}
