use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub avs: String,
}

impl Config {
    pub fn new(app: &AppHandle) -> anyhow::Result<Self> {
        let app_data_dir = app.path().app_data_dir()?;
        let config_path = app_data_dir.join("config.json");
        let default_config = Config { avs: String::new() };
        // 如果配置文件存在且能够解析，则使用配置文件中的配置，否则使用默认配置
        let config = if config_path.exists() {
            let config_string = std::fs::read_to_string(config_path)?;
            serde_json::from_str(&config_string).unwrap_or(default_config)
        } else {
            default_config
        };
        config.save(app)?;
        Ok(config)
    }

    pub fn save(&self, app: &AppHandle) -> anyhow::Result<()> {
        let resource_dir = app.path().app_data_dir()?;
        let config_path = resource_dir.join("config.json");
        let config_string = serde_json::to_string_pretty(self)?;
        std::fs::write(config_path, config_string)?;
        Ok(())
    }

    pub fn get_cookie(&self) -> String {
        format!("AVS={}", self.avs)
    }
}