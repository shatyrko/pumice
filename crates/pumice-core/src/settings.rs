use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// Extra folders to look in for project artifacts.
    pub purge_roots: Vec<String>,
    /// Move removed files to the Trash where the rules allow it (large files and
    /// duplicates always go to the Trash unless this is off).
    pub use_trash: bool,
    /// Smallest file (bytes) that gets its own row in the analysis.
    pub min_file_bytes: u64,
    /// Paths that no rule may touch.
    pub protected: Vec<String>,
    /// Only look at artifacts whose project has been idle this many days.
    pub purge_idle_days: u64,
    /// Dashboard warning limits (percent / degrees Celsius) and the temperature unit (c or f).
    pub cpu_warn: f32,
    pub cpu_crit: f32,
    pub temp_warn: f32,
    pub temp_crit: f32,
    pub ram_warn: f32,
    pub ram_crit: f32,
    pub temp_unit: String,
    /// AI explanations: "off", "anthropic" or "openai" (any OpenAI-compatible server).
    pub ai_provider: String,
    pub ai_base_url: String,
    pub ai_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { purge_roots: Vec::new(), use_trash: true, min_file_bytes: 1 << 20, protected: Vec::new(), purge_idle_days: 30, cpu_warn: 70.0, cpu_crit: 90.0, temp_warn: 80.0, temp_crit: 95.0, ram_warn: 75.0, ram_crit: 90.0, temp_unit: "c".into(), ai_provider: "off".into(), ai_base_url: String::new(), ai_model: String::new() }
    }
}

fn file() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::safety::home().join(".config"));
    base.join("pumice/settings.json")
}

pub fn load() -> Settings {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(s: &Settings) -> crate::Result<()> {
    let f = file();
    if let Some(dir) = f.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(f, serde_json::to_string_pretty(s)?)?;
    Ok(())
}
