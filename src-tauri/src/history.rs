use shared::ChatConversation;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use crate::settings;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct ChatTreeNode {
    pub name: String,
    pub path: String, // Relative path from chats/ root (e.g. "My Folder" or "My Folder/chat_uuid.json")
    pub is_dir: bool,
    pub chat_id: Option<String>,
    pub updated_at: Option<u64>,
    pub children: Option<Vec<ChatTreeNode>>,
}

fn get_history_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let settings = settings::load_settings(app);
    let mut dir = if let Some(ref path_str) = settings.custom_storage_path {
        PathBuf::from(path_str)
    } else {
        app.path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data directory: {}", e))?
    };
    
    dir.push("chats");
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create chats directory: {}", e))?;
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
    
    // Determine target directory using folder_id (relative path of folder)
    let save_dir = if let Some(ref folder_path) = conversation.folder_id {
        let p = dir.join(folder_path);
        if !p.exists() {
            fs::create_dir_all(&p)
                .map_err(|e| format!("Failed to create target folder path: {}", e))?;
        }
        p
    } else {
        dir
    };

    let file_path = save_dir.join(format!("{}.json", conversation.id));
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

fn load_conversations_rec(
    root_dir: &Path,
    current_dir: &Path,
    conversations: &mut Vec<ChatConversation>,
) {
    if let Ok(entries) = fs::read_dir(current_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_dir() {
                    load_conversations_rec(root_dir, &path, conversations);
                } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(convo) = serde_json::from_str::<ChatConversation>(&content) {
                            conversations.push(convo);
                        }
                    }
                }
            }
        }
    }
}

pub fn load_conversations(app: &AppHandle) -> Result<Vec<ChatConversation>, String> {
    let dir = get_history_dir(app)?;
    let mut conversations = Vec::new();
    load_conversations_rec(&dir, &dir, &mut conversations);
    
    // Sort descending by updated_at
    conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(conversations)
}

fn find_conversation_file(dir: &Path, id: &str) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(found) = find_conversation_file(&path, id) {
                        return Some(found);
                    }
                } else if path.is_file() {
                    if path.file_name().and_then(|s| s.to_str()) == Some(&format!("{}.json", id)) {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

pub fn delete_conversation(app: &AppHandle, id: &str) -> Result<(), String> {
    let dir = get_history_dir(app)?;
    if let Some(file_path) = find_conversation_file(&dir, id) {
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

// ─── NESTED ORGANIZATION BACKEND OPERATIONS ───

fn scan_dir_rec(root_dir: &Path, current_dir: &Path) -> Result<Vec<ChatTreeNode>, String> {
    let mut nodes = Vec::new();
    if !current_dir.exists() || !current_dir.is_dir() {
        return Ok(nodes);
    }
    
    let entries = fs::read_dir(current_dir)
        .map_err(|e| format!("Failed to read directory {:?}: {}", current_dir, e))?;
        
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            let relative_path = path.strip_prefix(root_dir)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .to_string();
                
            let name = entry.file_name().to_string_lossy().to_string();
            
            if path.is_dir() {
                let children = scan_dir_rec(root_dir, &path)?;
                nodes.push(ChatTreeNode {
                    name,
                    path: relative_path,
                    is_dir: true,
                    chat_id: None,
                    updated_at: None,
                    children: Some(children),
                });
            } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(convo) = serde_json::from_str::<ChatConversation>(&content) {
                        nodes.push(ChatTreeNode {
                            name: convo.title.clone(),
                            path: relative_path,
                            is_dir: false,
                            chat_id: Some(convo.id.clone()),
                            updated_at: Some(convo.updated_at),
                            children: None,
                        });
                    }
                }
            }
        }
    }
    Ok(nodes)
}

pub fn get_chat_tree(app: &AppHandle) -> Result<Vec<ChatTreeNode>, String> {
    let dir = get_history_dir(app)?;
    scan_dir_rec(&dir, &dir)
}

pub fn create_folder(app: &AppHandle, relative_path: &str) -> Result<(), String> {
    let root_dir = get_history_dir(app)?;
    let target_dir = root_dir.join(relative_path);
    if !target_dir.exists() {
        fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create folder: {}", e))?;
    }
    Ok(())
}

fn update_folder_ids_in_dir(root_dir: &Path, current_dir: &Path) {
    if let Ok(entries) = fs::read_dir(current_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_dir() {
                    update_folder_ids_in_dir(root_dir, &path);
                } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(mut convo) = serde_json::from_str::<ChatConversation>(&content) {
                            let parent_rel = path.parent()
                                .unwrap()
                                .strip_prefix(root_dir)
                                .ok()
                                .map(|p| p.to_string_lossy().to_string())
                                .filter(|s| !s.is_empty());
                            if convo.folder_id != parent_rel {
                                convo.folder_id = parent_rel;
                                if let Ok(json) = serde_json::to_string_pretty(&convo) {
                                    let _ = fs::write(&path, json);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn move_item(app: &AppHandle, source_rel: &str, dest_rel: &str) -> Result<(), String> {
    let root_dir = get_history_dir(app)?;
    let source_path = root_dir.join(source_rel);
    let dest_path = root_dir.join(dest_rel);
    
    if !source_path.exists() {
        return Err(format!("Source path does not exist: {}", source_rel));
    }
    
    if let Some(parent) = dest_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create destination parent folder: {}", e))?;
        }
    }
    
    fs::rename(&source_path, &dest_path)
        .map_err(|e| format!("Failed to rename/move item: {}", e))?;
        
    if dest_path.is_file() && dest_path.extension().and_then(|s| s.to_str()) == Some("json") {
        if let Ok(content) = fs::read_to_string(&dest_path) {
            if let Ok(mut convo) = serde_json::from_str::<ChatConversation>(&content) {
                let parent_rel = dest_path.parent()
                    .unwrap()
                    .strip_prefix(&root_dir)
                    .ok()
                    .map(|p| p.to_string_lossy().to_string())
                    .filter(|s| !s.is_empty());
                convo.folder_id = parent_rel;
                if let Ok(json) = serde_json::to_string_pretty(&convo) {
                    let _ = fs::write(&dest_path, json);
                }
            }
        }
    } else if dest_path.is_dir() {
        update_folder_ids_in_dir(&root_dir, &dest_path);
    }
    
    Ok(())
}

fn find_chat_ids_in_dir(dir: &Path, ids: &mut Vec<String>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_dir() {
                    find_chat_ids_in_dir(&path, ids);
                } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Some(name_str) = path.file_stem().and_then(|s| s.to_str()) {
                        ids.push(name_str.to_string());
                    }
                }
            }
        }
    }
}

pub fn delete_folder_recursive(app: &AppHandle, relative_path: &str) -> Result<(), String> {
    let root_dir = get_history_dir(app)?;
    let target_dir = root_dir.join(relative_path);
    if !target_dir.exists() {
        return Ok(());
    }
    
    let mut chat_ids = Vec::new();
    find_chat_ids_in_dir(&target_dir, &mut chat_ids);
    
    fs::remove_dir_all(&target_dir)
        .map_err(|e| format!("Failed to delete folder directory: {}", e))?;
        
    for id in chat_ids {
        if let Ok(assets_dir) = get_assets_dir(app, &id) {
            if assets_dir.exists() {
                let _ = fs::remove_dir_all(assets_dir);
            }
        }
    }
    
    Ok(())
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

    #[test]
    fn test_recursive_tree_ops() {
        use shared::{ChatConversation, Provider};
        let temp_root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&temp_root).unwrap();

        let folder1 = temp_root.join("Subfolder A");
        let folder2 = folder1.join("Nested B");
        fs::create_dir_all(&folder2).unwrap();

        let convo_root = ChatConversation {
            id: "root-chat".to_string(),
            title: "Root Chat".to_string(),
            model: "model".to_string(),
            provider: Provider::OpenAI,
            created_at: chrono::Utc::now(),
            messages: vec![],
            updated_at: 100,
            connection_id: None,
            folder_id: None,
            system_prompt: None,
        };
        fs::write(
            temp_root.join("root-chat.json"),
            serde_json::to_string_pretty(&convo_root).unwrap(),
        )
        .unwrap();

        let convo_nested = ChatConversation {
            id: "nested-chat".to_string(),
            title: "Nested Chat".to_string(),
            model: "model".to_string(),
            provider: Provider::OpenAI,
            created_at: chrono::Utc::now(),
            messages: vec![],
            updated_at: 200,
            connection_id: None,
            folder_id: Some("Subfolder A/Nested B".to_string()),
            system_prompt: None,
        };
        fs::write(
            folder2.join("nested-chat.json"),
            serde_json::to_string_pretty(&convo_nested).unwrap(),
        )
        .unwrap();

        let tree = scan_dir_rec(&temp_root, &temp_root).unwrap();
        assert_eq!(tree.len(), 2);

        let root_chat_node = tree.iter().find(|n| !n.is_dir).unwrap();
        assert_eq!(root_chat_node.name, "Root Chat");
        assert_eq!(root_chat_node.path, "root-chat.json");

        let subfolder_a_node = tree.iter().find(|n| n.is_dir).unwrap();
        assert_eq!(subfolder_a_node.name, "Subfolder A");
        assert_eq!(subfolder_a_node.path, "Subfolder A");

        let children = subfolder_a_node.children.as_ref().unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "Nested B");

        // Move Nested B to root
        let source = "Subfolder A/Nested B";
        let dest = "Nested B";
        
        let source_path = temp_root.join(source);
        let dest_path = temp_root.join(dest);
        fs::rename(source_path, &dest_path).unwrap();
        update_folder_ids_in_dir(&temp_root, &dest_path);

        let chat_path = dest_path.join("nested-chat.json");
        let content = fs::read_to_string(chat_path).unwrap();
        let convo: ChatConversation = serde_json::from_str(&content).unwrap();
        assert_eq!(convo.folder_id, Some("Nested B".to_string()));

        fs::remove_dir_all(temp_root).unwrap();
    }
}
