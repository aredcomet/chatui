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
pub enum ContentBlock {
    Text {
        text: String,
    },
    Reasoning {
        text: String,
    },
    Image {
        path: String,
        mime_type: String,
    },
    Document {
        path: Option<String>,
        mime_type: String,
    },
    Audio {
        path: String,
        duration_secs: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageMetadata {
    pub model: String,
    pub provider: Provider,
    pub connection_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub ttft_ms: Option<u64>,
    pub tokens_per_sec: Option<f32>,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageVersion {
    pub content: Vec<ContentBlock>,
    pub metadata: MessageMetadata,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub versions: Vec<MessageVersion>,
    pub active_version: usize,
}

// Backward compatibility helper for deserializing old ChatMessages
#[derive(Deserialize)]
#[serde(untagged)]
enum ChatMessageDeHelper {
    Current {
        id: String,
        role: MessageRole,
        versions: Vec<MessageVersion>,
        active_version: usize,
    },
    Legacy {
        role: MessageRole,
        content: Vec<LegacyContentPart>,
        #[serde(default)]
        ttft_ms: Option<u64>,
        #[serde(default)]
        tokens_per_sec: Option<f32>,
        #[serde(default)]
        stop_reason: Option<String>,
        #[serde(default)]
        reasoning_duration_ms: Option<u64>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum LegacyContentPart {
    Text { text: String },
    Image { mime_type: String, base64: String },
}

impl<'de> Deserialize<'de> for ChatMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = ChatMessageDeHelper::deserialize(deserializer)?;
        match helper {
            ChatMessageDeHelper::Current {
                id,
                role,
                versions,
                active_version,
            } => Ok(ChatMessage {
                id,
                role,
                versions,
                active_version,
            }),
            ChatMessageDeHelper::Legacy {
                role,
                content,
                ttft_ms,
                tokens_per_sec,
                stop_reason,
                reasoning_duration_ms,
            } => {
                let mut content_blocks = Vec::new();
                for part in content {
                    match part {
                        LegacyContentPart::Text { text } => {
                            // If it has think tags, we might separate them later, but let's keep it simple
                            content_blocks.push(ContentBlock::Text { text });
                        }
                        LegacyContentPart::Image { mime_type, base64 } => {
                            // Legacy images were base64 inline strings, map to path empty for now
                            content_blocks.push(ContentBlock::Image {
                                path: format!("data:{};base64,{}", mime_type, base64),
                                mime_type,
                            });
                        }
                    }
                }
                
                if let Some(ms) = reasoning_duration_ms {
                    content_blocks.push(ContentBlock::Reasoning {
                        text: format!("Thinking took {}ms", ms),
                    });
                }

                let dummy_metadata = MessageMetadata {
                    model: "legacy".to_string(),
                    provider: Provider::OpenAI,
                    connection_id: "legacy".to_string(),
                    created_at: chrono::Utc::now(),
                    ttft_ms,
                    tokens_per_sec,
                    stop_reason,
                };

                Ok(ChatMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    role,
                    versions: vec![MessageVersion {
                        content: content_blocks,
                        metadata: dummy_metadata,
                    }],
                    active_version: 0,
                })
            }
        }
    }
}

impl ChatMessage {
    pub fn new_text(role: MessageRole, text: String) -> Self {
        let metadata = MessageMetadata {
            model: "unknown".to_string(),
            provider: Provider::OpenAI,
            connection_id: "unknown".to_string(),
            created_at: chrono::Utc::now(),
            ttft_ms: None,
            tokens_per_sec: None,
            stop_reason: None,
        };
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            versions: vec![MessageVersion {
                content: vec![ContentBlock::Text { text }],
                metadata,
            }],
            active_version: 0,
        }
    }

    pub fn get_text(&self) -> String {
        if let Some(version) = self.versions.get(self.active_version) {
            version.content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.clone()),
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
    #[serde(default = "chrono::Utc::now")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub folder_id: Option<String>,
    pub system_prompt: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub updated_at: u64,
    #[serde(default)]
    pub connection_id: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_serialization_roundtrip() {
        let msg = ChatMessage::new_text(MessageRole::User, "Hello world".to_string());
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: ChatMessage = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(msg.id, deserialized.id);
        assert_eq!(msg.role, deserialized.role);
        assert_eq!(msg.get_text(), deserialized.get_text());
    }

    #[test]
    fn test_legacy_deserialization() {
        let legacy_json = r#"{
            "role": "user",
            "content": [
                { "type": "text", "text": "Hello legacy world" }
            ],
            "reasoning_duration_ms": 1500
        }"#;

        let deserialized: ChatMessage = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(deserialized.role, MessageRole::User);
        assert_eq!(deserialized.get_text(), "Hello legacy world");
        
        // Assert reasoning block was correctly parsed and pushed
        assert_eq!(deserialized.versions[0].content.len(), 2);
        match &deserialized.versions[0].content[1] {
            ContentBlock::Reasoning { text } => {
                assert!(text.contains("1500ms"));
            }
            _ => panic!("Expected ContentBlock::Reasoning"),
        }
    }
}


