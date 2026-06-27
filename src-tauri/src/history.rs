use shared::ChatConversation;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use crate::settings;

fn get_history_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let settings = settings::load_settings(app);
    let mut dir = if let Some(ref path_str) = settings.custom_storage_path {
        PathBuf::from(path_str)
    } else {
        app.path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data directory: {}", e))?
    };
    
    dir.push("history");
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create history directory: {}", e))?;
    }
    Ok(dir)
}

pub fn save_conversation(app: &AppHandle, conversation: ChatConversation) -> Result<(), String> {
    let dir = get_history_dir(app)?;
    let file_path = dir.join(format!("{}.json", conversation.id));
    let temp_path = file_path.with_extension("tmp");
    
    let json = serde_json::to_string_pretty(&conversation)
        .map_err(|e| format!("Failed to serialize conversation: {}", e))?;
        
    // Atomic Write: Write to .tmp first, then rename to .json to prevent corruption on crash
    fs::write(&temp_path, json)
        .map_err(|e| format!("Failed to write temp conversation file: {}", e))?;
    fs::rename(temp_path, file_path)
        .map_err(|e| format!("Failed to commit conversation file: {}", e))?;
        
    Ok(())
}

pub fn load_conversations(app: &AppHandle) -> Result<Vec<ChatConversation>, String> {
    let dir = get_history_dir(app)?;
    let mut conversations = Vec::new();

    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read history directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(convo) = serde_json::from_str::<ChatConversation>(&content) {
                    conversations.push(convo);
                }
            }
        }
    }

    // Sort descending by updated_at
    conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(conversations)
}

pub fn delete_conversation(app: &AppHandle, id: &str) -> Result<(), String> {
    let dir = get_history_dir(app)?;
    let file_path = dir.join(format!("{}.json", id));
    if file_path.exists() {
        fs::remove_file(file_path)
            .map_err(|e| format!("Failed to delete conversation file: {}", e))?;
    }
    // Also try to remove assets subdirectory for this thread
    if let Ok(assets_dir) = get_assets_dir(app, id) {
        if assets_dir.exists() {
            let _ = fs::remove_dir_all(assets_dir);
        }
    }
    Ok(())
}

fn get_assets_dir(app: &AppHandle, thread_id: &str) -> Result<PathBuf, String> {
    let settings = settings::load_settings(app);
    let mut dir = if let Some(ref path_str) = settings.custom_storage_path {
        PathBuf::from(path_str)
    } else {
        app.path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data directory: {}", e))?
    };
    dir.push("assets");
    dir.push(thread_id);
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create assets directory: {}", e))?;
    }
    Ok(dir)
}

pub fn save_thread_asset(app: &AppHandle, thread_id: &str, filename: &str, base64_data: &str) -> Result<String, String> {
    use base64::Engine;
    let dir = get_assets_dir(app, thread_id)?;
    let target_path = dir.join(filename);
    
    let bytes = base64::prelude::BASE64_STANDARD.decode(base64_data)
        .map_err(|e| format!("Failed to decode base64 asset: {}", e))?;
        
    fs::write(&target_path, bytes)
        .map_err(|e| format!("Failed to write asset file: {}", e))?;
        
    Ok(target_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_save_decoding() {
        let temp_dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&temp_dir).unwrap();
        
        let filename = "test.txt";
        let base64_data = "SGVsbG8gQXNzZXQ="; // "Hello Asset"
        let target_path = temp_dir.join(filename);
        
        use base64::Engine;
        let bytes = base64::prelude::BASE64_STANDARD.decode(base64_data).unwrap();
        fs::write(&target_path, bytes).unwrap();
        
        assert!(target_path.exists());
        let content = fs::read_to_string(target_path).unwrap();
        assert_eq!(content, "Hello Asset");
        
        fs::remove_dir_all(temp_dir).unwrap();
    }
}


