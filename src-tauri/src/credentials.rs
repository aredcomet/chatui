use keyring::Entry;
use shared::Provider;

const SERVICE_NAME: &str = "chatui-api-keys";
const CONNECTION_SERVICE_NAME: &str = "chatui-connection-keys";

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
    match Entry::new(SERVICE_NAME, &key) {
        Ok(entry) => match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Failed to retrieve password from keyring: {}", e)),
        },
        Err(e) => Err(format!("Failed to initialize keyring entry: {}", e)),
    }
}

pub fn set_key(provider: Provider, key: &str) -> Result<(), String> {
    let provider_key = provider_to_key(provider);
    match Entry::new(SERVICE_NAME, &provider_key) {
        Ok(entry) => entry
            .set_password(key)
            .map_err(|e| format!("Failed to save password to keyring: {}", e)),
        Err(e) => Err(format!("Failed to initialize keyring entry: {}", e)),
    }
}

pub fn delete_key(provider: Provider) -> Result<(), String> {
    let provider_key = provider_to_key(provider);
    match Entry::new(SERVICE_NAME, &provider_key) {
        Ok(entry) => match entry.delete_password() {
            Ok(_) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Failed to delete password from keyring: {}", e)),
        },
        Err(e) => Err(format!("Failed to initialize keyring entry: {}", e)),
    }
}

// Connection-specific secure storage
pub fn get_connection_key(id: &str) -> Result<Option<String>, String> {
    match Entry::new(CONNECTION_SERVICE_NAME, id) {
        Ok(entry) => match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Failed to retrieve connection key from keyring: {}", e)),
        },
        Err(e) => Err(format!("Failed to initialize keyring entry for connection: {}", e)),
    }
}

pub fn set_connection_key(id: &str, key: &str) -> Result<(), String> {
    match Entry::new(CONNECTION_SERVICE_NAME, id) {
        Ok(entry) => entry
            .set_password(key)
            .map_err(|e| format!("Failed to save connection key to keyring: {}", e)),
        Err(e) => Err(format!("Failed to initialize keyring entry for connection: {}", e)),
    }
}

pub fn delete_connection_key(id: &str) -> Result<(), String> {
    match Entry::new(CONNECTION_SERVICE_NAME, id) {
        Ok(entry) => match entry.delete_password() {
            Ok(_) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Failed to delete connection key from keyring: {}", e)),
        },
        Err(e) => Err(format!("Failed to initialize keyring entry for connection: {}", e)),
    }
}
