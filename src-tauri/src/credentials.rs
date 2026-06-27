#[cfg(not(debug_assertions))]
use keyring::Entry;
use shared::Provider;

#[cfg(not(debug_assertions))]
const SERVICE_NAME: &str = "chatui-api-keys";
#[cfg(not(debug_assertions))]
const CONNECTION_SERVICE_NAME: &str = "chatui-connection-keys";

#[cfg(debug_assertions)]
use std::collections::HashMap;
#[cfg(debug_assertions)]
use std::fs::File;
#[cfg(debug_assertions)]
use std::io::{Read, Write};

#[cfg(debug_assertions)]
fn get_dev_store_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&home).join(".chatui-keys-dev.json")
}

#[cfg(debug_assertions)]
fn get_dev_keys() -> HashMap<String, String> {
    let path = get_dev_store_path();
    if !path.exists() {
        return HashMap::new();
    }
    let mut file = match File::open(&path) {
        Ok(f) => f,
        Err(_) => return HashMap::new(),
    };
    let mut contents = String::new();
    if file.read_to_string(&mut contents).is_err() {
        return HashMap::new();
    }
    serde_json::from_str(&contents).unwrap_or_default()
}

#[cfg(debug_assertions)]
fn save_dev_keys(keys: &HashMap<String, String>) -> Result<(), String> {
    let path = get_dev_store_path();
    let mut file = File::create(&path).map_err(|e| format!("Failed to create dev keys file: {}", e))?;
    let json = serde_json::to_string(keys).map_err(|e| format!("Failed to serialize dev keys: {}", e))?;
    file.write_all(json.as_bytes()).map_err(|e| format!("Failed to write dev keys: {}", e))?;
    Ok(())
}

fn provider_to_key(provider: Provider) -> String {
    match provider {
        Provider::OpenAI => "openai".to_string(),
        Provider::Claude => "claude".to_string(),
        Provider::Gemini => "gemini".to_string(),
        Provider::OpenRouter => "openrouter".to_string(),
        Provider::CustomOpenAICompliant => "custom_openai".to_string(),
    }
}

pub fn get_key(provider: Provider) -> Result<Option<String>, String> {
    let key = provider_to_key(provider);
    #[cfg(debug_assertions)]
    {
        let keys = get_dev_keys();
        return Ok(keys.get(&format!("global:{}", key)).cloned());
    }
    #[cfg(not(debug_assertions))]
    {
        match Entry::new(SERVICE_NAME, &key) {
            Ok(entry) => match entry.get_password() {
                Ok(password) => Ok(Some(password)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(format!("Failed to retrieve password from keyring: {}", e)),
            },
            Err(e) => Err(format!("Failed to initialize keyring entry: {}", e)),
        }
    }
}

pub fn set_key(provider: Provider, key: &str) -> Result<(), String> {
    let provider_key = provider_to_key(provider);
    #[cfg(debug_assertions)]
    {
        let mut keys = get_dev_keys();
        keys.insert(format!("global:{}", provider_key), key.to_string());
        save_dev_keys(&keys)
    }
    #[cfg(not(debug_assertions))]
    {
        match Entry::new(SERVICE_NAME, &provider_key) {
            Ok(entry) => entry
                .set_password(key)
                .map_err(|e| format!("Failed to save password to keyring: {}", e)),
            Err(e) => Err(format!("Failed to initialize keyring entry: {}", e)),
        }
    }
}

pub fn delete_key(provider: Provider) -> Result<(), String> {
    let provider_key = provider_to_key(provider);
    #[cfg(debug_assertions)]
    {
        let mut keys = get_dev_keys();
        keys.remove(&format!("global:{}", provider_key));
        save_dev_keys(&keys)
    }
    #[cfg(not(debug_assertions))]
    {
        match Entry::new(SERVICE_NAME, &provider_key) {
            Ok(entry) => match entry.delete_password() {
                Ok(_) => Ok(()),
                Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(format!("Failed to delete password from keyring: {}", e)),
            },
            Err(e) => Err(format!("Failed to initialize keyring entry: {}", e)),
        }
    }
}

// Connection-specific secure storage
pub fn get_connection_key(id: &str) -> Result<Option<String>, String> {
    #[cfg(debug_assertions)]
    {
        let keys = get_dev_keys();
        return Ok(keys.get(&format!("conn:{}", id)).cloned());
    }
    #[cfg(not(debug_assertions))]
    {
        match Entry::new(CONNECTION_SERVICE_NAME, id) {
            Ok(entry) => match entry.get_password() {
                Ok(password) => Ok(Some(password)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(format!("Failed to retrieve connection key from keyring: {}", e)),
            },
            Err(e) => Err(format!("Failed to initialize keyring entry for connection: {}", e)),
        }
    }
}

pub fn set_connection_key(id: &str, key: &str) -> Result<(), String> {
    #[cfg(debug_assertions)]
    {
        let mut keys = get_dev_keys();
        keys.insert(format!("conn:{}", id), key.to_string());
        save_dev_keys(&keys)
    }
    #[cfg(not(debug_assertions))]
    {
        match Entry::new(CONNECTION_SERVICE_NAME, id) {
            Ok(entry) => entry
                .set_password(key)
                .map_err(|e| format!("Failed to save connection key to keyring: {}", e)),
            Err(e) => Err(format!("Failed to initialize keyring entry for connection: {}", e)),
        }
    }
}

pub fn delete_connection_key(id: &str) -> Result<(), String> {
    #[cfg(debug_assertions)]
    {
        let mut keys = get_dev_keys();
        keys.remove(&format!("conn:{}", id));
        save_dev_keys(&keys)
    }
    #[cfg(not(debug_assertions))]
    {
        match Entry::new(CONNECTION_SERVICE_NAME, id) {
            Ok(entry) => match entry.delete_password() {
                Ok(_) => Ok(()),
                Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(format!("Failed to delete connection key from keyring: {}", e)),
            },
            Err(e) => Err(format!("Failed to initialize keyring entry for connection: {}", e)),
        }
    }
}
