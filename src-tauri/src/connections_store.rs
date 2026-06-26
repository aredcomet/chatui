use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use shared::Connection;
use crate::credentials;

fn get_connections_file(app: &AppHandle) -> Result<PathBuf, String> {
    let mut dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create app data directory: {}", e))?;
    }
    dir.push("connections.json");
    Ok(dir)
}

pub fn save_connections(app: &AppHandle, mut connections: Vec<Connection>) -> Result<(), String> {
    // Securely extract and store API keys in Keyring before writing profiles to disk
    for conn in &mut connections {
        if !conn.api_key.is_empty() && conn.api_key != "••••••••" {
            // Write key to secure keychain
            credentials::set_connection_key(&conn.id, &conn.api_key)?;
            // Replace key in struct with placeholder
            conn.api_key = "••••••••".to_string();
        }
    }

    let file = get_connections_file(app)?;
    let json = serde_json::to_string_pretty(&connections)
        .map_err(|e| format!("Failed to serialize connections: {}", e))?;
    fs::write(file, json)
        .map_err(|e| format!("Failed to write connections file: {}", e))?;
    Ok(())
}

pub fn load_connections(app: &AppHandle) -> Result<Vec<Connection>, String> {
    let file = get_connections_file(app)?;
    if !file.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(file)
        .map_err(|e| format!("Failed to read connections file: {}", e))?;
    let mut connections: Vec<Connection> = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse connections file: {}", e))?;

    // Verify key presence in Keyring and populate placeholder
    for conn in &mut connections {
        if let Ok(Some(key)) = credentials::get_connection_key(&conn.id) {
            if !key.is_empty() {
                conn.api_key = "••••••••".to_string();
            } else {
                conn.api_key = "".to_string();
            }
        } else {
            conn.api_key = "".to_string();
        }
    }

    Ok(connections)
}

pub fn delete_connection(app: &AppHandle, id: &str) -> Result<(), String> {
    let mut connections = load_connections(app)?;
    connections.retain(|c| c.id != id);
    save_connections(app, connections)?;
    // Delete corresponding key from keychain
    let _ = credentials::delete_connection_key(id);
    Ok(())
}
