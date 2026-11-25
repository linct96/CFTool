use crate::StoredInput;
use serde_json::{Map, Value};
use std::path::Path;
use tauri::{AppHandle, Manager};

type Config = Map<String, Value>;
const CONFIG: StoredInput = StoredInput {
    filename: "config.json",
    label: "config.json",
};

fn read_config(dir: &Path) -> Result<Config, String> {
    let Some(text) = CONFIG.read(dir).map_err(|err| err.to_string())? else {
        return Ok(Config::new());
    };
    serde_json::from_str(&text).map_err(|err| format!("config.json 格式无效：{err}"))
}

fn cfst_section(config: &Config) -> Result<Option<&Config>, String> {
    config
        .get("cfst")
        .map(|value| {
            value
                .as_object()
                .ok_or_else(|| "config.json 的 cfst 配置格式无效".into())
        })
        .transpose()
}

fn write_cfst_settings(dir: &Path, settings: Config) -> Result<(), String> {
    let mut config = read_config(dir)?;
    cfst_section(&config)?;
    config.insert("cfst".into(), Value::Object(settings));
    let text = serde_json::to_string_pretty(&config).map_err(|err| err.to_string())? + "\n";
    CONFIG.write(dir, &text).map_err(|err| err.to_string())
}

async fn config_task<T: Send + 'static>(
    app: AppHandle,
    task: impl FnOnce(&Path) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
    tauri::async_runtime::spawn_blocking(move || task(&dir))
        .await
        .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn load_cfst_settings(app: AppHandle) -> Result<Option<Config>, String> {
    config_task(app, |dir| Ok(cfst_section(&read_config(dir)?)?.cloned())).await
}

#[tauri::command]
pub async fn save_cfst_settings(app: AppHandle, settings: String) -> Result<(), String> {
    CONFIG
        .check_size(&settings)
        .map_err(|err| err.to_string())?;
    config_task(app, move |dir| {
        let settings = serde_json::from_str(&settings).map_err(|_| "CFST 参数格式无效")?;
        write_cfst_settings(dir, settings)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::PrivateDir;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn persists_cfst_settings_without_overwriting_other_sections_or_corrupt_config() {
        let temporary =
            PrivateDir::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string())).unwrap();
        let dir = temporary.path();
        let path = dir.join(CONFIG.filename);
        assert!(cfst_section(&read_config(dir).unwrap()).unwrap().is_none());

        fs::write(&path, r#"{"future":{"key":true}}"#).unwrap();
        let first = serde_json::from_str(r#"{"mode":"http","n":42,"tp":null}"#).unwrap();
        write_cfst_settings(dir, first).unwrap();
        let second: Config = serde_json::from_str(r#"{"mode":"tcp","n":100,"tp":null}"#).unwrap();
        write_cfst_settings(dir, second.clone()).unwrap();
        let config = read_config(dir).unwrap();
        assert_eq!(cfst_section(&config).unwrap(), Some(&second));
        assert_eq!(config["future"]["key"], true);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        for corrupt in ["{", r#"{"cfst":null}"#, "[]"] {
            fs::write(&path, corrupt).unwrap();
            assert!(write_cfst_settings(dir, second.clone()).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), corrupt);
        }
    }
}
