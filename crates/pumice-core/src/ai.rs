//! Optional explanations of what an item is, from an AI service the person
//! connects themselves (Anthropic, or any OpenAI-compatible server such as a
//! local Ollama). Off by default. Only the item's name, its description and a
//! few paths are sent, never file contents. The key lives in the system keyring.

use crate::settings::Settings;
use serde_json::{json, Value};
use std::time::Duration;

const SERVICE: &str = "pumice";
const ACCOUNT: &str = "ai-key";
const SYSTEM: &str = "You explain items found by a Linux disk cleaner to someone who is not an expert. \
Say what the item is, whether it is safe to remove, and what will happen afterwards (for example that it is rebuilt or downloaded again). \
Use plain language, at most 90 words, no markdown, no lists. If you are not sure, say so instead of guessing.";

pub const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-haiku-5-5";

fn entry() -> crate::Result<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, ACCOUNT)?)
}

pub fn set_key(key: &str) -> crate::Result<()> {
    let key = key.trim();
    if key.is_empty() || key.len() > 400 || key.chars().any(|c| c.is_control() || c.is_whitespace()) {
        anyhow::bail!("That does not look like an API key.");
    }
    entry()?.set_password(key)?;
    Ok(())
}

pub fn clear_key() -> crate::Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn key() -> Option<String> {
    entry().ok().and_then(|e| e.get_password().ok()).or_else(|| std::env::var("PUMICE_AI_KEY").ok()).filter(|k| !k.is_empty())
}

pub fn has_key() -> bool {
    key().is_some()
}

/// The base URL has to be https, or http to this computer (a local model).
pub fn check_url(url: &str) -> Result<(), String> {
    let local = ["http://localhost", "http://127.0.0.1", "http://[::1]"].iter().any(|p| url.starts_with(p) && matches!(url[p.len()..].chars().next(), None | Some(':') | Some('/')));
    if !(url.starts_with("https://") || local) {
        return Err("The server address must start with https:// (or http://localhost for a local model).".into());
    }
    if url.contains(char::is_whitespace) || url.contains('@') {
        return Err("The server address is not valid.".into());
    }
    Ok(())
}

pub struct Config {
    pub provider: String,
    pub base_url: String,
    pub model: String,
}

pub fn config(s: &Settings) -> Option<Config> {
    match s.ai_provider.as_str() {
        "anthropic" => Some(Config {
            provider: "anthropic".into(),
            base_url: if s.ai_base_url.is_empty() { DEFAULT_ANTHROPIC_URL.into() } else { s.ai_base_url.trim_end_matches('/').into() },
            model: if s.ai_model.is_empty() { DEFAULT_ANTHROPIC_MODEL.into() } else { s.ai_model.clone() },
        }),
        "openai" if !s.ai_base_url.is_empty() && !s.ai_model.is_empty() => Some(Config { provider: "openai".into(), base_url: s.ai_base_url.trim_end_matches('/').into(), model: s.ai_model.clone() }),
        _ => None,
    }
}

pub fn request_body(provider: &str, model: &str, facts: &str) -> Value {
    if provider == "anthropic" {
        json!({ "model": model, "max_tokens": 300, "system": SYSTEM, "messages": [{ "role": "user", "content": facts }] })
    } else {
        json!({ "model": model, "max_tokens": 300, "messages": [{ "role": "system", "content": SYSTEM }, { "role": "user", "content": facts }] })
    }
}

pub fn parse_response(provider: &str, v: &Value) -> Option<String> {
    let text = if provider == "anthropic" {
        v["content"].as_array()?.iter().find_map(|b| if b["type"] == "text" { b["text"].as_str() } else { None })?
    } else {
        v["choices"][0]["message"]["content"].as_str()?
    };
    Some(text.trim().to_string()).filter(|t| !t.is_empty())
}

/// Asks the connected service about one item. `facts` is what the person sees
/// on screen about it; it is cut to 1500 characters.
pub fn explain(s: &Settings, topic: &str, facts: &str) -> crate::Result<String> {
    let cfg = config(s).ok_or_else(|| anyhow::anyhow!("AI explanations are not set up. Turn them on in Settings."))?;
    check_url(&cfg.base_url).map_err(anyhow::Error::msg)?;
    let key = key();
    if cfg.provider == "anthropic" && key.is_none() {
        anyhow::bail!("No API key saved. Add one in Settings.");
    }
    let facts: String = facts.chars().take(1500).collect();
    let content = format!("Item type: {topic}\n{facts}");
    let body = request_body(&cfg.provider, &cfg.model, &content);
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(40)).build();
    let mut req;
    if cfg.provider == "anthropic" {
        req = agent.post(&format!("{}/v1/messages", cfg.base_url)).set("anthropic-version", "2023-06-01");
        if let Some(k) = &key {
            req = req.set("x-api-key", k);
        }
    } else {
        req = agent.post(&format!("{}/chat/completions", cfg.base_url));
        if let Some(k) = &key {
            req = req.set("Authorization", &format!("Bearer {k}"));
        }
    }
    let resp = match req.send_json(body) {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let detail = r.into_string().unwrap_or_default();
            let short: String = detail.chars().take(200).collect();
            anyhow::bail!("The service answered {code}. {short}");
        }
        Err(e) => anyhow::bail!("Could not reach the service: {e}"),
    };
    let v: Value = resp.into_json()?;
    parse_response(&cfg.provider, &v).ok_or_else(|| anyhow::anyhow!("The service sent an answer Pumice could not read."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert!(check_url("https://api.anthropic.com").is_ok());
        assert!(check_url("http://localhost:11434/v1").is_ok());
        assert!(check_url("http://127.0.0.1:8080").is_ok());
        assert!(check_url("http://example.com").is_err());
        assert!(check_url("http://localhost.evil.com").is_err());
        assert!(check_url("https://user@host").is_err());
    }

    #[test]
    fn bodies_and_answers() {
        let b = request_body("anthropic", "m", "facts");
        assert_eq!(b["messages"][0]["content"], "facts");
        assert!(b["system"].as_str().unwrap().contains("disk cleaner"));
        let o = request_body("openai", "m", "facts");
        assert_eq!(o["messages"][0]["role"], "system");
        let a = json!({ "content": [{ "type": "text", "text": " Safe to remove. " }] });
        assert_eq!(parse_response("anthropic", &a).unwrap(), "Safe to remove.");
        let c = json!({ "choices": [{ "message": { "content": "Fine." } }] });
        assert_eq!(parse_response("openai", &c).unwrap(), "Fine.");
        assert!(parse_response("openai", &json!({})).is_none());
    }

    #[test]
    fn config_needs_a_model_for_openai() {
        let mut s = Settings::default();
        assert!(config(&s).is_none());
        s.ai_provider = "openai".into();
        assert!(config(&s).is_none());
        s.ai_base_url = "http://localhost:11434/v1".into();
        s.ai_model = "llama".into();
        assert!(config(&s).is_some());
        s.ai_provider = "anthropic".into();
        s.ai_base_url.clear();
        s.ai_model.clear();
        assert_eq!(config(&s).unwrap().model, DEFAULT_ANTHROPIC_MODEL);
    }
}
