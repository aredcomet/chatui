use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub custom_storage_path: Option<String>,
}

fn get_settings_file(app: &AppHandle) -> Result<PathBuf, String> {
    let mut dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create app data directory: {}", e))?;
    }
    dir.push("settings.json");
    Ok(dir)
}

pub fn load_settings(app: &AppHandle) -> AppSettings {
    if let Ok(file) = get_settings_file(app) {
        if file.exists() {
            if let Ok(content) = fs::read_to_string(file) {
                if let Ok(settings) = serde_json::from_str::<AppSettings>(&content) {
                    return settings;
                }
            }
        }
    }
    AppSettings::default()
}

pub fn save_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let file = get_settings_file(app)?;
    let temp_file = file.with_extension("tmp");
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        
    // Atomic write
    fs::write(&temp_file, json)
        .map_err(|e| format!("Failed to write temp settings file: {}", e))?;
    fs::rename(temp_file, file)
        .map_err(|e| format!("Failed to rename settings file: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_write_simulation() {
        let temp_dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&temp_dir).unwrap();
        
        let target_file = temp_dir.join("settings.json");
        let temp_file = target_file.with_extension("tmp");
        
        let settings = AppSettings {
            custom_storage_path: Some("/mock/custom/path".to_string()),
        };
        
        // Write to temp, then rename
        let json = serde_json::to_string_pretty(&settings).unwrap();
        fs::write(&temp_file, &json).unwrap();
        
        assert!(temp_file.exists());
        assert!(!target_file.exists());
        
        fs::rename(&temp_file, &target_file).unwrap();
        
        assert!(!temp_file.exists());
        assert!(target_file.exists());
        
        let read_back_json = fs::read_to_string(target_file).unwrap();
        let read_back: AppSettings = serde_json::from_str(&read_back_json).unwrap();
        assert_eq!(read_back.custom_storage_path, Some("/mock/custom/path".to_string()));
        
        fs::remove_dir_all(temp_dir).unwrap();
    }
}

