mod api_client;
mod credentials;
mod history;

use shared::{ApiConfig, ChatConversation, ChatMessage, Provider};

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
async fn send_message_stream(
    window: tauri::WebviewWindow,
    conversation_id: String,
    config: ApiConfig,
    messages: Vec<ChatMessage>,
) -> Result<(), String> {
    let api_key = match credentials::get_key(config.provider)? {
        Some(key) => key,
        None => {
            return Err(format!(
                "API key not found for provider {:?}",
                config.provider
            ))
        }
    };

    // Spawn task to handle stream asynchronously and return immediately
    tokio::spawn(async move {
        let _ = api_client::stream_chat_completion(
            window,
            conversation_id,
            api_key,
            config,
            messages,
        )
        .await;
    });

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_api_key,
            set_api_key,
            delete_api_key,
            send_message_stream,
            save_conversation,
            load_conversations,
            delete_conversation
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
