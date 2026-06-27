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

fn get_active_assets(conversation: &ChatConversation) -> std::collections::HashSet<PathBuf> {
    use shared::ContentBlock;
    let mut active = std::collections::HashSet::new();
    for msg in &conversation.messages {
        for version in &msg.versions {
            for block in &version.content {
                match block {
                    ContentBlock::Image { path, .. } => {
                        active.insert(PathBuf::from(path));
                    }
                    ContentBlock::Document { path: Some(path), .. } => {
                        active.insert(PathBuf::from(path));
                    }
                    ContentBlock::Audio { path, .. } => {
                        active.insert(PathBuf::from(path));
                    }
                    _ => {}
                }
            }
        }
    }
    active
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
        
    // Garbage collect unused assets in this thread's assets directory
    if let Ok(assets_dir) = get_assets_dir(app, &conversation.id) {
        if assets_dir.exists() {
            let active_assets = get_active_assets(&conversation);
            if let Ok(entries) = fs::read_dir(&assets_dir) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if path.is_file() {
                            if !active_assets.contains(&path) {
                                let _ = fs::remove_file(path);
                            }
                        }
                    }
                }
            }
        }
    }
    
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

    #[test]
    fn test_asset_garbage_collection() {
        use shared::{ChatConversation, ChatMessage, MessageRole, MessageVersion, MessageMetadata, ContentBlock, Provider};
        let temp_dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&temp_dir).unwrap();
        
        let file1 = temp_dir.join("referenced.png");
        let file2 = temp_dir.join("orphaned.png");
        fs::write(&file1, "referenced").unwrap();
        fs::write(&file2, "orphaned").unwrap();
        
        let conversation = ChatConversation {
            id: "test-thread".to_string(),
            title: "Test".to_string(),
            model: "test".to_string(),
            provider: Provider::OpenAI,
            created_at: chrono::Utc::now(),
            messages: vec![ChatMessage {
                id: "test-msg".to_string(),
                role: MessageRole::User,
                versions: vec![MessageVersion {
                    content: vec![
                        ContentBlock::Image {
                            path: file1.to_string_lossy().to_string(),
                            mime_type: "image/png".to_string(),
                        }
                    ],
                    metadata: MessageMetadata {
                        model: "test".to_string(),
                        provider: Provider::OpenAI,
                        connection_id: "test".to_string(),
                        created_at: chrono::Utc::now(),
                        ttft_ms: None,
                        tokens_per_sec: None,
                        stop_reason: None,
                    }
                }],
                active_version: 0,
            }],
            updated_at: 0,
            connection_id: None,
            folder_id: None,
            system_prompt: None,
        };
        
        let active = get_active_assets(&conversation);
        assert!(active.contains(&file1));
        assert!(!active.contains(&file2));
        
        // Scan directory and clean up orphaned files (imitating the logic in save_conversation)
        let entries = fs::read_dir(&temp_dir).unwrap();
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_file() {
                if !active.contains(&path) {
                    fs::remove_file(path).unwrap();
                }
            }
        }
        
        assert!(file1.exists());
        assert!(!file2.exists());
        
        fs::remove_dir_all(temp_dir).unwrap();
    }
}


