use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Provider {
    OpenAI,
    Claude,
    Gemini,
    OpenRouter,
    CustomOpenAICompliant,
}

impl Provider {
    pub fn to_string(&self) -> String {
        match self {
            Provider::OpenAI => "OpenAI".to_string(),
            Provider::Claude => "Claude (Anthropic)".to_string(),
            Provider::Gemini => "Google Gemini".to_string(),
            Provider::OpenRouter => "OpenRouter".to_string(),
            Provider::CustomOpenAICompliant => "Custom OpenAI-Compliant".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Connection {
    pub id: String,                  // Unique UUID
    pub name: String,                // User-defined name
    pub provider: Provider,
    pub api_key: String,             // API Key (stored securely)
    pub base_url: Option<String>,    // Custom Base URL
    pub enabled_models: Vec<String>, // Checked model names
    pub default_model: String,       // Default model name
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: Provider,
    pub is_vlm: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentPart {
    Text { text: String },
    Image { mime_type: String, base64: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: Vec<ContentPart>,
}

impl ChatMessage {
    pub fn new_text(role: MessageRole, text: String) -> Self {
        Self {
            role,
            content: vec![ContentPart::Text { text }],
        }
    }

    pub fn get_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiConfig {
    pub provider: Provider,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub connection_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatConversation {
    pub id: String,
    pub title: String,
    pub model: String,
    pub provider: Provider,
    pub messages: Vec<ChatMessage>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StreamPayload {
    pub conversation_id: String,
    pub text: String,
    pub done: bool,
    pub error: Option<String>,
}
