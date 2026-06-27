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
pub struct ModelReasoningConfig {
    pub model_id: String,
    pub enabled: bool,
    pub is_raw_stream: bool,
    pub start_tag: String,
    pub end_tag: String,
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
    #[serde(default)]
    pub reasoning_configs: Vec<ModelReasoningConfig>,
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
pub struct MessageVersion {
    pub content: Vec<ContentPart>,
    pub ttft_ms: Option<u64>,
    pub tokens_per_sec: Option<f32>,
    pub total_tokens: Option<u32>,
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub reasoning_duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub versions: Vec<MessageVersion>,
    pub active_version: usize,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ChatMessageDeHelper {
    Current {
        role: MessageRole,
        versions: Vec<MessageVersion>,
        active_version: usize,
    },
    Legacy {
        role: MessageRole,
        content: Vec<ContentPart>,
    },
}

impl<'de> Deserialize<'de> for ChatMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = ChatMessageDeHelper::deserialize(deserializer)?;
        match helper {
            ChatMessageDeHelper::Current {
                role,
                versions,
                active_version,
            } => Ok(ChatMessage {
                role,
                versions,
                active_version,
            }),
            ChatMessageDeHelper::Legacy { role, content } => Ok(ChatMessage {
                role,
                versions: vec![MessageVersion {
                    content,
                    ttft_ms: None,
                    tokens_per_sec: None,
                    total_tokens: None,
                    stop_reason: None,
                    reasoning_duration_ms: None,
                }],
                active_version: 0,
            }),
        }
    }
}

impl ChatMessage {
    pub fn new_text(role: MessageRole, text: String) -> Self {
        Self {
            role,
            versions: vec![MessageVersion {
                content: vec![ContentPart::Text { text }],
                ttft_ms: None,
                tokens_per_sec: None,
                total_tokens: None,
                stop_reason: None,
                reasoning_duration_ms: None,
            }],
            active_version: 0,
        }
    }

    pub fn get_text(&self) -> String {
        if let Some(version) = self.versions.get(self.active_version) {
            version.content
                .iter()
                .filter_map(|part| match part {
                    ContentPart::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new()
        }
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
    pub ttft_ms: Option<u64>,
    pub tokens_per_sec: Option<f32>,
    pub total_tokens: Option<u32>,
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub reasoning_duration_ms: Option<u64>,
}
