use std::path::PathBuf;

use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub lilac_version: String,
    pub output_dir: Option<String>,
    pub output_type: String,
}

pub fn load_config(dir: &PathBuf) -> Result<Config, String> {
    let path = dir.join("lilac.json");

    if !path.exists() {
        let config = Config::default();
        write_config(dir,  &config)?;
        return Ok(config);
    }

    let config = std::fs::read_to_string(path).map_err(|e| e.to_string())?;

    Ok(serde_json::from_str(&config).map_err(|e| e.to_string())?)
}

fn write_config(dir: &PathBuf, config: &Config) -> Result<(), String> {
    let path = dir.join("lilac.json");

    let config = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;

    std::fs::write(path, config).map_err(|e| e.to_string())?;

    Ok(())
}

impl Default for Config {
    fn default() -> Self {
        Config {
            lilac_version: env!("CARGO_PKG_VERSION").to_string(),
            output_dir: Some("dist".to_string()),
            output_type: "html".to_string(),
        }
    }
}
