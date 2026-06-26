use shared::ChatConversation;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn get_history_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let mut dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
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
    let json = serde_json::to_string_pretty(&conversation)
        .map_err(|e| format!("Failed to serialize conversation: {}", e))?;
    fs::write(file_path, json)
        .map_err(|e| format!("Failed to write conversation file: {}", e))?;
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
    Ok(())
}
