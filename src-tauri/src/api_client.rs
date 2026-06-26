use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use shared::{ApiConfig, ChatMessage, ContentPart, MessageRole, Provider, StreamPayload};
use tauri::Emitter;

// OpenAI API request structures
#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    stream: bool,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: OpenAiContent,
}

#[derive(Serialize)]
#[serde(untagged)]
enum OpenAiContent {
    Text(String),
    Parts(Vec<OpenAiContentPart>),
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpenAiContentPart {
    Text { text: String },
    ImageUrl { image_url: OpenAiImageUrl },
}

#[derive(Serialize)]
struct OpenAiImageUrl {
    url: String,
}

// OpenAI API response stream structures
#[derive(Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiStreamDelta,
}

#[derive(Deserialize)]
struct OpenAiStreamDelta {
    content: Option<String>,
}

// Anthropic API request structures
#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    system: Option<String>,
    stream: bool,
    max_tokens: u32,
    temperature: Option<f32>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Parts(Vec<AnthropicContentPart>),
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentPart {
    Text { text: String },
    Image { source: AnthropicImageSource },
}

#[derive(Serialize)]
struct AnthropicImageSource {
    #[serde(rename = "type")]
    source_type: String, // "base64"
    media_type: String,  // e.g. "image/jpeg"
    data: String,        // base64 data
}

// Anthropic API response stream structures
#[derive(Deserialize)]
struct AnthropicStreamChunk {
    #[serde(rename = "type")]
    chunk_type: String,
    delta: Option<AnthropicStreamDelta>,
}

#[derive(Deserialize)]
struct AnthropicStreamDelta {
    text: Option<String>,
}

fn convert_to_openai_message(msg: &ChatMessage) -> OpenAiMessage {
    let role = match msg.role {
        MessageRole::System => "system".to_string(),
        MessageRole::User => "user".to_string(),
        MessageRole::Assistant => "assistant".to_string(),
    };

    let content = if msg.content.len() == 1 {
        match &msg.content[0] {
            ContentPart::Text { text } => OpenAiContent::Text(text.clone()),
            ContentPart::Image { mime_type, base64 } => {
                OpenAiContent::Parts(vec![OpenAiContentPart::ImageUrl {
                    image_url: OpenAiImageUrl {
                        url: format!("data:{};base64,{}", mime_type, base64),
                    },
                }])
            }
        }
    } else {
        let parts = msg
            .content
            .iter()
            .map(|part| match part {
                ContentPart::Text { text } => OpenAiContentPart::Text { text: text.clone() },
                ContentPart::Image { mime_type, base64 } => OpenAiContentPart::ImageUrl {
                    image_url: OpenAiImageUrl {
                        url: format!("data:{};base64,{}", mime_type, base64),
                    },
                },
            })
            .collect();
        OpenAiContent::Parts(parts)
    };

    OpenAiMessage { role, content }
}

fn convert_to_anthropic(
    messages: &[ChatMessage],
    model: String,
    temperature: f32,
    max_tokens: Option<u32>,
) -> AnthropicRequest {
    let mut system_prompts = Vec::new();
    let mut anthropic_messages = Vec::new();

    for msg in messages {
        match msg.role {
            MessageRole::System => {
                system_prompts.push(msg.get_text());
            }
            MessageRole::User | MessageRole::Assistant => {
                let role = match msg.role {
                    MessageRole::User => "user".to_string(),
                    _ => "assistant".to_string(),
                };

                let content = if msg.content.len() == 1 {
                    match &msg.content[0] {
                        ContentPart::Text { text } => AnthropicContent::Text(text.clone()),
                        ContentPart::Image { mime_type, base64 } => {
                            AnthropicContent::Parts(vec![AnthropicContentPart::Image {
                                source: AnthropicImageSource {
                                    source_type: "base64".to_string(),
                                    media_type: mime_type.clone(),
                                    data: base64.clone(),
                                },
                            }])
                        }
                    }
                } else {
                    let parts = msg
                        .content
                        .iter()
                        .map(|part| match part {
                            ContentPart::Text { text } => {
                                AnthropicContentPart::Text { text: text.clone() }
                            }
                            ContentPart::Image { mime_type, base64 } => {
                                AnthropicContentPart::Image {
                                    source: AnthropicImageSource {
                                        source_type: "base64".to_string(),
                                        media_type: mime_type.clone(),
                                        data: base64.clone(),
                                    },
                                }
                            }
                        })
                        .collect();
                    AnthropicContent::Parts(parts)
                };

                anthropic_messages.push(AnthropicMessage { role, content });
            }
        }
    }

    let system = if system_prompts.is_empty() {
        None
    } else {
        Some(system_prompts.join("\n\n"))
    };

    AnthropicRequest {
        model,
        messages: anthropic_messages,
        system,
        stream: true,
        max_tokens: max_tokens.unwrap_or(4096),
        temperature: Some(temperature),
    }
}

pub async fn stream_chat_completion(
    window: tauri::WebviewWindow,
    conversation_id: String,
    api_key: String,
    config: ApiConfig,
    messages: Vec<ChatMessage>,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let conversation_id_clone = conversation_id.clone();
    let window_clone = window.clone();

    let result: Result<(), String> = async move {
        match config.provider {
            Provider::OpenAi => {
                let url = "https://api.openai.com/v1/chat/completions";
                let mut headers = HeaderMap::new();
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {}", api_key))
                        .map_err(|e| e.to_string())?,
                );
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

                let openai_req = OpenAiRequest {
                    model: config.model,
                    messages: messages.iter().map(convert_to_openai_message).collect(),
                    stream: true,
                    temperature: Some(config.temperature),
                    max_tokens: config.max_tokens,
                };

                let response = client
                    .post(url)
                    .headers(headers)
                    .json(&openai_req)
                    .send()
                    .await
                    .map_err(|e| format!("Network request failed: {}", e))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let err_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    return Err(format!("OpenAI API error ({}): {}", status, err_text));
                }

                let mut stream = response.bytes_stream();
                let mut buffer = Vec::new();

                while let Some(chunk_res) = stream.next().await {
                    let chunk = chunk_res.map_err(|e| format!("Stream chunk read failed: {}", e))?;
                    buffer.extend_from_slice(&chunk);

                    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                        let line_bytes = buffer.drain(..pos + 1).collect::<Vec<u8>>();
                        let line = String::from_utf8_lossy(&line_bytes);
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        if line.starts_with("data: ") {
                            let data = &line[6..];
                            if data == "[DONE]" {
                                break;
                            }

                            if let Ok(chunk) = serde_json::from_str::<OpenAiStreamChunk>(data) {
                                if let Some(content) = chunk
                                    .choices
                                    .first()
                                    .and_then(|c| c.delta.content.as_ref())
                                {
                                    window
                                        .emit(
                                            "chat-stream-chunk",
                                            StreamPayload {
                                                conversation_id: conversation_id.clone(),
                                                text: content.clone(),
                                                done: false,
                                                error: None,
                                            },
                                        )
                                        .map_err(|e| e.to_string())?;
                                }
                            }
                        }
                    }
                }
            }
            Provider::Anthropic => {
                let url = "https://api.anthropic.com/v1/messages";
                let mut headers = HeaderMap::new();
                headers.insert(
                    "x-api-key",
                    HeaderValue::from_str(&api_key).map_err(|e| e.to_string())?,
                );
                headers.insert(
                    "anthropic-version",
                    HeaderValue::from_static("2023-06-01"),
                );
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

                let anthropic_req = convert_to_anthropic(
                    &messages,
                    config.model,
                    config.temperature,
                    config.max_tokens,
                );

                let response = client
                    .post(url)
                    .headers(headers)
                    .json(&anthropic_req)
                    .send()
                    .await
                    .map_err(|e| format!("Network request failed: {}", e))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let err_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    return Err(format!(
                        "Anthropic API error ({}): {}",
                        status,
                        err_text
                    ));
                }

                let mut stream = response.bytes_stream();
                let mut buffer = Vec::new();

                while let Some(chunk_res) = stream.next().await {
                    let chunk = chunk_res.map_err(|e| format!("Stream chunk read failed: {}", e))?;
                    buffer.extend_from_slice(&chunk);

                    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                        let line_bytes = buffer.drain(..pos + 1).collect::<Vec<u8>>();
                        let line = String::from_utf8_lossy(&line_bytes);
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        if line.starts_with("data: ") {
                            let data = &line[6..];
                            if let Ok(chunk) = serde_json::from_str::<AnthropicStreamChunk>(data) {
                                if chunk.chunk_type == "content_block_delta" {
                                    if let Some(delta) = chunk.delta {
                                        if let Some(text) = delta.text {
                                            window
                                                .emit(
                                                    "chat-stream-chunk",
                                                    StreamPayload {
                                                        conversation_id: conversation_id.clone(),
                                                        text,
                                                        done: false,
                                                        error: None,
                                                    },
                                                )
                                                .map_err(|e| e.to_string())?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    .await;

    // Emit final chunk or error
    match result {
        Ok(_) => {
            let _ = window_clone.emit(
                "chat-stream-chunk",
                StreamPayload {
                    conversation_id: conversation_id_clone,
                    text: "".to_string(),
                    done: true,
                    error: None,
                },
            );
            Ok(())
        }
        Err(e) => {
            let _ = window_clone.emit(
                "chat-stream-chunk",
                StreamPayload {
                    conversation_id: conversation_id_clone,
                    text: "".to_string(),
                    done: true,
                    error: Some(e.clone()),
                },
            );
            Err(e)
        }
    }
}
