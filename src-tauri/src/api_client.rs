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

// Gemini API request structures
#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig,
}

#[derive(Serialize)]
struct GeminiContent {
    role: String, // "user" or "model"
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum GeminiPart {
    Text { text: String },
    InlineData { inline_data: GeminiInlineData },
}

#[derive(Serialize)]
struct GeminiInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String, // base64
}

#[derive(Serialize)]
struct GeminiGenerationConfig {
    temperature: f32,
}

// Gemini API response stream structures
#[derive(Deserialize)]
struct GeminiStreamChunk {
    candidates: Option<Vec<GeminiStreamCandidate>>,
}

#[derive(Deserialize)]
struct GeminiStreamCandidate {
    content: Option<GeminiStreamContent>,
}

#[derive(Deserialize)]
struct GeminiStreamContent {
    parts: Option<Vec<GeminiStreamPart>>,
}

#[derive(Deserialize)]
struct GeminiStreamPart {
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

fn convert_to_gemini(messages: &[ChatMessage], temperature: f32) -> GeminiRequest {
    let mut contents = Vec::new();

    for msg in messages {
        let role = match msg.role {
            MessageRole::User => "user".to_string(),
            MessageRole::Assistant => "model".to_string(),
            MessageRole::System => continue, // Prepend system message to first user msg below
        };

        let parts = msg
            .content
            .iter()
            .map(|part| match part {
                ContentPart::Text { text } => GeminiPart::Text { text: text.clone() },
                ContentPart::Image { mime_type, base64 } => GeminiPart::InlineData {
                    inline_data: GeminiInlineData {
                        mime_type: mime_type.clone(),
                        data: base64.clone(),
                    },
                },
            })
            .collect();

        contents.push(GeminiContent { role, parts });
    }

    // Prepend system message to first user prompt if any exists
    let system_prompts: Vec<String> = messages
        .iter()
        .filter(|msg| msg.role == MessageRole::System)
        .map(|msg| msg.get_text())
        .collect();

    if !system_prompts.is_empty() {
        let system_text = system_prompts.join("\n\n");
        if let Some(first_user) = contents.iter_mut().find(|c| c.role == "user") {
            if let Some(GeminiPart::Text { text }) = first_user.parts.first_mut() {
                *text = format!("System Instructions:\n{}\n\n{}", system_text, text);
            } else {
                first_user.parts.insert(0, GeminiPart::Text { text: system_text });
            }
        }
    }

    GeminiRequest {
        contents,
        generation_config: GeminiGenerationConfig { temperature },
    }
}

pub async fn fetch_provider_models(
    provider: Provider,
    api_key: String,
    base_url: Option<String>,
) -> Result<Vec<String>, String> {
    println!("Entering fetch_provider_models: provider={:?}, base_url={:?}", provider, base_url);
    let client = reqwest::Client::new();
    match provider {
        Provider::Claude => {
            // Static predefined list
            Ok(vec![
                "claude-3-5-sonnet-latest".to_string(),
                "claude-3-5-sonnet-20241022".to_string(),
                "claude-3-5-haiku-20241022".to_string(),
                "claude-3-opus-20240229".to_string(),
                "claude-3-sonnet-20240229".to_string(),
                "claude-3-haiku-20240307".to_string(),
            ])
        }
        Provider::Gemini => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={}",
                api_key
            );
            let response = client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("Gemini API call failed: {}", e))?;

            if !response.status().is_success() {
                let status = response.status();
                let err_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                return Err(format!("Gemini API error ({}): {}", status, err_text));
            }

            #[derive(Deserialize)]
            struct GeminiModel {
                name: String,
            }
            #[derive(Deserialize)]
            struct GeminiResponse {
                models: Vec<GeminiModel>,
            }

            let body: GeminiResponse = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse Gemini models: {}", e))?;

            let model_names = body
                .models
                .into_iter()
                .map(|m| {
                    if m.name.starts_with("models/") {
                        m.name[7..].to_string()
                    } else {
                        m.name
                    }
                })
                .collect();
            Ok(model_names)
        }
        Provider::OpenAI | Provider::OpenRouter | Provider::CustomOpenAICompliant => {
            let url = if let Some(mut base) = base_url {
                if !base.ends_with('/') {
                    base.push('/');
                }
                if base.contains("/v1/") {
                    format!("{}models", base)
                } else {
                    format!("{}v1/models", base)
                }
            } else {
                match provider {
                    Provider::OpenAI => "https://api.openai.com/v1/models".to_string(),
                    Provider::OpenRouter => "https://openrouter.ai/api/v1/models".to_string(),
                    _ => "https://api.openai.com/v1/models".to_string(), // Fallback
                }
            };
            println!("Sending GET request to custom/OpenAI models endpoint: url={}", url);
            let mut req = client.get(&url);

            req = req.header("Authorization", format!("Bearer {}", api_key));

            let response = req
                .send()
                .await
                .map_err(|e| format!("API request failed: {}", e))?;
            println!("Received response from custom/OpenAI models endpoint: status={}", response.status());

            if !response.status().is_success() {
                let status = response.status();
                let err_text = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                return Err(format!("API error ({}): {}", status, err_text));
            }

            #[derive(Deserialize)]
            struct OpenAiModel {
                id: String,
            }
            #[derive(Deserialize)]
            struct OpenAiResponse {
                data: Vec<OpenAiModel>,
            }

            let body: OpenAiResponse = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse models JSON: {}", e))?;

            let model_names = body.data.into_iter().map(|m| m.id).collect();
            Ok(model_names)
        }
    }
}

pub async fn stream_chat_completion(
    window: tauri::WebviewWindow,
    conversation_id: String,
    api_key: String,
    base_url: Option<String>,
    config: ApiConfig,
    messages: Vec<ChatMessage>,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let conversation_id_clone = conversation_id.clone();
    let window_clone = window.clone();

    let result: Result<(), String> = async move {
        match config.provider {
            Provider::OpenAI | Provider::OpenRouter | Provider::CustomOpenAICompliant => {
                let url = if let Some(mut base) = base_url {
                    if !base.ends_with('/') {
                        base.push('/');
                    }
                    if base.contains("/v1/") {
                        format!("{}chat/completions", base)
                    } else {
                        format!("{}v1/chat/completions", base)
                    }
                } else {
                    match config.provider {
                        Provider::OpenAI => "https://api.openai.com/v1/chat/completions".to_string(),
                        Provider::OpenRouter => "https://openrouter.ai/api/v1/chat/completions".to_string(),
                        _ => "https://api.openai.com/v1/chat/completions".to_string(), // Fallback
                    }
                };
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
                    return Err(format!("API error ({}): {}", status, err_text));
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
            Provider::Claude => {
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
            Provider::Gemini => {
                let url = format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?key={}",
                    config.model, api_key
                );

                let gemini_req = convert_to_gemini(&messages, config.temperature);

                let response = client
                    .post(&url)
                    .header(CONTENT_TYPE, "application/json")
                    .json(&gemini_req)
                    .send()
                    .await
                    .map_err(|e| format!("Gemini API call failed: {}", e))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let err_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    return Err(format!("Gemini API error ({}): {}", status, err_text));
                }

                let mut stream = response.bytes_stream();
                let mut buffer = Vec::new();

                while let Some(chunk_res) = stream.next().await {
                    let chunk = chunk_res.map_err(|e| format!("Stream chunk read failed: {}", e))?;
                    buffer.extend_from_slice(&chunk);

                    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                        let line_bytes = buffer.drain(..pos + 1).collect::<Vec<u8>>();
                        let line = String::from_utf8_lossy(&line_bytes);
                        let mut chunk_text = line.trim();
                        if chunk_text.is_empty() {
                            continue;
                        }

                        // Gemini REST stream items are items of a JSON array, e.g. [, { ... }]
                        if chunk_text.starts_with('[') {
                            chunk_text = &chunk_text[1..];
                        }
                        if chunk_text.starts_with(',') {
                            chunk_text = &chunk_text[1..];
                        }
                        if chunk_text.ends_with(']') {
                            chunk_text = &chunk_text[..chunk_text.len() - 1];
                        }
                        let chunk_text = chunk_text.trim();
                        if chunk_text.is_empty() {
                            continue;
                        }

                        if let Ok(chunk) = serde_json::from_str::<GeminiStreamChunk>(chunk_text) {
                            if let Some(candidates) = chunk.candidates {
                                if let Some(candidate) = candidates.first() {
                                    if let Some(content) = &candidate.content {
                                        if let Some(parts) = &content.parts {
                                            if let Some(part) = parts.first() {
                                                if let Some(text) = &part.text {
                                                    window
                                                        .emit(
                                                            "chat-stream-chunk",
                                                            StreamPayload {
                                                                conversation_id: conversation_id.clone(),
                                                                text: text.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{ChatMessage, MessageRole, ContentPart};

    #[test]
    fn test_convert_to_openai_message() {
        let msg = ChatMessage::new_text(MessageRole::User, "Hello".to_string());
        let openai_msg = convert_to_openai_message(&msg);
        assert_eq!(openai_msg.role, "user");
        match openai_msg.content {
            OpenAiContent::Text(text) => assert_eq!(text, "Hello"),
            _ => panic!("Expected OpenAiContent::Text"),
        }

        // Multimodal image message
        let img_msg = ChatMessage {
            role: MessageRole::User,
            content: vec![
                ContentPart::Text { text: "Look at this:".to_string() },
                ContentPart::Image { mime_type: "image/png".to_string(), base64: "dGVzdA==".to_string() },
            ],
        };
        let openai_img_msg = convert_to_openai_message(&img_msg);
        assert_eq!(openai_img_msg.role, "user");
        match openai_img_msg.content {
            OpenAiContent::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                match &parts[0] {
                    OpenAiContentPart::Text { text } => assert_eq!(text, "Look at this:"),
                    _ => panic!("Expected Text part"),
                }
                match &parts[1] {
                    OpenAiContentPart::ImageUrl { image_url } => {
                        assert_eq!(image_url.url, "data:image/png;base64,dGVzdA==");
                    }
                    _ => panic!("Expected ImageUrl part"),
                }
            }
            _ => panic!("Expected OpenAiContent::Parts"),
        }
    }

    #[test]
    fn test_convert_to_anthropic() {
        let messages = vec![
            ChatMessage::new_text(MessageRole::System, "You are a helpful assistant".to_string()),
            ChatMessage::new_text(MessageRole::User, "Hello".to_string()),
        ];
        let req = convert_to_anthropic(&messages, "claude-3-5-sonnet".to_string(), 0.7, Some(1024));
        assert_eq!(req.model, "claude-3-5-sonnet");
        assert_eq!(req.system, Some("You are a helpful assistant".to_string()));
        assert_eq!(req.max_tokens, 1024);
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
        match &req.messages[0].content {
            AnthropicContent::Text(text) => assert_eq!(text, "Hello"),
            _ => panic!("Expected AnthropicContent::Text"),
        }
    }

    #[test]
    fn test_convert_to_gemini() {
        let messages = vec![
            ChatMessage::new_text(MessageRole::System, "Keep it short".to_string()),
            ChatMessage::new_text(MessageRole::User, "Explain Rust in 1 sentence".to_string()),
        ];
        let req = convert_to_gemini(&messages, 0.5);
        assert_eq!(req.generation_config.temperature, 0.5);
        assert_eq!(req.contents.len(), 1);
        assert_eq!(req.contents[0].role, "user");
        assert_eq!(req.contents[0].parts.len(), 1);
        match &req.contents[0].parts[0] {
            GeminiPart::Text { text } => {
                assert!(text.contains("System Instructions:\nKeep it short"));
                assert!(text.contains("Explain Rust in 1 sentence"));
            }
            _ => panic!("Expected GeminiPart::Text"),
        }
    }

    #[test]
    fn test_gemini_stream_chunk_bracket_parsing() {
        // Test bracket and comma stripping logic
        let raw_chunks = vec![
            "[  {\"candidates\": []} ",
            "  , {\"candidates\": []} ",
            "  , {\"candidates\": []} ] ",
        ];

        let mut parsed_count = 0;
        for raw in raw_chunks {
            let mut chunk_text = raw.trim();
            if chunk_text.starts_with('[') {
                chunk_text = &chunk_text[1..];
            }
            if chunk_text.starts_with(',') {
                chunk_text = &chunk_text[1..];
            }
            if chunk_text.ends_with(']') {
                chunk_text = &chunk_text[..chunk_text.len() - 1];
            }
            let chunk_text = chunk_text.trim();
            assert!(!chunk_text.is_empty());
            
            let chunk_res: Result<GeminiStreamChunk, _> = serde_json::from_str(chunk_text);
            assert!(chunk_res.is_ok());
            parsed_count += 1;
        }
        assert_eq!(parsed_count, 3);
    }

    #[tokio::test]
    async fn test_local_lm_studio_fetching() {
        if std::net::TcpStream::connect("127.0.0.1:1234").is_err() {
            println!("Local LM Studio is not running on 127.0.0.1:1234, skipping local fetch test.");
            return;
        }

        let models = fetch_provider_models(
            Provider::CustomOpenAICompliant,
            "dummy".to_string(),
            Some("http://127.0.0.1:1234/v1".to_string()),
        )
        .await;

        assert!(models.is_ok(), "Failed to fetch models from local LM Studio: {:?}", models.err());
        let list = models.unwrap();
        println!("Fetched local models: {:?}", list);
        assert!(!list.is_empty(), "No models returned by local LM Studio");
    }

    #[tokio::test]
    async fn test_local_lm_studio_streaming_completion() {
        use futures_util::StreamExt;

        if std::net::TcpStream::connect("127.0.0.1:1234").is_err() {
            println!("Local LM Studio is not running, skipping local streaming test.");
            return;
        }

        let client = reqwest::Client::new();
        let openai_req = OpenAiRequest {
            model: "gemma-3-270m-it".to_string(),
            messages: vec![OpenAiMessage {
                role: "user".to_string(),
                content: OpenAiContent::Text("Hello, list 3 colors".to_string()),
            }],
            stream: true,
            temperature: Some(0.7),
            max_tokens: Some(50),
        };

        let response = client
            .post("http://127.0.0.1:1234/v1/chat/completions")
            .header("Authorization", "Bearer dummy")
            .header("Content-Type", "application/json")
            .json(&openai_req)
            .send()
            .await;

        assert!(response.is_ok(), "Request failed: {:?}", response.err());
        let res = response.unwrap();
        assert!(res.status().is_success(), "Response status error: {}", res.status());

        let mut stream = res.bytes_stream();
        let mut buffer = Vec::new();
        let mut tokens = Vec::new();

        while let Some(chunk_res) = stream.next().await {
            assert!(chunk_res.is_ok());
            let chunk = chunk_res.unwrap();
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
                            tokens.push(content.clone());
                        }
                    }
                }
            }
        }

        let full_text = tokens.join("");
        println!("Full streamed response: {}", full_text);
        assert!(!full_text.is_empty(), "Streamed response was empty");
    }
}

