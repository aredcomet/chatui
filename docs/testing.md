# Testing and Development Guide

This document outlines how to programmatically test the `chatui` application, bypass credential prompt hurdles during development, and debug streaming operations.

---

## 1. Programmatic Testing (`cargo test`)

We can run unit and integration tests programmatically from the Rust backend using:
```bash
rtk cargo test -p chatui
```

### Mocking the Tauri Environment
Because the inference engine ([api_client.rs](file:///Users/bran/src/play/chatui/src-tauri/src/api_client.rs)) requires a Tauri window to emit real-time chunks, we refactored the functions to be generic over Tauri's [Runtime](https://docs.rs/tauri/latest/tauri/trait.Runtime.html):

```rust
pub async fn stream_chat_completion<R: tauri::Runtime>(
    window: tauri::WebviewWindow<R>,
    // ...
)
```

In unit/integration tests, we mock the application and window using `tauri::test`:
```rust
#[tokio::test]
async fn test_mock_window_streaming() {
    // 1. Initialize a mock app context with noop assets
    let app = tauri::test::mock_builder()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();

    // 2. Build a mock WebviewWindow under the MockRuntime
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    // 3. Call the generic stream completion engine
    let res = stream_chat_completion(
        window,
        "test-conv".to_string(),
        "dummy-key".to_string(),
        Some("http://127.0.0.1:1234/v1".to_string()),
        config,
        messages,
    ).await;
}
```

---

## 2. Bypassing macOS Keychain Prompts in Development

### The Hurdle
Because code changes alter the application binary signature on every rebuild, macOS Keychain prompts the developer for the system password each time the compiled binary attempts to access the OS Keychain (via the `keyring` crate).

### The Solution
We implemented a conditional developer fallback in [credentials.rs](file:///Users/bran/src/play/chatui/src-tauri/src/credentials.rs) using Rust's `#[cfg(debug_assertions)]`.
- **In Release Mode (`#[cfg(not(debug_assertions))]`)**: The application uses the highly secure, native macOS/System OS Keychain via `keyring`.
- **In Development/Debug Mode (`#[cfg(debug_assertions)]`)**: The application automatically falls back to storing API keys and connection credentials in a local unencrypted JSON file: `~/.chatui-keys-dev.json`.

This maintains maximum production security while completely eliminating annoying system prompts during rapid local coding cycles.

---

## 3. Troubleshooting and Debugging Streaming

If local OpenAI-compliant models or proxies (e.g., LM Studio, Ollama) appear to block or buffer response streams:

### Robust Prefix Handling
Some local OpenAI-compliant servers omit the space after the `data:` prefix (e.g. sending `data:{"choices":...}`). The streaming parser has been updated to support both styles:
```rust
if line.starts_with("data:") {
    let data = line["data:".len()..].trim();
    // ...
}
```

### Verbose Logging
Logging has been added to trace the lifecycle of stream events across the boundaries:
1. **Rust Backend**: Prints incoming raw HTTP chunks and emitted token payloads to stdout. Look for lines prefixing:
   - `Backend: received chunk of size X bytes`
   - `Backend: processing line -> ...`
   - `Backend: emitting token #X -> ...`
2. **Frontend Wasm**: Prints received event payloads directly to the browser console. Look for:
   - `Frontend: received chat-stream-chunk payload: ...` in the browser devtools console.

---

## 4. Local OpenAI-Compliant Server Testing (e.g., LM Studio)

To test a local inference server directly using `curl`, run:
```bash
curl -s http://127.0.0.1:1234/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "ministral-3-3b-reasoning-2512",
    "messages": [{"role": "user", "content": "say hi"}],
    "stream": true
  }'
```

This returns Server-Sent Event (SSE) chunks formatted as:
```
data: {"id":"chatcmpl-...","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}
data: [DONE]
```

---

## 5. Token Generation Speed Calculation (tokens/sec)

### The Problem
Previously, the generation speed (`tokens_per_sec`) was calculated by dividing the total tokens by the total request duration:
$$\text{Tokens/sec} = \frac{\text{Token Count}}{\text{Total Request Time}}$$
This was incorrect because the total request time includes prefill latency, connection time, and network handshake overhead. This resulted in reporting much lower throughput (e.g., 15 tok/sec instead of the actual 30+ tok/sec generation speed).

### The Solution
We changed the calculation to divide token count by the **active generation duration** (time elapsed between the arrival of the first token and the final chunk):
$$\text{Tokens/sec} = \frac{\text{Token Count}}{\text{Total Request Time} - \text{Time To First Token (TTFT)}}$$

Implementation in [api_client.rs](file:///Users/bran/src/play/chatui/src-tauri/src/api_client.rs):
```rust
let total_duration = start_time.elapsed();
let tokens_per_sec = if let Some(first_tok_dur) = first_token_time {
    let gen_duration = total_duration.saturating_sub(first_tok_dur);
    if gen_duration.as_secs_f32() > 0.0 {
        token_count as f32 / gen_duration.as_secs_f32()
    } else {
        0.0
    }
} else {
    0.0
};
```
This reflects the true generation throughput of active model inference.
