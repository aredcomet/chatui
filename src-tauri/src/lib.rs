mod api_client;
mod connections_store;
mod credentials;
mod history;
mod settings;

use shared::{ApiConfig, ChatConversation, ChatMessage, Connection, Provider};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;

pub struct ActiveStreams(pub Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>);

#[tauri::command]
fn get_api_key(provider: Provider) -> Result<Option<String>, String> {
    credentials::get_key(provider)
}

#[tauri::command]
fn set_api_key(provider: Provider, key: String) -> Result<(), String> {
    credentials::set_key(provider, &key)
}

#[tauri::command]
fn delete_api_key(provider: Provider) -> Result<(), String> {
    credentials::delete_key(provider)
}

#[tauri::command]
async fn fetch_models(
    provider: Provider,
    api_key: String,
    base_url: Option<String>,
) -> Result<Vec<String>, String> {
    println!("Backend fetch_models command invoked: provider={:?}, base_url={:?}", provider, base_url);
    let res = api_client::fetch_provider_models(provider, api_key, base_url).await;
    println!("Backend fetch_models command completed: res={:?}", res);
    res
}

#[tauri::command]
fn save_connections(app: tauri::AppHandle, connections: Vec<Connection>) -> Result<(), String> {
    connections_store::save_connections(&app, connections)
}

#[tauri::command]
fn load_connections(app: tauri::AppHandle) -> Result<Vec<Connection>, String> {
    connections_store::load_connections(&app)
}

#[tauri::command]
fn delete_connection(app: tauri::AppHandle, id: String) -> Result<(), String> {
    connections_store::delete_connection(&app, &id)
}

#[tauri::command]
async fn send_message_stream(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    conversation_id: String,
    config: ApiConfig,
    messages: Vec<ChatMessage>,
) -> Result<(), String> {
    let (api_key, base_url, reasoning_config) = if let Some(ref conn_id) = config.connection_id {
        // Load connection from store
        let connections = connections_store::load_connections(&app)?;
        let conn = connections
            .iter()
            .find(|c| &c.id == conn_id)
            .ok_or_else(|| format!("Connection not found: {}", conn_id))?;

        let key = credentials::get_connection_key(&conn.id)?
            .ok_or_else(|| "API Key not found in secure store".to_string())?;

        let reasoning_config = conn.reasoning_configs
            .iter()
            .find(|rc| rc.model_id == config.model)
            .cloned();

        (key, conn.base_url.clone(), reasoning_config)
    } else {
        // Fallback to legacy global keychain keys
        let key = match credentials::get_key(config.provider)? {
            Some(k) => k,
            None => {
                return Err(format!(
                    "API key not found for provider {:?}",
                    config.provider
                ))
            }
        };
        (key, None, None)
    };

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    
    // Register active stream
    {
        if let Some(active_streams) = app.try_state::<ActiveStreams>() {
            let mut map = active_streams.0.lock().unwrap();
            map.insert(conversation_id.clone(), cancel_tx);
        }
    }

    let app_clone = app.clone();
    let conversation_id_clone = conversation_id.clone();

    // Spawn task to handle stream asynchronously and return immediately
    tokio::spawn(async move {
        let _ = api_client::stream_chat_completion(
            window,
            conversation_id,
            api_key,
            base_url,
            config,
            messages,
            reasoning_config,
            cancel_rx,
        )
        .await;

        // Cleanup active stream when finished or cancelled
        if let Some(active_streams) = app_clone.try_state::<ActiveStreams>() {
            let mut map = active_streams.0.lock().unwrap();
            map.remove(&conversation_id_clone);
        }
    });

    Ok(())
}

#[tauri::command]
fn cancel_chat_stream(app: tauri::AppHandle, conversation_id: String) -> Result<(), String> {
    if let Some(active_streams) = app.try_state::<ActiveStreams>() {
        let mut map = active_streams.0.lock().unwrap();
        if let Some(cancel_tx) = map.remove(&conversation_id) {
            let _ = cancel_tx.send(());
            println!("Backend: stream cancellation triggered for conversation {}", conversation_id);
        }
    }
    Ok(())
}

#[tauri::command]
fn save_conversation(app: tauri::AppHandle, conversation: ChatConversation) -> Result<(), String> {
    history::save_conversation(&app, conversation)
}

#[tauri::command]
fn load_conversations(app: tauri::AppHandle) -> Result<Vec<ChatConversation>, String> {
    history::load_conversations(&app)
}

#[tauri::command]
fn delete_conversation(app: tauri::AppHandle, id: String) -> Result<(), String> {
    history::delete_conversation(&app, &id)
}

#[tauri::command]
fn get_settings(app: tauri::AppHandle) -> settings::AppSettings {
    settings::load_settings(&app)
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, settings: settings::AppSettings) -> Result<(), String> {
    settings::save_settings(&app, &settings)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(ActiveStreams(Arc::new(Mutex::new(HashMap::new()))))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_api_key,
            set_api_key,
            delete_api_key,
            send_message_stream,
            cancel_chat_stream,
            save_conversation,
            load_conversations,
            delete_conversation,
            fetch_models,
            save_connections,
            load_connections,
            delete_connection,
            get_settings,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
