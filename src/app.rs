use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::Serialize;
use shared::{
    ApiConfig, ChatConversation, ChatMessage, Connection, ContentPart, MessageRole, Provider,
    StreamPayload, MessageVersion, ModelReasoningConfig,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    fn invoke_raw(cmd: &str, args: JsValue) -> js_sys::Promise;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;

    #[wasm_bindgen(js_name = eval)]
    fn eval_js(s: &str);
}

// Helper function to call Tauri commands safely and catch exceptions without panicking Wasm
async fn invoke(cmd: &str, args: JsValue) -> JsValue {
    let promise = invoke_raw(cmd, args);
    match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(val) => val,
        Err(err) => {
            web_sys::console::error_1(&err);
            JsValue::NULL
        }
    }
}

// Arguments for Tauri commands
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FetchModelsArgs {
    provider: Provider,
    api_key: String,
    base_url: Option<String>,
}

#[derive(Serialize)]
struct SaveConnectionsArgs {
    connections: Vec<Connection>,
}

#[derive(Serialize)]
struct DeleteConnectionArgs {
    id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SendMessageStreamArgs {
    conversation_id: String,
    config: ApiConfig,
    messages: Vec<ChatMessage>,
}

#[derive(Serialize)]
struct SaveConversationArgs {
    conversation: ChatConversation,
}

#[derive(Serialize)]
struct DeleteConversationArgs {
    id: String,
}

fn read_file_as_data_url(file: &web_sys::File) -> Result<js_sys::Promise, JsValue> {
    let reader = web_sys::FileReader::new()?;
    let reader_c = reader.clone();
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let reader_inner = reader_c.clone();
        let onload = Closure::wrap(Box::new(move |_: web_sys::Event| {
            if let Ok(result) = reader_inner.result() {
                let _ = resolve.call1(&JsValue::UNDEFINED, &result);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);

        let onerror = Closure::wrap(Box::new(move |_: web_sys::Event| {
            let _ = reject.call1(&JsValue::UNDEFINED, &JsValue::from_str("Error reading file"));
        }) as Box<dyn FnMut(web_sys::Event)>);

        reader_c.set_onload(Some(onload.as_ref().unchecked_ref()));
        reader_c.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        onload.forget();
        onerror.forget();
    });
    reader.read_as_data_url(file)?;
    Ok(promise)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
pub enum AppTheme {
    Light,
    Dark,
}

impl AppTheme {
    fn to_class(self) -> &'static str {
        match self {
            AppTheme::Light => "theme-light",
            AppTheme::Dark => "theme-dark",
        }
    }
}

fn get_saved_theme() -> AppTheme {
    if let Some(w) = web_sys::window() {
        if let Ok(Some(ls)) = w.local_storage() {
            if let Ok(Some(val)) = ls.get_item("chatui_theme") {
                match val.as_str() {
                    "light" => return AppTheme::Light,
                    "dark" => return AppTheme::Dark,
                    _ => {}
                }
            }
        }
    }
    AppTheme::Dark // Default to Dark
}

fn save_theme(theme: AppTheme) {
    if let Some(w) = web_sys::window() {
        if let Ok(Some(ls)) = w.local_storage() {
            let val = match theme {
                AppTheme::Light => "light",
                AppTheme::Dark => "dark",
            };
            let _ = ls.set_item("chatui_theme", val);
        }
    }
}

fn render_inline(text: String) -> Vec<AnyView> {
    let mut views = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '*' && chars[i+1] == '*' {
            if !current.is_empty() {
                let c = current.clone();
                views.push(view! { <span>{c}</span> }.into_any());
                current.clear();
            }
            let mut j = i + 2;
            let mut found = false;
            while j + 1 < chars.len() {
                if chars[j] == '*' && chars[j+1] == '*' {
                    found = true;
                    break;
                }
                j += 1;
            }
            if found {
                let bold_text: String = chars[i+2..j].iter().collect();
                views.push(view! { <strong class="font-bold text-theme-text">{bold_text}</strong> }.into_any());
                i = j + 2;
            } else {
                current.push('*');
                current.push('*');
                i += 2;
            }
        } else if chars[i] == '`' {
            if !current.is_empty() {
                let c = current.clone();
                views.push(view! { <span>{c}</span> }.into_any());
                current.clear();
            }
            let mut j = i + 1;
            let mut found = false;
            while j < chars.len() {
                if chars[j] == '`' {
                    found = true;
                    break;
                }
                j += 1;
            }
            if found {
                let code_text: String = chars[i+1..j].iter().collect();
                views.push(view! { <code class="px-1.5 py-0.5 rounded bg-theme-panel font-mono text-[13px] text-theme-accent">{code_text}</code> }.into_any());
                i = j + 1;
            } else {
                current.push('`');
                i += 1;
            }
        } else if chars[i] == '*' {
            if !current.is_empty() {
                let c = current.clone();
                views.push(view! { <span>{c}</span> }.into_any());
                current.clear();
            }
            let mut j = i + 1;
            let mut found = false;
            while j < chars.len() {
                if chars[j] == '*' {
                    if j + 1 < chars.len() && chars[j+1] == '*' {
                        j += 2;
                        continue;
                    }
                    found = true;
                    break;
                }
                j += 1;
            }
            if found {
                let italic_text: String = chars[i+1..j].iter().collect();
                views.push(view! { <em class="italic text-theme-text">{italic_text}</em> }.into_any());
                i = j + 1;
            } else {
                current.push('*');
                i += 1;
            }
        } else {
            current.push(chars[i]);
            i += 1;
        }
    }
    if !current.is_empty() {
        views.push(view! { <span>{current}</span> }.into_any());
    }
    views
}

fn render_latex_and_mermaid() {
    let script = r#"
        setTimeout(() => {
            if (window.renderMathInElement) {
                window.renderMathInElement(document.body, {
                    delimiters: [
                        {left: '$$', right: '$$', display: true},
                        {left: '$', right: '$', display: false},
                        {left: '\\(', right: '\\)', display: false},
                        {left: '\\[', right: '\\]', display: true}
                    ],
                    throwOnError: false
                });
            }
            if (window.mermaid) {
                try {
                    window.mermaid.run();
                } catch(e) {
                    console.error("Mermaid initialization failed", e);
                }
            }
        }, 50);
    "#;
    eval_js(script);
}

fn update_model_reasoning_config<F>(
    configs_signal: WriteSignal<Vec<ModelReasoningConfig>>,
    current_configs: Vec<ModelReasoningConfig>,
    model_id: String,
    mutator: F,
) where
    F: FnOnce(&mut ModelReasoningConfig),
{
    let mut configs = current_configs;
    if let Some(config) = configs.iter_mut().find(|c| c.model_id == model_id) {
        mutator(config);
    } else {
        let mut new_config = ModelReasoningConfig {
            model_id: model_id.clone(),
            enabled: false,
            is_raw_stream: false,
            start_tag: "<think>".to_string(),
            end_tag: "</think>".to_string(),
        };
        mutator(&mut new_config);
        configs.push(new_config);
    }
    configs_signal.set(configs);
}

fn parse_thinking_content(text: &str) -> (Option<String>, String) {
    if let Some(start_idx) = text.find("<think>") {
        let content_start = start_idx + "<think>".len();
        if let Some(end_idx) = text[content_start..].find("</think>") {
            let actual_end = content_start + end_idx;
            let thinking = text[content_start..actual_end].to_string();
            let remaining = format!("{}{}", &text[..start_idx], &text[actual_end + "</think>".len()..]);
            (Some(thinking), remaining)
        } else {
            let thinking = text[content_start..].to_string();
            let remaining = text[..start_idx].to_string();
            (Some(thinking), remaining)
        }
    } else {
        (None, text.to_string())
    }
}

#[component]
fn ThinkingBlock(thinking: String, is_thinking: bool, duration_ms: Option<u64>) -> impl IntoView {
    let (collapsed, set_collapsed) = signal(false);
    
    let label = move || {
        if is_thinking {
            "Thinking...".to_string()
        } else if let Some(ms) = duration_ms {
            format!("Thought for {:.1}s", ms as f64 / 1000.0)
        } else {
            "Thought".to_string()
        }
    };
    
    view! {
        <div class="mb-3 rounded-xl border border-theme-border/40 bg-theme-panel/20 overflow-hidden theme-transition w-full">
            <div 
                on:click=move |_| set_collapsed.update(|c| *c = !*c)
                class="flex items-center justify-between px-4 py-3 cursor-pointer hover:bg-theme-border/10 select-none text-xs font-semibold text-theme-muted/80 theme-transition"
            >
                <div class="flex items-center gap-1.5">
                    <svg class="w-3.5 h-3.5 text-theme-accent animate-pulse" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
                    </svg>
                    <span>{label}</span>
                    <Show when=move || is_thinking fallback=move || view! {}>
                        <span class="inline-flex h-1.5 w-1.5 rounded-full bg-theme-accent animate-ping"></span>
                    </Show>
                </div>
                <svg 
                    class=move || format!("w-3.5 h-3.5 transform transition-transform duration-200 {}", if collapsed.get() { "-rotate-90" } else { "" })
                    fill="none" 
                    viewBox="0 0 24 24" 
                    stroke="currentColor" 
                    stroke-width="2.5"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
                </svg>
            </div>
            <div 
                class=move || format!("px-4 pb-4 pt-1 text-[13px] text-theme-muted/90 font-sans leading-relaxed overflow-x-auto select-text {}", if collapsed.get() { "hidden" } else { "block" })
            >
                {render_message_content(thinking.clone())}
            </div>
        </div>
    }
}

fn render_message_content(text: String) -> impl IntoView {
    let mut views = Vec::new();
    let parts = text.split("```");
    let mut is_code = false;

    for part in parts {
        if is_code {
            let code_text = part.trim();
            let mut lines = code_text.lines();
            let first_line = lines.next().unwrap_or("");
            let lang = if first_line.chars().all(|c| c.is_alphabetic()) {
                first_line.to_string()
            } else {
                "".to_string()
            };
            let code_content = if lang.is_empty() {
                code_text.to_string()
            } else {
                lines.collect::<Vec<_>>().join("\n")
            };

            if lang == "mermaid" {
                views.push(view! {
                    <div class="mermaid my-3 p-4 bg-theme-panel/40 border border-theme-border/60 rounded-xl flex justify-center overflow-x-auto">
                        {code_content}
                    </div>
                }.into_any());
            } else {
                views.push(view! {
                    <div class="my-3 rounded-lg overflow-hidden border border-theme-border/60 bg-theme-panel font-mono text-sm max-w-full">
                        <div class="flex justify-between items-center bg-theme-panel/85 px-4 py-1.5 text-xs text-theme-muted border-b border-theme-border/60 select-none">
                            <span>{if lang.is_empty() { "code".to_string() } else { lang.clone() }}</span>
                        </div>
                        <pre class="p-4 overflow-x-auto text-theme-text font-mono">
                            <code>{code_content}</code>
                        </pre>
                    </div>
                }.into_any());
            }
        } else {
            let mut current_paragraph = Vec::new();
            let mut current_list = Vec::new();

            let flush_paragraph = |para: &mut Vec<String>, views: &mut Vec<AnyView>| {
                if !para.is_empty() {
                    let para_text = para.join(" ");
                    views.push(view! {
                        <p class="text-theme-text py-1 leading-relaxed break-words">{render_inline(para_text)}</p>
                    }.into_any());
                    para.clear();
                }
            };

            let flush_list = |list: &mut Vec<String>, views: &mut Vec<AnyView>| {
                if !list.is_empty() {
                    let list_items: Vec<_> = list.drain(..).map(|item| {
                        view! {
                            <li class="list-disc ml-6 text-theme-text py-0.5">{render_inline(item)}</li>
                        }
                    }).collect();
                    views.push(view! {
                        <ul class="space-y-1 my-1">
                            {list_items}
                        </ul>
                    }.into_any());
                }
            };

            let lines: Vec<&str> = part.lines().collect();
            let mut i = 0;

            while i < lines.len() {
                let trimmed = lines[i].trim();
                if trimmed.is_empty() {
                    // Look ahead to see if the next non-empty line is a list item
                    let mut next_list_item = false;
                    let mut j = i + 1;
                    while j < lines.len() {
                        let next_trimmed = lines[j].trim();
                        if !next_trimmed.is_empty() {
                            if next_trimmed.starts_with("- ") || next_trimmed.starts_with("* ") {
                                next_list_item = true;
                            }
                            break;
                        }
                        j += 1;
                    }

                    if !next_list_item {
                        flush_list(&mut current_list, &mut views);
                        flush_paragraph(&mut current_paragraph, &mut views);
                    }
                    i += 1;
                    continue;
                }

                if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                    flush_paragraph(&mut current_paragraph, &mut views);
                    let content = trimmed[2..].to_string();
                    current_list.push(content);
                } else if trimmed.starts_with("### ") {
                    flush_list(&mut current_list, &mut views);
                    flush_paragraph(&mut current_paragraph, &mut views);
                    let content = trimmed[4..].to_string();
                    views.push(view! {
                        <h4 class="text-md font-bold text-theme-text mt-4 mb-2">{render_inline(content)}</h4>
                    }.into_any());
                } else if trimmed.starts_with("## ") {
                    flush_list(&mut current_list, &mut views);
                    flush_paragraph(&mut current_paragraph, &mut views);
                    let content = trimmed[3..].to_string();
                    views.push(view! {
                        <h3 class="text-lg font-bold text-theme-text mt-4 mb-2">{render_inline(content)}</h3>
                    }.into_any());
                } else if trimmed.starts_with("# ") {
                    flush_list(&mut current_list, &mut views);
                    flush_paragraph(&mut current_paragraph, &mut views);
                    let content = trimmed[2..].to_string();
                    views.push(view! {
                        <h2 class="text-xl font-bold text-theme-text mt-4 mb-2">{render_inline(content)}</h2>
                    }.into_any());
                } else {
                    flush_list(&mut current_list, &mut views);
                    current_paragraph.push(trimmed.to_string());
                }
                i += 1;
            }

            flush_list(&mut current_list, &mut views);
            flush_paragraph(&mut current_paragraph, &mut views);
        }
        is_code = !is_code;
    }

    view! {
        <div class="space-y-1 overflow-hidden max-w-full">
            {views}
        </div>
    }
}



#[component]
pub fn App() -> impl IntoView {
    // Conversations state
    let (conversations, set_conversations) = signal(Vec::<ChatConversation>::new());
    let (current_conversation_id, set_current_conversation_id) = signal(None::<String>);
    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
    let (input_text, set_input_text) = signal(String::new());
    let (attached_image, set_attached_image) = signal(None::<(String, String)>);
    // Sidebar rename state
    let (editing_convo_id, set_editing_convo_id) = signal(None::<String>);
    let (editing_convo_title, set_editing_convo_title) = signal(String::new());

    // Theme state
    let (app_theme, set_app_theme) = signal(get_saved_theme());

    // Connections manager state
    let (connections, set_connections) = signal(Vec::<Connection>::new());
    let (active_connection_id, set_active_connection_id) = signal(None::<String>);

    // Chat configuration state
    let (selected_provider, set_selected_provider) = signal(Provider::OpenAI);
    let (selected_model, set_selected_model) = signal("gpt-4o-mini".to_string());
    let (temperature, set_temperature) = signal(0.7f32);

    // Modal settings control
    let (show_settings, set_show_settings) = signal(false);
    let (show_add_connection, set_show_add_connection) = signal(false);
    // Tracks which connection is being edited (None = adding new)
    let (editing_connection_id, set_editing_connection_id) = signal(None::<String>);

    // Add Connection Form state
    let (new_conn_provider, set_new_conn_provider) = signal(Provider::OpenAI);
    let (new_conn_name, set_new_conn_name) = signal(String::new());
    let (new_conn_api_key, set_new_conn_api_key) = signal(String::new());
    let (new_conn_base_url, set_new_conn_base_url) = signal(String::new());
    let (new_conn_fetched_models, set_new_conn_fetched_models) = signal(Vec::<String>::new());
    let (new_conn_search_query, set_new_conn_search_query) = signal(String::new());
    let (new_conn_enabled_models, set_new_conn_enabled_models) = signal(Vec::<String>::new());
    let (new_conn_default_model, set_new_conn_default_model) = signal(String::new());
    let (new_conn_reasoning_configs, set_new_conn_reasoning_configs) = signal(Vec::<ModelReasoningConfig>::new());

    // Fetch models loading & errors
    let (fetching_models_loading, set_fetching_models_loading) = signal(false);
    let (fetching_models_error, set_fetching_models_error) = signal(None::<String>);

    // Streaming state
    let (is_streaming, set_is_streaming) = signal(false);
    let (stream_chunks, set_stream_chunks) = signal(None::<StreamPayload>);

    // Editing message state
    let (editing_message_idx, set_editing_message_idx) = signal(None::<usize>);
    let (editing_message_text, set_editing_message_text) = signal(String::new());

    // Toast notification state
    let (toast_message, set_toast_message) = signal(None::<String>);
    let show_toast = move |msg: String| {
        set_toast_message.set(Some(msg));
        spawn_local(async move {
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                if let Some(w) = web_sys::window() {
                    let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 2000);
                }
            });
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            set_toast_message.set(None);
        });
    };

    // Memoized filtered models checklist
    let filtered_fetched_models = Memo::new(move |_| {
        let query = new_conn_search_query.get().to_lowercase();
        let models = new_conn_fetched_models.get();
        if query.is_empty() {
            models
        } else {
            models
                .into_iter()
                .filter(|m| m.to_lowercase().contains(&query))
                .collect()
        }
    });

    // Scroll helper
    let scroll_chat_to_bottom = move || {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(el) = document.get_element_by_id("chat-messages-container") {
                    el.set_scroll_top(el.scroll_height());
                }
            }
        }
    };

    // Load initial configurations from backend
    let load_init_data = move || {
        spawn_local(async move {
            // Fetch conversations
            let args = serde_wasm_bindgen::to_value(&()).unwrap();
            let res_convs = invoke("load_conversations", args).await;
            if let Ok(convs) =
                serde_wasm_bindgen::from_value::<Vec<ChatConversation>>(res_convs)
            {
                set_conversations.set(convs);
            }

            // Fetch saved connections
            let res_conns = invoke("load_connections", serde_wasm_bindgen::to_value(&()).unwrap()).await;
            if let Ok(conns) = serde_wasm_bindgen::from_value::<Vec<Connection>>(res_conns) {
                set_connections.set(conns.clone());
                if !conns.is_empty() {
                    let first_id = conns[0].id.clone();
                    set_active_connection_id.set(Some(first_id.clone()));
                    set_selected_provider.set(conns[0].provider);
                    set_selected_model.set(conns[0].default_model.clone());
                }
            }
        });
    };

    // Effects for mounting and listening
    Effect::new(move |_| {
        load_init_data();

        let handler = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload")) {
                web_sys::console::log_2(&JsValue::from_str("Frontend: received chat-stream-chunk payload:"), &payload);
                if let Ok(payload_struct) =
                    serde_wasm_bindgen::from_value::<StreamPayload>(payload)
                {
                    set_stream_chunks.set(Some(payload_struct));
                }
            }
        }) as Box<dyn Fn(JsValue)>);

        spawn_local(async move {
            let handler_js = handler.into_js_value();
            listen("chat-stream-chunk", handler_js.unchecked_ref()).await;
        });
    });

    // Effect to auto-resize the inline edit textarea when it is opened/mounted
    Effect::new(move |_| {
        if let Some(_) = editing_message_idx.get() {
            spawn_local(async move {
                let promise = js_sys::Promise::new(&mut |resolve, _| {
                    if let Some(w) = web_sys::window() {
                        let _ = w.request_animation_frame(&resolve);
                    }
                });
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                
                if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                    if let Some(el) = doc.get_element_by_id("inline-edit-textarea") {
                        if let Ok(textarea) = el.dyn_into::<web_sys::HtmlTextAreaElement>() {
                            let style = web_sys::HtmlElement::style(&textarea);
                            let _ = style.set_property("height", "auto");
                            let scroll_height = textarea.scroll_height();
                            let _ = style.set_property("height", &format!("{}px", scroll_height));
                        }
                    }
                }
            });
        }
    });

    // Auto-render LaTeX equations and Mermaid diagrams when streaming completes or messages list changes
    Effect::new(move |_| {
        let streaming = is_streaming.get();
        let _ = messages.get();
        if !streaming {
            render_latex_and_mermaid();
        }
    });

    let get_active_model_reasoning_config = move || {
        let conn_id = active_connection_id.get()?;
        let conns = connections.get();
        let conn = conns.iter().find(|c| c.id == conn_id)?;
        let model = selected_model.get();
        conn.reasoning_configs.iter().find(|rc| rc.model_id == model).cloned()
    };

    // Streaming chunk listener
    Effect::new(move |_| {
        if let Some(payload) = stream_chunks.get() {
            let current_id = current_conversation_id.get_untracked();
            if Some(payload.conversation_id.clone()) == current_id {
                let mut current_msgs = messages.get_untracked();

                if payload.done {
                    set_is_streaming.set(false);
                    if let Some(last_msg) = current_msgs.last_mut() {
                        if last_msg.role == MessageRole::Assistant {
                            if let Some(version) = last_msg.versions.get_mut(last_msg.active_version) {
                                version.ttft_ms = payload.ttft_ms;
                                version.tokens_per_sec = payload.tokens_per_sec;
                                version.total_tokens = payload.total_tokens;
                                version.stop_reason = payload.stop_reason.clone();
                                version.reasoning_duration_ms = payload.reasoning_duration_ms;
                            }
                        }
                    }
                    if let Some(convo_id) = current_id {
                        let mut convos = conversations.get_untracked();
                        if let Some(convo) = convos.iter_mut().find(|c| c.id == convo_id) {
                            convo.messages = current_msgs.clone();
                            convo.updated_at = js_sys::Date::now() as u64;

                            let convo_clone = convo.clone();
                            spawn_local(async move {
                                let args = serde_wasm_bindgen::to_value(
                                    &SaveConversationArgs {
                                        conversation: convo_clone,
                                    },
                                )
                                .unwrap();
                                invoke("save_conversation", args).await;
                            });
                        }
                        set_conversations.set(convos);
                    }
                    set_messages.set(current_msgs);
                } else if let Some(err_msg) = payload.error {
                    set_is_streaming.set(false);
                    current_msgs.push(ChatMessage::new_text(
                        MessageRole::Assistant,
                        format!("⚠️ Error: {}", err_msg),
                    ));
                    set_messages.set(current_msgs);
                } else {
                    if let Some(last_msg) = current_msgs.last_mut() {
                        if last_msg.role == MessageRole::Assistant {
                            if let Some(version) = last_msg.versions.get_mut(last_msg.active_version) {
                                if let Some(ContentPart::Text {
                                    text: ref mut existing_text,
                                }) = version.content.first_mut()
                                {
                                    existing_text.push_str(&payload.text);
                                    
                                    // Normalize custom raw stream tags if configured
                                    if let Some(rc) = get_active_model_reasoning_config() {
                                        if rc.enabled && rc.is_raw_stream {
                                            if !rc.start_tag.is_empty() && rc.start_tag != "<think>" {
                                                *existing_text = existing_text.replace(&rc.start_tag, "<think>");
                                            }
                                            if !rc.end_tag.is_empty() && rc.end_tag != "</think>" {
                                                *existing_text = existing_text.replace(&rc.end_tag, "</think>");
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            let mut text = payload.text;
                            if let Some(rc) = get_active_model_reasoning_config() {
                                if rc.enabled && rc.is_raw_stream {
                                    if !rc.start_tag.is_empty() && rc.start_tag != "<think>" {
                                        text = text.replace(&rc.start_tag, "<think>");
                                    }
                                    if !rc.end_tag.is_empty() && rc.end_tag != "</think>" {
                                        text = text.replace(&rc.end_tag, "</think>");
                                    }
                                }
                            }
                            current_msgs.push(ChatMessage::new_text(
                                MessageRole::Assistant,
                                text,
                            ));
                        }
                    } else {
                        let mut text = payload.text;
                        if let Some(rc) = get_active_model_reasoning_config() {
                            if rc.enabled && rc.is_raw_stream {
                                if !rc.start_tag.is_empty() && rc.start_tag != "<think>" {
                                    text = text.replace(&rc.start_tag, "<think>");
                                }
                                if !rc.end_tag.is_empty() && rc.end_tag != "</think>" {
                                    text = text.replace(&rc.end_tag, "</think>");
                                }
                            }
                        }
                        current_msgs.push(ChatMessage::new_text(
                            MessageRole::Assistant,
                            text,
                        ));
                    }
                    set_messages.set(current_msgs);
                }
                scroll_chat_to_bottom();
            }
        }
    });

    // Handle Active Connection dropdown change
    let on_connection_change = move |ev| {
        let id_str = event_target_value(&ev);
        if id_str.is_empty() {
            set_active_connection_id.set(None);
            if let Some(convo_id) = current_conversation_id.get_untracked() {
                let mut convos = conversations.get_untracked();
                if let Some(convo) = convos.iter_mut().find(|c| c.id == convo_id) {
                    convo.connection_id = None;
                    convo.updated_at = js_sys::Date::now() as u64;

                    let convo_clone = convo.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                            conversation: convo_clone,
                        })
                        .unwrap();
                        invoke("save_conversation", args).await;
                    });
                }
                set_conversations.set(convos);
            }
            return;
        }
        set_active_connection_id.set(Some(id_str.clone()));

        if let Some(conn) = connections
            .get_untracked()
            .iter()
            .find(|c| c.id == id_str)
        {
            let provider = conn.provider;
            let model = conn.default_model.clone();
            let connection_id = Some(id_str);

            set_selected_provider.set(provider);
            set_selected_model.set(model.clone());

            // Save connection change to current conversation
            if let Some(convo_id) = current_conversation_id.get_untracked() {
                let mut convos = conversations.get_untracked();
                if let Some(convo) = convos.iter_mut().find(|c| c.id == convo_id) {
                    convo.provider = provider;
                    convo.model = model;
                    convo.connection_id = connection_id;
                    convo.updated_at = js_sys::Date::now() as u64;

                    let convo_clone = convo.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                            conversation: convo_clone,
                        })
                        .unwrap();
                        invoke("save_conversation", args).await;
                    });
                }
                set_conversations.set(convos);
            }
        }
    };

    let on_model_change = move |ev| {
        let model = event_target_value(&ev);
        set_selected_model.set(model.clone());

        // Save model change to current conversation
        if let Some(convo_id) = current_conversation_id.get_untracked() {
            let mut convos = conversations.get_untracked();
            if let Some(convo) = convos.iter_mut().find(|c| c.id == convo_id) {
                convo.model = model;
                convo.updated_at = js_sys::Date::now() as u64;

                let convo_clone = convo.clone();
                spawn_local(async move {
                    let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                        conversation: convo_clone,
                    })
                    .unwrap();
                    invoke("save_conversation", args).await;
                });
            }
            set_conversations.set(convos);
        }
    };

    let update_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap().dyn_into::<web_sys::HtmlTextAreaElement>().unwrap();
        set_input_text.set(target.value());
        let style = web_sys::HtmlElement::style(&target);
        let _ = style.set_property("height", "auto");
        let scroll_height = target.scroll_height();
        let _ = style.set_property("height", &format!("{}px", scroll_height));
    };

    // Chat navigation functions
    let select_conversation = move |id: String| {
        if is_streaming.get_untracked() {
            return;
        }
        set_current_conversation_id.set(Some(id.clone()));
        if let Some(convo) = conversations
            .get_untracked()
            .iter()
            .find(|c| c.id == id)
        {
            set_messages.set(convo.messages.clone());
            
            // Resolve connection:
            // 1. Try convo.connection_id
            // 2. Try matching convo.provider
            // 3. Fallback to first connection
            let conns = connections.get_untracked();
            let resolved_conn = if let Some(ref conn_id) = convo.connection_id {
                conns.iter().find(|c| &c.id == conn_id).cloned()
            } else {
                None
            };
            
            let resolved_conn = resolved_conn
                .or_else(|| {
                    conns.iter().find(|c| c.provider == convo.provider).cloned()
                })
                .or_else(|| conns.first().cloned());
                
            if let Some(conn) = resolved_conn {
                set_active_connection_id.set(Some(conn.id.clone()));
                set_selected_provider.set(conn.provider);
                
                if !convo.model.is_empty() && conn.enabled_models.contains(&convo.model) {
                    set_selected_model.set(convo.model.clone());
                } else {
                    set_selected_model.set(conn.default_model.clone());
                }
            } else {
                set_active_connection_id.set(None);
                set_selected_provider.set(convo.provider);
                set_selected_model.set(convo.model.clone());
            }

            spawn_local(async move {
                scroll_chat_to_bottom();
            });
        }
    };

    let create_new_chat = move |_| {
        if is_streaming.get_untracked() {
            return;
        }
        let uuid = uuid::Uuid::new_v4().to_string();
        
        let conns = connections.get_untracked();
        let conn_id = active_connection_id.get_untracked();
        let resolved_conn = if let Some(ref cid) = conn_id {
            conns.iter().find(|c| &c.id == cid).cloned()
        } else {
            None
        };
        let resolved_conn = resolved_conn.or_else(|| conns.first().cloned());
        
        let (resolved_conn_id, provider, model) = if let Some(conn) = resolved_conn {
            let cid = conn.id.clone();
            let prov = conn.provider;
            let m = conn.default_model.clone();
            
            set_active_connection_id.set(Some(cid.clone()));
            set_selected_provider.set(prov);
            set_selected_model.set(m.clone());
            
            (Some(cid), prov, m)
        } else {
            (None, selected_provider.get_untracked(), selected_model.get_untracked())
        };

        let new_convo = ChatConversation {
            id: uuid.clone(),
            title: format!("New Chat ({})", provider.to_string()),
            model,
            provider,
            messages: Vec::new(),
            updated_at: js_sys::Date::now() as u64,
            connection_id: resolved_conn_id,
        };

        let mut current_convs = conversations.get_untracked();
        current_convs.insert(0, new_convo.clone());
        set_conversations.set(current_convs);

        set_current_conversation_id.set(Some(uuid.clone()));
        set_messages.set(Vec::new());

        let new_convo_c = new_convo.clone();
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                conversation: new_convo_c,
            })
            .unwrap();
            invoke("save_conversation", args).await;
        });
    };

    let delete_chat = move |id: String, ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        if is_streaming.get_untracked() {
            return;
        }

        let mut current_convs = conversations.get_untracked();
        current_convs.retain(|c| c.id != id);
        set_conversations.set(current_convs);

        if current_conversation_id.get_untracked() == Some(id.clone()) {
            set_current_conversation_id.set(None);
            set_messages.set(Vec::new());
        }

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&DeleteConversationArgs { id }).unwrap();
            invoke("delete_conversation", args).await;
        });
    };

    // Rename a conversation and persist
    let rename_chat = move |id: String, new_title: String| {
        if new_title.trim().is_empty() { return; }
        let mut current_convs = conversations.get_untracked();
        if let Some(convo) = current_convs.iter_mut().find(|c| c.id == id) {
            convo.title = new_title.trim().to_string();
            let convo_clone = convo.clone();
            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&SaveConversationArgs { conversation: convo_clone }).unwrap();
                invoke("save_conversation", args).await;
            });
        }
        set_conversations.set(current_convs);
        set_editing_convo_id.set(None);
        set_editing_convo_title.set(String::new());
    };

    // VLM File Select
    let on_file_change = move |ev: web_sys::Event| {
        let target = ev
            .target()
            .unwrap()
            .dyn_into::<web_sys::HtmlInputElement>()
            .unwrap();
        if let Some(files) = target.files() {
            if files.length() > 0 {
                if let Some(file) = files.get(0) {
                    let file_type = file.type_();
                    if file_type.starts_with("image/") {
                        spawn_local(async move {
                            if let Ok(promise) = read_file_as_data_url(&file) {
                                if let Ok(res) =
                                    wasm_bindgen_futures::JsFuture::from(promise).await
                                {
                                    if let Some(result_str) = res.as_string() {
                                        if let Some(comma_pos) = result_str.find(',') {
                                            let base64 = result_str[comma_pos + 1..].to_string();
                                            let mime = file_type.clone();
                                            set_attached_image.set(Some((mime, base64)));
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
            }
        }
    };

    // Drag and Drop
    let handle_drag_over = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
    };

    let handle_drop = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        if let Some(dt) = ev.data_transfer() {
            if let Some(files) = dt.files() {
                if files.length() > 0 {
                    if let Some(file) = files.get(0) {
                        let file_type = file.type_();
                        if file_type.starts_with("image/") {
                            spawn_local(async move {
                                if let Ok(promise) = read_file_as_data_url(&file) {
                                    if let Ok(res) =
                                        wasm_bindgen_futures::JsFuture::from(promise).await
                                    {
                                        if let Some(result_str) = res.as_string() {
                                            if let Some(comma_pos) = result_str.find(',') {
                                                let base64 =
                                                    result_str[comma_pos + 1..].to_string();
                                                let mime = file_type.clone();
                                                set_attached_image.set(Some((mime, base64)));
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }
    };

    // Message Control Helpers
    let copy_message_text = move |text: String| {
        if let Some(window) = web_sys::window() {
            let navigator = window.navigator();
            let clipboard = navigator.clipboard();
            let _ = clipboard.write_text(&text);
            show_toast("Copied to clipboard!".to_string());
        }
    };

    let delete_message = move |idx: usize| {
        if is_streaming.get_untracked() {
            return;
        }
        let mut current_msgs = messages.get_untracked();
        if idx < current_msgs.len() {
            current_msgs.remove(idx);
            set_messages.set(current_msgs.clone());
            
            // Save conversation
            if let Some(convo_id) = current_conversation_id.get_untracked() {
                let mut convos = conversations.get_untracked();
                if let Some(convo) = convos.iter_mut().find(|c| c.id == convo_id) {
                    convo.messages = current_msgs;
                    convo.updated_at = js_sys::Date::now() as u64;
                    let convo_clone = convo.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                            conversation: convo_clone,
                        })
                        .unwrap();
                        invoke("save_conversation", args).await;
                    });
                }
            }
        }
    };

    let save_edited_message = move |idx: usize| {
        let mut current_msgs = messages.get_untracked();
        if let Some(msg) = current_msgs.get_mut(idx) {
            let new_text = editing_message_text.get_untracked();
            
            if let Some(version) = msg.versions.get_mut(msg.active_version) {
                let mut has_text = false;
                for part in &mut version.content {
                    if let ContentPart::Text { text } = part {
                        *text = new_text.clone();
                        has_text = true;
                        break;
                    }
                }
                if !has_text {
                    version.content.insert(0, ContentPart::Text { text: new_text.clone() });
                }
            }
            
            set_messages.set(current_msgs.clone());
            set_editing_message_idx.set(None);
            
            // Save conversation
            if let Some(convo_id) = current_conversation_id.get_untracked() {
                let mut convos = conversations.get_untracked();
                if let Some(convo) = convos.iter_mut().find(|c| c.id == convo_id) {
                    convo.messages = current_msgs;
                    convo.updated_at = js_sys::Date::now() as u64;
                    let convo_clone = convo.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                            conversation: convo_clone,
                        })
                        .unwrap();
                        invoke("save_conversation", args).await;
                    });
                }
            }
        }
    };

    let switch_version = move |idx: usize, next: bool| {
        let mut current_msgs = messages.get_untracked();
        if let Some(msg) = current_msgs.get_mut(idx) {
            let total_versions = msg.versions.len();
            if total_versions <= 1 {
                return;
            }
            if next {
                if msg.active_version + 1 < total_versions {
                    msg.active_version += 1;
                }
            } else {
                if msg.active_version > 0 {
                    msg.active_version -= 1;
                }
            }
            set_messages.set(current_msgs.clone());
            
            // Save conversation
            if let Some(convo_id) = current_conversation_id.get_untracked() {
                let mut convos = conversations.get_untracked();
                if let Some(convo) = convos.iter_mut().find(|c| c.id == convo_id) {
                    convo.messages = current_msgs;
                    convo.updated_at = js_sys::Date::now() as u64;
                    let convo_clone = convo.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                            conversation: convo_clone,
                        })
                        .unwrap();
                        invoke("save_conversation", args).await;
                    });
                }
            }
        }
    };

    let branch_conversation = move |idx: usize| {
        if is_streaming.get_untracked() {
            return;
        }
        let current_convo_id = current_conversation_id.get_untracked();
        if let Some(convo_id) = current_convo_id {
            let mut convos = conversations.get_untracked();
            if let Some(convo) = convos.iter().find(|c| c.id == convo_id) {
                let uuid = uuid::Uuid::new_v4().to_string();
                let branched_messages = convo.messages[..=idx].to_vec();
                
                let new_convo = ChatConversation {
                    id: uuid.clone(),
                    title: format!("Branch of {}", convo.title),
                    model: convo.model.clone(),
                    provider: convo.provider,
                    messages: branched_messages.clone(),
                    updated_at: js_sys::Date::now() as u64,
                    connection_id: convo.connection_id.clone(),
                };
                
                convos.insert(0, new_convo.clone());
                set_conversations.set(convos);
                select_conversation(uuid.clone());
                
                let new_convo_c = new_convo.clone();
                spawn_local(async move {
                    let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                        conversation: new_convo_c,
                    })
                    .unwrap();
                    invoke("save_conversation", args).await;
                });
            }
        }
    };

    let retry_last_message = move |_| {
        if is_streaming.get_untracked() {
            return;
        }
        let mut current_msgs = messages.get_untracked();
        if current_msgs.is_empty() {
            return;
        }
        
        let last_idx = current_msgs.len() - 1;
        if current_msgs[last_idx].role != MessageRole::Assistant {
            return;
        }
        
        let new_version = MessageVersion {
            content: vec![ContentPart::Text { text: String::new() }],
            ttft_ms: None,
            tokens_per_sec: None,
            total_tokens: None,
            stop_reason: None,
            reasoning_duration_ms: None,
        };
        current_msgs[last_idx].versions.push(new_version);
        let new_active = current_msgs[last_idx].versions.len() - 1;
        current_msgs[last_idx].active_version = new_active;
        
        set_messages.set(current_msgs.clone());
        set_is_streaming.set(true);
        
        let messages_history = current_msgs[..last_idx].to_vec();
        
        let active_id = match current_conversation_id.get_untracked() {
            Some(id) => id,
            None => return,
        };
        
        let provider = selected_provider.get_untracked();
        let model = selected_model.get_untracked();
        let temp = temperature.get_untracked();
        let conn_id = active_connection_id.get_untracked();
        
        let api_config = ApiConfig {
            provider,
            model,
            temperature: temp,
            max_tokens: None,
            connection_id: conn_id,
        };
        
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&SendMessageStreamArgs {
                conversation_id: active_id,
                config: api_config,
                messages: messages_history,
            })
            .unwrap();
            
            let promise = invoke_raw("send_message_stream", args);
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        });
    };

    let continue_last_message = move |_| {
        if is_streaming.get_untracked() {
            return;
        }
        let current_msgs = messages.get_untracked();
        if current_msgs.is_empty() {
            return;
        }
        
        let last_idx = current_msgs.len() - 1;
        if current_msgs[last_idx].role != MessageRole::Assistant {
            return;
        }
        
        set_is_streaming.set(true);
        
        let active_id = match current_conversation_id.get_untracked() {
            Some(id) => id,
            None => return,
        };
        
        let provider = selected_provider.get_untracked();
        let model = selected_model.get_untracked();
        let temp = temperature.get_untracked();
        let conn_id = active_connection_id.get_untracked();
        
        let api_config = ApiConfig {
            provider,
            model,
            temperature: temp,
            max_tokens: None,
            connection_id: conn_id,
        };
        
        let messages_history = current_msgs.clone();
        
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&SendMessageStreamArgs {
                conversation_id: active_id,
                config: api_config,
                messages: messages_history,
            })
            .unwrap();
            
            let promise = invoke_raw("send_message_stream", args);
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        });
    };

    // Send Message
    let send_message = move |ev: SubmitEvent| {
        ev.prevent_default();
        if is_streaming.get_untracked() {
            return;
        }

        let text = input_text.get_untracked();
        let img = attached_image.get_untracked();

        if text.is_empty() && img.is_none() {
            return;
        }

        let active_id = match current_conversation_id.get_untracked() {
            Some(id) => id,
            None => {
                let uuid = uuid::Uuid::new_v4().to_string();
                let provider = selected_provider.get_untracked();
                let model = selected_model.get_untracked();
                let title = if text.len() > 25 {
                    format!("{}...", &text[..22])
                } else if !text.is_empty() {
                    text.clone()
                } else {
                    format!("New Chat ({})", provider.to_string())
                };

                let new_convo = ChatConversation {
                    id: uuid.clone(),
                    title,
                    model,
                    provider,
                    messages: Vec::new(),
                    updated_at: js_sys::Date::now() as u64,
                    connection_id: active_connection_id.get_untracked(),
                };

                let mut current_convs = conversations.get_untracked();
                current_convs.insert(0, new_convo.clone());
                set_conversations.set(current_convs);

                set_current_conversation_id.set(Some(uuid.clone()));
                uuid
            }
        };

        let mut parts = Vec::new();
        if !text.is_empty() {
            parts.push(ContentPart::Text { text: text.clone() });
        }
        if let Some((mime, b64)) = img {
            parts.push(ContentPart::Image {
                mime_type: mime,
                base64: b64,
            });
        }

        let new_user_msg = ChatMessage {
            role: MessageRole::User,
            versions: vec![MessageVersion {
                content: parts,
                ttft_ms: None,
                tokens_per_sec: None,
                total_tokens: None,
                stop_reason: None,
                reasoning_duration_ms: None,
            }],
            active_version: 0,
        };

        let mut active_msgs = messages.get_untracked();
        active_msgs.push(new_user_msg);
        set_messages.set(active_msgs.clone());

        set_input_text.set(String::new());
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            if let Some(el) = doc.get_element_by_id("prompt-input-box") {
                if let Ok(textarea) = el.dyn_into::<web_sys::HtmlTextAreaElement>() {
                    let _ = web_sys::HtmlElement::style(&textarea).set_property("height", "auto");
                }
            }
        }
        set_attached_image.set(None);
        set_is_streaming.set(true);

        let provider = selected_provider.get_untracked();
        let model = selected_model.get_untracked();
        let temp = temperature.get_untracked();
        let conn_id = active_connection_id.get_untracked();

        let api_config = ApiConfig {
            provider,
            model,
            temperature: temp,
            max_tokens: None,
            connection_id: conn_id,
        };

        let messages_c = active_msgs.clone();
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&SendMessageStreamArgs {
                conversation_id: active_id.clone(),
                config: api_config,
                messages: messages_c,
            })
            .unwrap();

            let promise = invoke_raw("send_message_stream", args);
            let invoke_res = wasm_bindgen_futures::JsFuture::from(promise).await;

            let mut convos = conversations.get_untracked();
            if let Some(convo) = convos.iter_mut().find(|c| c.id == active_id) {
                convo.messages = active_msgs.clone();
                convo.updated_at = js_sys::Date::now() as u64;

                if convo.title.starts_with("New Chat") && !text.is_empty() {
                    convo.title = if text.len() > 25 {
                        format!("{}...", &text[..22])
                    } else {
                        text.clone()
                    };
                }

                let convo_clone = convo.clone();
                let save_promise = invoke_raw(
                    "save_conversation",
                    serde_wasm_bindgen::to_value(&SaveConversationArgs {
                        conversation: convo_clone,
                    })
                    .unwrap(),
                );
                let _ = wasm_bindgen_futures::JsFuture::from(save_promise).await;
            }
            set_conversations.set(convos);

            if let Err(err) = invoke_res {
                let err_str = err.as_string().unwrap_or_else(|| "Unknown connection error".to_string());
                set_is_streaming.set(false);
                let mut current_msgs = messages.get_untracked();
                current_msgs.push(ChatMessage::new_text(
                    MessageRole::Assistant,
                    format!("⚠️ Connection error: {}", err_str),
                ));
                set_messages.set(current_msgs);
            }
        });

        spawn_local(async move {
            scroll_chat_to_bottom();
        });
    };

    // Connection manager triggers
    let fetch_models_click = move |_| {
        let provider = new_conn_provider.get_untracked();
        let api_key = new_conn_api_key.get_untracked().trim().to_string();
        let base_url_str = new_conn_base_url.get_untracked().trim().to_string();

        if api_key.is_empty() {
            set_fetching_models_error.set(Some("API key is required to fetch models".to_string()));
            return;
        }

        let base_url = if provider == Provider::CustomOpenAICompliant
            || provider == Provider::OpenRouter
        {
            if base_url_str.is_empty() {
                None
            } else {
                Some(base_url_str)
            }
        } else {
            None
        };

        set_fetching_models_loading.set(true);
        set_fetching_models_error.set(None);

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&FetchModelsArgs {
                provider,
                api_key,
                base_url,
            })
            .unwrap();

            let promise = invoke_raw("fetch_models", args);
            let invoke_res = wasm_bindgen_futures::JsFuture::from(promise).await;
            set_fetching_models_loading.set(false);

            match invoke_res {
                Ok(val) => {
                    match serde_wasm_bindgen::from_value::<Vec<String>>(val) {
                        Ok(models) => {
                            if models.is_empty() {
                                set_fetching_models_error
                                    .set(Some("No models returned by this endpoint".to_string()));
                            } else {
                                set_new_conn_fetched_models.set(models);
                            }
                        }
                        Err(e) => {
                            set_fetching_models_error
                                .set(Some(format!("Tauri mapping failure: {:?}", e)));
                        }
                    }
                }
                Err(err) => {
                    let err_str = err.as_string().unwrap_or_else(|| "Unknown error".to_string());
                    set_fetching_models_error.set(Some(err_str));
                }
            }
        });
    };

    let toggle_model = move |model: String| {
        let mut selected = new_conn_enabled_models.get_untracked();
        if let Some(pos) = selected.iter().position(|m| m == &model) {
            selected.remove(pos);
        } else {
            selected.push(model.clone());
            let current_default = new_conn_default_model.get_untracked();
            if current_default.is_empty() {
                set_new_conn_default_model.set(model);
            }
        }
        set_new_conn_enabled_models.set(selected);
    };

    let select_all_models = move |_| {
        let models = new_conn_fetched_models.get_untracked();
        set_new_conn_enabled_models.set(models.clone());
        if !models.is_empty() {
            set_new_conn_default_model.set(models[0].clone());
        }
    };

    let deselect_all_models = move |_| {
        set_new_conn_enabled_models.set(Vec::new());
        set_new_conn_default_model.set(String::new());
    };

    // Open the connection form pre-filled with an existing connection's data
    let open_connection_for_edit = move |conn: Connection| {
        set_editing_connection_id.set(Some(conn.id.clone()));
        set_new_conn_name.set(conn.name.clone());
        set_new_conn_api_key.set(conn.api_key.clone());
        set_new_conn_base_url.set(conn.base_url.clone().unwrap_or_default());
        set_new_conn_provider.set(conn.provider);
        set_new_conn_enabled_models.set(conn.enabled_models.clone());
        set_new_conn_default_model.set(conn.default_model.clone());
        // Pre-populate fetched models with the current enabled list so user can see/modify them
        set_new_conn_fetched_models.set(conn.enabled_models.clone());
        set_new_conn_reasoning_configs.set(conn.reasoning_configs.clone());
        set_new_conn_search_query.set(String::new());
        set_fetching_models_error.set(None);
        set_show_add_connection.set(true);
    };

    let save_new_connection = move |_| {
        let name = new_conn_name.get_untracked().trim().to_string();
        let provider = new_conn_provider.get_untracked();
        let api_key = new_conn_api_key.get_untracked().trim().to_string();
        let base_url_str = new_conn_base_url.get_untracked().trim().to_string();
        let enabled_models = new_conn_enabled_models.get_untracked();
        let default_model = new_conn_default_model.get_untracked();
        let editing_id = editing_connection_id.get_untracked();

        if name.trim().is_empty() {
            set_fetching_models_error.set(Some("Connection name is required".to_string()));
            return;
        }
        if api_key.trim().is_empty() {
            set_fetching_models_error.set(Some("API key is required".to_string()));
            return;
        }
        if enabled_models.is_empty() {
            set_fetching_models_error.set(Some("Select at least one model".to_string()));
            return;
        }
        if default_model.is_empty() {
            set_fetching_models_error.set(Some("Choose a default model".to_string()));
            return;
        }

        let base_url = if provider == Provider::CustomOpenAICompliant || provider == Provider::OpenRouter {
            if base_url_str.is_empty() { None } else { Some(base_url_str) }
        } else {
            None
        };

        let mut current_conns = connections.get_untracked();
        let mut reasoning_configs = new_conn_reasoning_configs.get_untracked();
        reasoning_configs.retain(|rc| enabled_models.contains(&rc.model_id));

        if let Some(edit_id) = editing_id {
            // ── Edit mode: update the existing connection in-place ──
            if let Some(conn) = current_conns.iter_mut().find(|c| c.id == edit_id) {
                conn.name = name;
                conn.api_key = api_key;
                conn.provider = provider;
                conn.base_url = base_url;
                conn.enabled_models = enabled_models;
                conn.default_model = default_model.clone();
                conn.reasoning_configs = reasoning_configs;
            }
            set_connections.set(current_conns.clone());

            // Refresh the active connection selectors if it was the active one
            if active_connection_id.get_untracked() == Some(edit_id) {
                set_selected_provider.set(provider);
                set_selected_model.set(default_model);
            }
        } else {
            // ── Add mode: create a new connection ──
            let conn = Connection {
                id: uuid::Uuid::new_v4().to_string(),
                name,
                provider,
                api_key,
                base_url,
                enabled_models,
                default_model: default_model.clone(),
                reasoning_configs,
            };
            current_conns.push(conn.clone());
            set_connections.set(current_conns.clone());
            set_active_connection_id.set(Some(conn.id.clone()));
            set_selected_provider.set(conn.provider);
            set_selected_model.set(default_model);
        }

        // Reset form state
        set_show_add_connection.set(false);
        set_editing_connection_id.set(None);
        set_new_conn_name.set(String::new());
        set_new_conn_api_key.set(String::new());
        set_new_conn_base_url.set(String::new());
        set_new_conn_fetched_models.set(Vec::new());
        set_new_conn_search_query.set(String::new());
        set_new_conn_enabled_models.set(Vec::new());
        set_new_conn_default_model.set(String::new());
        set_new_conn_reasoning_configs.set(Vec::new());
        set_fetching_models_error.set(None);

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&SaveConnectionsArgs { connections: current_conns }).unwrap();
            invoke("save_connections", args).await;
        });
    };

    let delete_connection_click = move |id: String| {
        let mut current_conns = connections.get_untracked();
        current_conns.retain(|c| c.id != id);
        set_connections.set(current_conns.clone());

        let active_id = active_connection_id.get_untracked();
        if active_id == Some(id.clone()) {
            if !current_conns.is_empty() {
                let first_id = current_conns[0].id.clone();
                set_active_connection_id.set(Some(first_id.clone()));
                set_selected_provider.set(current_conns[0].provider);
                set_selected_model.set(current_conns[0].default_model.clone());
            } else {
                set_active_connection_id.set(None);
            }
        }

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&DeleteConnectionArgs { id }).unwrap();
            invoke("delete_connection", args).await;
        });
    };

    view! {
        <div
            class=move || format!(
                "flex h-screen w-screen overflow-hidden bg-theme-bg text-theme-text font-sans select-none theme-transition {}",
                app_theme.get().to_class()
            )
            on:dragover=handle_drag_over
            on:drop=handle_drop
        >
            // ─── SIDEBAR ───
            <aside class="flex flex-col w-72 bg-theme-panel border-r border-theme-border/60 shrink-0 theme-transition">
                // Sidebar Header
                <div class="p-4 border-b border-theme-border/60">
                    <button
                        on:click=create_new_chat
                        disabled=move || is_streaming.get()
                        class="w-full flex items-center justify-center gap-2 py-2.5 px-4 rounded-xl border border-theme-border bg-theme-bg/60 text-theme-text font-medium hover:bg-theme-bg hover:border-theme-accent/50 transition-all active:scale-[0.98] disabled:opacity-40 disabled:cursor-not-allowed theme-transition"
                    >
                        <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/>
                        </svg>
                        "New chat"
                    </button>
                </div>

                // Conversations List
                <div class="flex-1 overflow-y-auto px-2 py-3 space-y-1 scrollbar-thin">
                    {move || conversations.get().iter().map(|convo| {
                        let id = convo.id.clone();
                        let title = convo.title.clone();
                        let active = current_conversation_id.get() == Some(id.clone());
                        let is_renaming = editing_convo_id.get() == Some(id.clone());
                        let active_class = if active {
                            "bg-theme-bg text-theme-text font-semibold border-l-2 border-theme-accent"
                        } else {
                            "text-theme-muted hover:bg-theme-bg/40 hover:text-theme-text border-l-2 border-transparent"
                        };
                        let active_trash_class = if active { "text-theme-muted hover:text-red-400" } else { "text-theme-muted/70 hover:text-red-400" };
                        let delete_btn_class = format!("p-1 rounded hover:bg-theme-bg/60 transition-all {}", active_trash_class);
                        // Gradient fades toward the row background so text doesn't bleed into buttons
                        let fade_to = if active { "to-theme-bg" } else { "to-theme-panel" };
                        let fade_class = format!("absolute right-0 top-0 h-full w-8 bg-gradient-to-r from-transparent {} pointer-events-none opacity-0 group-hover:opacity-100 transition-opacity", fade_to);
                        // Buttons sit on a solid background matching the row bg
                        let btns_bg = if active { "bg-theme-bg" } else { "bg-theme-panel" };

                        let id_trash2 = id.clone();
                        let id_rename_btn = id.clone();
                        let id_rename_save = id.clone();
                        let id_rename_blur = id.clone();
                        let title_for_input = title.clone();

                        view! {
                            <div
                                on:click=move |_| {
                                    // Don't switch chat if we're in rename mode
                                    if editing_convo_id.get_untracked().is_none() {
                                        select_conversation(id.clone());
                                    }
                                }
                                class={format!("group flex items-center justify-between px-3 py-2.5 rounded-xl cursor-pointer transition-all theme-transition {}", active_class)}
                            >
                                // Title or inline rename input
                                <div class="relative flex-1 min-w-0 overflow-hidden">
                                    <Show
                                        when=move || is_renaming
                                        fallback=move || view! {
                                            <span class="text-sm truncate select-none block pr-1">{title.clone()}</span>
                                            // Gradient fade that appears on hover to separate text from buttons
                                            <div class=fade_class.clone()></div>
                                        }
                                    >
                                        <input
                                            type="text"
                                            class="w-full bg-theme-input border border-theme-accent/60 rounded-md px-1.5 py-0.5 text-sm text-theme-text outline-none focus:border-theme-accent"
                                            prop:value=move || editing_convo_title.get()
                                            on:input=move |ev| set_editing_convo_title.set(event_target_value(&ev))
                                            on:keydown={
                                                let id_s = id_rename_save.clone();
                                                move |ev: web_sys::KeyboardEvent| {
                                                    ev.stop_propagation();
                                                    match ev.key().as_str() {
                                                        "Enter" => rename_chat(id_s.clone(), editing_convo_title.get_untracked()),
                                                        "Escape" => { set_editing_convo_id.set(None); set_editing_convo_title.set(String::new()); }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                            on:blur={
                                                let id_b = id_rename_blur.clone();
                                                move |_| rename_chat(id_b.clone(), editing_convo_title.get_untracked())
                                            }
                                            on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()
                                        />
                                    </Show>
                                </div>

                                // Action buttons — solid background so they're always legible over text
                                <div
                                    class=format!("flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-all shrink-0 pl-1 {}", btns_bg)
                                    style=move || if is_renaming { "display:none" } else { "" }
                                >
                                    // Rename button
                                    <button
                                        on:click={
                                            let id_r = id_rename_btn.clone();
                                            let t = title_for_input.clone();
                                            move |ev: web_sys::MouseEvent| {
                                                ev.stop_propagation();
                                                set_editing_convo_id.set(Some(id_r.clone()));
                                                set_editing_convo_title.set(t.clone());
                                            }
                                        }
                                        class="p-1 rounded hover:bg-theme-bg text-theme-muted/70 hover:text-theme-muted transition-all"
                                        title="Rename chat"
                                    >
                                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                                        </svg>
                                    </button>
                                    // Delete button
                                    <button
                                        on:click=move |ev| delete_chat(id_trash2.clone(), ev)
                                        class=delete_btn_class.clone()
                                        title="Delete chat"
                                    >
                                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                                        </svg>
                                    </button>
                                </div>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>

                // Sidebar Footer (Connections settings trigger)
                <div class="p-4 border-t border-theme-border/60 bg-theme-panel">
                    <button
                        on:click=move |_| set_show_settings.set(true)
                        class="w-full flex items-center justify-between px-3 py-2 rounded-xl text-theme-muted hover:bg-theme-bg hover:text-theme-text transition-all theme-transition"
                    >
                        <div class="flex items-center gap-2.5">
                            <svg class="w-5 h-5 opacity-70" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/>
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                            </svg>
                            <span class="text-sm font-medium">"Connections & Keys"</span>
                        </div>
                        <span class="bg-theme-bg border border-theme-border/80 text-theme-accent font-mono text-[10px] py-0.5 px-2 rounded-full theme-transition">
                            {move || connections.get().len()}
                        </span>
                    </button>
                </div>
            </aside>

            // ─── MAIN CHAT AREA ───
            <main class="flex-1 flex flex-col min-w-0 bg-theme-bg h-full relative theme-transition">
                // Chat Header
                <header class="flex items-center justify-between h-16 border-b border-theme-border/60 px-6 shrink-0 bg-theme-bg/85 backdrop-blur-md theme-transition">
                    <div class="flex items-center gap-4">
                        // Active Connection Selector
                        <div class="flex items-center gap-2">
                            <span class="text-xs text-theme-muted font-semibold uppercase tracking-wider select-none">"Connection:"</span>
                            <select
                                on:change=on_connection_change
                                class="themed-select select-none"
                            >
                                {move || {
                                    let conns = connections.get();
                                    if conns.is_empty() {
                                        view! {
                                            <option value="">"No active connections"</option>
                                        }.into_any()
                                    } else {
                                        conns.into_iter().map(|c| {
                                            let id_val = c.id.clone();
                                            let id_clone = c.id.clone();
                                            let name = c.name.clone();
                                            view! {
                                                <option value=id_val selected={move || active_connection_id.get() == Some(id_clone.clone())}>{name}</option>
                                            }
                                        }).collect::<Vec<_>>().into_any()
                                    }
                                }}
                            </select>
                        </div>

                        // Model selector (Filtered based on active connection enabled models)
                        <div class="flex items-center gap-2">
                            <span class="text-xs text-theme-muted font-semibold uppercase tracking-wider select-none">"Model:"</span>
                            <select
                                on:change=on_model_change
                                class="themed-select select-none"
                            >
                                {move || {
                                    let active_id = active_connection_id.get();
                                    if let Some(conn_id) = active_id {
                                        if let Some(conn) = connections.get().into_iter().find(|c| c.id == conn_id) {
                                            return conn.enabled_models.into_iter().map(|m| {
                                                let m_val = m.clone();
                                                let m_clone1 = m.clone();
                                                let m_clone2 = m.clone();
                                                view! {
                                                    <option value=m_val selected={move || selected_model.get() == m_clone1.clone()}>{m_clone2}</option>
                                                }
                                            }).collect::<Vec<_>>().into_any();
                                        }
                                    }
                                    view! {
                                        <option value="">"Configure in Settings..."</option>
                                    }.into_any()
                                }}
                            </select>
                        </div>
                    </div>

                    // Temperature indicator & Theme selector
                    <div class="flex items-center gap-4">
                        <div class="flex items-center gap-2">
                            <span class="text-xs text-theme-muted font-medium font-mono select-none">
                                "Temp: " {move || format!("{:.1}", temperature.get())}
                            </span>
                            <input
                                type="range"
                                min="0.0"
                                max="1.5"
                                step="0.1"
                                prop:value=move || temperature.get()
                                on:input=move |ev| {
                                    if let Ok(val) = event_target_value(&ev).parse::<f32>() {
                                        set_temperature.set(val);
                                    }
                                }
                                class="w-16 accent-theme-accent h-1 rounded bg-theme-border/80 outline-none cursor-pointer theme-transition"
                            />
                        </div>

                        // Theme switcher segmented control (Light / Dark)
                        <div class="flex items-center gap-1 bg-theme-panel border border-theme-border/60 rounded-xl p-1 select-none theme-transition">
                            <button
                                type="button"
                                title="Light Mode"
                                class=move || format!("px-2.5 py-1 text-[11px] font-semibold rounded-lg transition-all {}",
                                    if app_theme.get() == AppTheme::Light {
                                        "bg-theme-bg text-theme-text shadow-sm border border-theme-border/20"
                                    } else {
                                        "text-theme-muted hover:text-theme-text"
                                    }
                                )
                                on:click=move |_| {
                                    set_app_theme.set(AppTheme::Light);
                                    save_theme(AppTheme::Light);
                                }
                            >
                                "Light"
                            </button>
                            <button
                                type="button"
                                title="Dark Mode"
                                class=move || format!("px-2.5 py-1 text-[11px] font-semibold rounded-lg transition-all {}",
                                    if app_theme.get() == AppTheme::Dark {
                                        "bg-theme-bg text-theme-text shadow-sm border border-theme-border/20"
                                    } else {
                                        "text-theme-muted hover:text-theme-text"
                                    }
                                )
                                on:click=move |_| {
                                    set_app_theme.set(AppTheme::Dark);
                                    save_theme(AppTheme::Dark);
                                }
                            >
                                "Dark"
                            </button>
                        </div>
                    </div>
                </header>

                // Message Feed
                <div
                    id="chat-messages-container"
                    class="flex-1 overflow-y-auto px-6 py-8 space-y-6 scrollbar-thin scroll-smooth"
                >
                    <Show
                        when=move || !messages.get().is_empty()
                        fallback=move || view! {
                            <div class="flex flex-col items-center justify-center h-full max-w-lg mx-auto text-center space-y-4 select-none transition-all theme-transition">
                                <div class="p-4 rounded-3xl bg-theme-panel border border-theme-border text-theme-accent">
                                    <svg class="w-10 h-10" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"/>
                                    </svg>
                                </div>
                                <h2 class="text-xl font-bold text-theme-text font-serif">"Ask anything"</h2>
                                <p class="text-sm text-theme-muted leading-relaxed">
                                    "Select a connection, write your query, or drop an image file directly into the editor below to execute multimodal VLM inference securely."
                                </p>
                            </div>
                        }
                    >
                        {move || messages.get().into_iter().enumerate().map(|(idx, msg)| {
                            let is_user = msg.role == MessageRole::User;
                            let is_editing = editing_message_idx.get() == Some(idx);
                            let is_last_msg = idx == messages.get().len() - 1;

                            let active_ver = msg.versions.get(msg.active_version);
                            let content_parts = active_ver.map(|v| v.content.clone()).unwrap_or_default();
                            let reasoning_duration = active_ver.and_then(|v| v.reasoning_duration_ms);

                            if is_user {
                                view! {
                                    <div class="w-full flex justify-end py-2 select-text">
                                        <div class="w-full max-w-[70ch] bg-theme-panel text-theme-text border border-theme-border/60 rounded-2xl p-4 shadow-sm transition-all theme-transition">
                                            <div class="flex items-center gap-2 mb-2 text-xs font-semibold text-theme-muted uppercase tracking-wider select-none font-sans justify-end">
                                                "You"
                                            </div>

                                            <div class="space-y-3 max-w-full overflow-hidden">
                                                <Show
                                                    when=move || is_editing
                                                    fallback=move || {
                                                        let parts = content_parts.clone();
                                                        view! {
                                                            <div class="space-y-3 max-w-full overflow-hidden prose max-w-none font-serif leading-relaxed text-theme-text">
                                                                {parts.iter().map(|part| match part {
                                                                    ContentPart::Text { text } => {
                                                                        render_message_content(text.clone()).into_any()
                                                                    }
                                                                    ContentPart::Image { mime_type, base64 } => {
                                                                        view! {
                                                                            <div class="rounded-lg overflow-hidden border border-theme-border max-w-sm mt-1">
                                                                                <img
                                                                                    src={format!("data:{};base64,{}", mime_type, base64)}
                                                                                    class="w-full object-cover max-h-60"
                                                                                    alt="Attached Image"
                                                                                />
                                                                            </div>
                                                                        }.into_any()
                                                                    }
                                                                }).collect::<Vec<_>>()}
                                                            </div>
                                                        }
                                                    }
                                                >
                                                    <div class="w-full flex flex-col space-y-2 mt-1 select-text">
                                                        <textarea
                                                            id="inline-edit-textarea"
                                                            class="w-full min-h-[80px] p-3 rounded-lg bg-theme-input border border-theme-border text-theme-text placeholder-theme-muted/70 focus:outline-none focus:border-theme-accent text-sm font-sans resize-none overflow-hidden theme-transition"
                                                            prop:value=editing_message_text
                                                            on:input=move |ev: web_sys::Event| {
                                                                let target = ev.target().unwrap().dyn_into::<web_sys::HtmlTextAreaElement>().unwrap();
                                                                set_editing_message_text.set(target.value());
                                                                let style = web_sys::HtmlElement::style(&target);
                                                                let _ = style.set_property("height", "auto");
                                                                let _ = style.set_property("height", &format!("{}px", target.scroll_height()));
                                                            }
                                                        />
                                                        <div class="flex items-center gap-2 justify-end">
                                                            <button
                                                                type="button"
                                                                class="px-3 py-1.5 rounded-lg text-xs font-semibold bg-theme-panel hover:bg-theme-border/80 text-theme-text border border-theme-border/60 transition-colors theme-transition"
                                                                on:click=move |_| set_editing_message_idx.set(None)
                                                            >
                                                                "Cancel"
                                                            </button>
                                                            <button
                                                                type="button"
                                                                class="px-3 py-1.5 rounded-lg text-xs font-semibold bg-theme-accent text-theme-bg hover:opacity-95 transition-all theme-transition"
                                                                on:click=move |_| save_edited_message(idx)
                                                            >
                                                                "Save"
                                                            </button>
                                                        </div>
                                                    </div>
                                                </Show>
                                            </div>

                                            // User message controls: copy + edit (shown when not editing)
                                            <Show when=move || !is_editing>
                                                <div class="flex items-center gap-1 justify-end mt-2 select-none">
                                                    // Copy
                                                    <button
                                                        type="button"
                                                        title="Copy message"
                                                        class="p-1.5 rounded-lg hover:bg-theme-bg hover:text-theme-text text-theme-muted transition-colors theme-transition"
                                                        on:click={
                                                            let text = msg.get_text();
                                                            move |_| copy_message_text(text.clone())
                                                        }
                                                    >
                                                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                                                        </svg>
                                                    </button>
                                                    // Edit
                                                    <button
                                                        type="button"
                                                        title="Edit message"
                                                        class="p-1.5 rounded-lg hover:bg-theme-bg hover:text-theme-text text-theme-muted transition-colors disabled:opacity-30 theme-transition"
                                                        disabled=move || is_streaming.get()
                                                        on:click={
                                                            let text = msg.get_text();
                                                            move |_| {
                                                                set_editing_message_idx.set(Some(idx));
                                                                set_editing_message_text.set(text.clone());
                                                            }
                                                        }
                                                    >
                                                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                                                        </svg>
                                                    </button>
                                                    // Delete
                                                    <button
                                                        type="button"
                                                        title="Delete message"
                                                        class="p-1.5 rounded-lg hover:bg-theme-bg hover:text-red-400 text-theme-muted transition-colors disabled:opacity-30 theme-transition"
                                                        disabled=move || is_streaming.get()
                                                        on:click=move |_| delete_message(idx)
                                                    >
                                                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                                        </svg>
                                                    </button>
                                                </div>
                                            </Show>
                                        </div>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="w-full py-6 transition-all theme-transition select-text">
                                        <div class="flex items-center gap-2 mb-3 text-xs font-semibold text-theme-muted uppercase tracking-wider select-none font-sans">
                                            "AI"
                                        </div>

                                        <div class="prose max-w-none font-serif leading-relaxed text-theme-text overflow-hidden">
                                            <Show
                                                when=move || is_editing
                                                fallback=move || {
                                                    let parts = content_parts.clone();
                                                    view! {
                                                        <div class="space-y-4 max-w-full overflow-hidden">
                                                            {parts.iter().map(|part| match part {
                                                                ContentPart::Text { text } => {
                                                                    let (thinking_opt, remaining) = parse_thinking_content(text);
                                                                    let is_thinking_active = thinking_opt.is_some() && !text.contains("</think>");
                                                                    let thinking_opt_for_show = thinking_opt.clone();
                                                                    let thinking_opt_for_block = thinking_opt.clone();
                                                                    
                                                                    view! {
                                                                        <div class="flex flex-col w-full">
                                                                            <Show 
                                                                                when=move || thinking_opt_for_show.is_some()
                                                                                fallback=move || view! {}
                                                                            >
                                                                                <ThinkingBlock 
                                                                                    thinking=thinking_opt_for_block.clone().unwrap_or_default() 
                                                                                    is_thinking=is_thinking_active 
                                                                                    duration_ms=reasoning_duration
                                                                                />
                                                                            </Show>
                                                                            {render_message_content(remaining.clone())}
                                                                        </div>
                                                                    }.into_any()
                                                                }
                                                                ContentPart::Image { mime_type, base64 } => {
                                                                    view! {
                                                                        <div class="rounded-lg overflow-hidden border border-theme-border max-w-sm mt-1">
                                                                            <img
                                                                                src={format!("data:{};base64,{}", mime_type, base64)}
                                                                                class="w-full object-cover max-h-60"
                                                                                alt="Attached Image"
                                                                            />
                                                                        </div>
                                                                    }.into_any()
                                                                }
                                                            }).collect::<Vec<_>>()}
                                                        </div>
                                                    }
                                                }
                                            >
                                                <div class="w-full flex flex-col space-y-2 mt-1 select-text">
                                                    <textarea
                                                        id="inline-edit-textarea"
                                                        class="w-full min-h-[80px] p-3 rounded-lg bg-theme-input border border-theme-border text-theme-text placeholder-theme-muted/70 focus:outline-none focus:border-theme-accent text-sm font-sans resize-none overflow-hidden theme-transition"
                                                        prop:value=editing_message_text
                                                        on:input=move |ev: web_sys::Event| {
                                                            let target = ev.target().unwrap().dyn_into::<web_sys::HtmlTextAreaElement>().unwrap();
                                                            set_editing_message_text.set(target.value());
                                                            let style = web_sys::HtmlElement::style(&target);
                                                            let _ = style.set_property("height", "auto");
                                                            let _ = style.set_property("height", &format!("{}px", target.scroll_height()));
                                                        }
                                                    />
                                                    <div class="flex items-center gap-2 justify-end">
                                                        <button
                                                            type="button"
                                                            class="px-3 py-1.5 rounded-lg text-xs font-semibold bg-theme-panel hover:bg-theme-border/80 text-theme-text border border-theme-border/60 transition-colors theme-transition"
                                                            on:click=move |_| set_editing_message_idx.set(None)
                                                        >
                                                            "Cancel"
                                                        </button>
                                                        <button
                                                            type="button"
                                                            class="px-3 py-1.5 rounded-lg text-xs font-semibold bg-theme-accent text-theme-bg hover:opacity-95 transition-all theme-transition"
                                                            on:click=move |_| save_edited_message(idx)
                                                        >
                                                            "Save"
                                                        </button>
                                                    </div>
                                                </div>
                                            </Show>
                                        </div>

                                        <Show when=move || !is_editing>
                                            {
                                                let msg = msg.clone();
                                                let active_ver = msg.versions.get(msg.active_version).cloned();
                                                let total_versions = msg.versions.len();
                                                let active_version_idx = msg.active_version;
                                                
                                                let show_stats = active_ver.as_ref().map(|v| v.ttft_ms.is_some() || v.tokens_per_sec.is_some() || v.total_tokens.is_some() || v.stop_reason.is_some()).unwrap_or(false);
                                                
                                                view! {
                                                    <div class="mt-4 flex flex-col gap-1.5 font-sans">
                                                        <Show when=move || show_stats>
                                                            {
                                                                let ver = active_ver.clone().unwrap();
                                                                let stop_reason_for_cond = ver.stop_reason.clone();
                                                                let stop_reason_for_text = ver.stop_reason.clone();
                                                                view! {
                                                                    <div class="flex flex-wrap items-center gap-4 mt-1.5 text-[10px] font-mono text-theme-muted select-none">
                                                                        <Show when=move || ver.tokens_per_sec.is_some()>
                                                                            <div class="flex items-center gap-1">
                                                                                <svg class="w-3 h-3 text-theme-muted" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 6V12h6a6 6 0 10-6-6z" />
                                                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 2a10 10 0 1010 10A10 10 0 0012 2zm0 18a8 8 0 118-8 8 8 0 01-8 8z" />
                                                                                </svg>
                                                                                <span>{format!("{:.1} t/s", ver.tokens_per_sec.unwrap_or(0.0))}</span>
                                                                            </div>
                                                                        </Show>

                                                                        <Show when=move || ver.total_tokens.is_some()>
                                                                            <div class="flex items-center gap-1">
                                                                                <svg class="w-3 h-3 text-theme-muted" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2H5a2 2 0 00-2 2v2m14-4V5a2 2 0 00-2-2H5a2 2 0 00-2 2v2" />
                                                                                </svg>
                                                                                <span>{format!("{} tokens", ver.total_tokens.unwrap_or(0))}</span>
                                                                            </div>
                                                                        </Show>

                                                                        <Show when=move || ver.ttft_ms.is_some()>
                                                                            <div class="flex items-center gap-1">
                                                                                <svg class="w-3 h-3 text-theme-muted" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                                                                                </svg>
                                                                                <span>{format!("{:.2}s TTFT", (ver.ttft_ms.unwrap_or(0) as f32) / 1000.0)}</span>
                                                                            </div>
                                                                        </Show>

                                                                        <Show when=move || stop_reason_for_cond.is_some()>
                                                                            <div class="flex items-center gap-1">
                                                                                <span>{format!("Stop reason: {}", stop_reason_for_text.clone().unwrap_or_default())}</span>
                                                                            </div>
                                                                        </Show>
                                                                    </div>
                                                                }
                                                            }
                                                        </Show>

                                                        <div class="flex items-center justify-between mt-1 text-theme-muted text-xs select-none">
                                                            <Show when=move || is_last_msg && (total_versions > 1)>
                                                                <div class="flex items-center gap-1 bg-theme-panel rounded-lg p-0.5 border border-theme-border/60">
                                                                    <button
                                                                        type="button"
                                                                        class="p-1 rounded hover:bg-theme-border/60 text-theme-muted hover:text-theme-text disabled:opacity-30 disabled:hover:bg-transparent transition-all"
                                                                        disabled=move || active_version_idx == 0
                                                                        on:click=move |_| switch_version(idx, false)
                                                                    >
                                                                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7" />
                                                                        </svg>
                                                                    </button>
                                                                    <span class="text-[10px] font-mono font-semibold px-1 text-theme-text">
                                                                        {format!("{} / {}", active_version_idx + 1, total_versions)}
                                                                    </span>
                                                                    <button
                                                                        type="button"
                                                                        class="p-1 rounded hover:bg-theme-border/60 text-theme-muted hover:text-theme-text disabled:opacity-30 disabled:hover:bg-transparent transition-all"
                                                                        disabled=move || active_version_idx + 1 == total_versions
                                                                        on:click=move |_| switch_version(idx, true)
                                                                    >
                                                                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
                                                                        </svg>
                                                                    </button>
                                                                </div>
                                                            </Show>
                                                            <Show when=move || !(is_last_msg && total_versions > 1)>
                                                                <div></div>
                                                            </Show>

                                                            <div class="flex items-center gap-1 select-none">
                                                                <Show when=move || is_last_msg>
                                                                    <button
                                                                        type="button"
                                                                        title="Regenerate response"
                                                                        class="p-1.5 rounded-lg hover:bg-theme-panel hover:text-theme-text text-theme-muted transition-colors disabled:opacity-30 theme-transition"
                                                                        disabled=move || is_streaming.get()
                                                                        on:click=move |_| retry_last_message(())
                                                                    >
                                                                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0l3.181 3.183a8.25 8.25 0 0013.803-3.7M4.031 9.865a8.25 8.25 0 0113.803-3.7l3.181 3.182m0-4.991v4.99" />
                                                                        </svg>
                                                                    </button>
                                                                </Show>

                                                                <Show when=move || is_last_msg>
                                                                    <button
                                                                        type="button"
                                                                        title="Continue generating"
                                                                        class="p-1.5 rounded-lg hover:bg-theme-panel hover:text-theme-text text-theme-muted transition-colors disabled:opacity-30 theme-transition"
                                                                        disabled=move || is_streaming.get()
                                                                        on:click=move |_| continue_last_message(())
                                                                    >
                                                                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M14 5l7 7m0 0l-7 7m7-7H3" />
                                                                        </svg>
                                                                    </button>
                                                                </Show>

                                                                <button
                                                                    type="button"
                                                                    title="Branch conversation from here"
                                                                    class="p-1.5 rounded-lg hover:bg-theme-panel hover:text-theme-text text-theme-muted transition-colors disabled:opacity-30 theme-transition"
                                                                    disabled=move || is_streaming.get()
                                                                    on:click=move |_| branch_conversation(idx)
                                                                >
                                                                    <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                        <path stroke-linecap="round" stroke-linejoin="round" d="M8 7a3 3 0 100-6 3 3 0 000 6zM8 17a3 3 0 100 6 3 3 0 000-6zM18 12a3 3 0 100-6 3 3 0 000 6zM8 7v10M8 12h7" />
                                                                    </svg>
                                                                </button>

                                                                <button
                                                                    type="button"
                                                                    title="Copy message"
                                                                    class="p-1.5 rounded-lg hover:bg-theme-panel hover:text-theme-text text-theme-muted transition-colors theme-transition"
                                                                    on:click={
                                                                        let text = msg.get_text();
                                                                        move |_| copy_message_text(text.clone())
                                                                    }
                                                                >
                                                                    <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                        <path stroke-linecap="round" stroke-linejoin="round" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                                                                    </svg>
                                                                </button>

                                                                <button
                                                                    type="button"
                                                                    title="Edit response"
                                                                    class="p-1.5 rounded-lg hover:bg-theme-panel hover:text-theme-text text-theme-muted transition-colors disabled:opacity-30 theme-transition"
                                                                    disabled=move || is_streaming.get()
                                                                    on:click={
                                                                        let text = msg.get_text();
                                                                        move |_| {
                                                                            set_editing_message_idx.set(Some(idx));
                                                                            set_editing_message_text.set(text.clone());
                                                                        }
                                                                    }
                                                                >
                                                                    <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                        <path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                                                                    </svg>
                                                                </button>

                                                                <button
                                                                    type="button"
                                                                    title="Delete response"
                                                                    class="p-1.5 rounded-lg hover:bg-theme-panel hover:text-red-400 text-theme-muted transition-colors disabled:opacity-30 theme-transition"
                                                                    disabled=move || is_streaming.get()
                                                                    on:click=move |_| delete_message(idx)
                                                                >
                                                                    <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                        <path stroke-linecap="round" stroke-linejoin="round" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                                                    </svg>
                                                                </button>
                                                            </div>
                                                        </div>
                                                    </div>
                                                }
                                            }
                                        </Show>
                                    </div>
                                }.into_any()
                            }
                        }).collect::<Vec<_>>()}
                    </Show>

                    // Loader when thinking
                    <Show
                        when=move || is_streaming.get()
                        fallback=move || view! {}
                    >
                        <div class="w-full py-6 transition-all theme-transition">
                            <div class="flex items-center gap-2 mb-2 text-xs font-semibold text-theme-muted uppercase tracking-wider select-none font-sans">
                                "AI is thinking..."
                            </div>
                            <div class="flex space-x-1.5 py-2.5">
                                <div class="w-2 h-2 bg-theme-accent rounded-full animate-bounce" style="animation-delay: 0ms"></div>
                                <div class="w-2 h-2 bg-theme-accent rounded-full animate-bounce" style="animation-delay: 150ms"></div>
                                <div class="w-2 h-2 bg-theme-accent rounded-full animate-bounce" style="animation-delay: 300ms"></div>
                            </div>
                        </div>
                    </Show>
                </div>

                // Bottom Prompt Box / Input
                <footer class="p-6 border-t border-theme-border/60 bg-theme-bg shrink-0 theme-transition">
                    <form on:submit=send_message class="max-w-4xl mx-auto flex flex-col gap-3 bg-theme-panel border border-theme-border/80 rounded-2xl p-3 shadow-sm relative theme-transition">
                        // Attached Image Thumbnail Preview
                        <Show
                            when=move || attached_image.get().is_some()
                            fallback=move || view! {}
                        >
                            <div class="relative w-20 h-20 rounded-xl overflow-hidden border border-theme-border group">
                                <img
                                    src={move || {
                                        if let Some((mime, b64)) = attached_image.get() {
                                            format!("data:{};base64,{}", mime, b64)
                                        } else {
                                            "".to_string()
                                        }
                                    }}
                                    class="w-full h-full object-cover"
                                    alt="Thumbnail"
                                />
                                <button
                                    type="button"
                                    on:click=move |_| set_attached_image.set(None)
                                    class="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-all text-white rounded-xl"
                                >
                                    <svg class="w-5 h-5 hover:text-red-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                                    </svg>
                                </button>
                            </div>
                        </Show>

                        // Input control
                        <div class="flex items-end gap-3">
                            <div class="flex shrink-0">
                                <input
                                    type="file"
                                    id="image-upload-el"
                                    accept="image/*"
                                    class="hidden"
                                    on:change=on_file_change
                                />
                                <label
                                    for="image-upload-el"
                                    class="flex items-center justify-center p-2.5 rounded-xl border border-theme-border/80 bg-theme-bg hover:bg-theme-border/40 text-theme-muted hover:text-theme-text transition-all cursor-pointer shadow shadow-black/10 active:scale-[0.96] theme-transition"
                                >
                                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"/>
                                    </svg>
                                </label>
                            </div>

                            <textarea
                                id="prompt-input-box"
                                placeholder={move || if is_streaming.get() { "AI is generating..." } else { "Type your prompt or drag/drop an image..." }}
                                prop:value=move || input_text.get()
                                on:input=update_input
                                disabled=move || is_streaming.get()
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    if ev.key() == "Enter" && !ev.shift_key() {
                                        ev.prevent_default();
                                        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                            if let Some(btn) = doc.get_element_by_id("send-btn-el") {
                                                btn.dyn_into::<web_sys::HtmlButtonElement>().unwrap().click();
                                            }
                                        }
                                    }
                                }
                                rows="1"
                                class="flex-1 min-h-[42px] max-h-48 resize-none bg-transparent border-0 py-2.5 text-sm text-theme-text placeholder-theme-muted/70 outline-none scrollbar-none"
                            ></textarea>

                            <button
                                type="submit"
                                id="send-btn-el"
                                disabled=move || is_streaming.get() || (input_text.get().trim().is_empty() && attached_image.get().is_none())
                                class="shrink-0 flex items-center justify-center p-2.5 rounded-xl bg-theme-accent text-theme-bg hover:opacity-95 transition-all shadow-md active:scale-[0.96] disabled:opacity-35 disabled:cursor-not-allowed theme-transition"
                            >
                                <svg class="w-5 h-5 rotate-90" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8"/>
                                </svg>
                            </button>
                        </div>
                    </form>
                </footer>
            </main>

            // ─── CONNECTIONS SETTINGS MODAL ───
            <Show
                when=move || show_settings.get()
                fallback=move || view! {}
            >
                <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm animate-fade-in p-4">
                    <div class="w-full max-w-2xl bg-theme-panel border border-theme-border/60 rounded-3xl shadow-2xl p-6 relative overflow-hidden select-none max-h-[85vh] flex flex-col theme-transition">
                        // Modal Header
                        <div class="flex justify-between items-center pb-4 border-b border-theme-border/60 shrink-0">
                            <div>
                                <h3 class="text-lg font-bold text-theme-text">"Connection Manager"</h3>
                                <p class="text-xs text-theme-muted mt-1">"Manage your API endpoints and secure keychain credentials."</p>
                            </div>
                            <button
                                on:click=move |_| {
                                    set_show_settings.set(false);
                                    set_show_add_connection.set(false);
                                    set_editing_connection_id.set(None);
                                    set_new_conn_fetched_models.set(Vec::new());
                                    set_new_conn_enabled_models.set(Vec::new());
                                    set_new_conn_default_model.set(String::new());
                                    set_fetching_models_error.set(None);
                                }
                                class="text-theme-muted hover:text-theme-text p-1.5 rounded-lg hover:bg-theme-bg transition-all"
                            >
                                <svg class="w-5.5 h-5.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                                </svg>
                            </button>
                        </div>

                        // Modal Scrollable Area
                        <div class="flex-1 overflow-y-auto py-5 space-y-6">
                            // Section: Add Connection Form
                            <Show
                                when=move || show_add_connection.get()
                                fallback=move || view! {
                                    // Connection list view
                                    <div class="space-y-4">
                                        <div class="flex justify-between items-center">
                                            <h4 class="text-sm font-semibold text-theme-text">"Active Connections"</h4>
                                            <button
                                                on:click=move |_| set_show_add_connection.set(true)
                                                class="flex items-center gap-1.5 py-1.5 px-3 rounded-lg bg-theme-accent/20 hover:bg-theme-accent/30 border border-theme-accent/40 text-theme-text text-xs font-semibold transition-all active:scale-[0.97]"
                                            >
                                                <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/>
                                                </svg>
                                                "Add Connection"
                                            </button>
                                        </div>

                                        {move || {
                                            let conns = connections.get();
                                            if conns.is_empty() {
                                                view! {
                                                    <div class="flex flex-col items-center justify-center p-8 rounded-2xl border border-dashed border-theme-border text-theme-muted">
                                                        <p class="text-sm">"No API connections configured yet."</p>
                                                        <p class="text-xs mt-1">"Click 'Add Connection' to configure OpenAI, Gemini, Claude, or OpenRouter."</p>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                                                        {conns.into_iter().map(|conn| {
                                                            let id = conn.id.clone();
                                                            let name = conn.name.clone();
                                                            let provider_name = conn.provider.to_string();
                                                            let default_model = conn.default_model.clone();
                                                            let enabled_count = conn.enabled_models.len();

                                                            view! {
                                                                <div class="p-4 bg-theme-bg/40 border border-theme-border/60 rounded-2xl flex items-center justify-between group hover:border-theme-accent/40 transition-all">
                                                                    <div class="min-w-0 pr-2 flex-1">
                                                                        <div class="flex items-center gap-2">
                                                                            <span class="text-sm font-bold text-theme-text truncate">{name}</span>
                                                                            <span class="text-[9px] font-semibold bg-theme-border/80 text-theme-accent py-0.5 px-1.5 rounded-full">{provider_name}</span>
                                                                        </div>
                                                                        <p class="text-xs text-theme-muted mt-1 truncate">"Default: " <span class="font-mono text-theme-text/80">{default_model}</span></p>
                                                                        <p class="text-[10px] text-theme-muted/60 mt-0.5">{enabled_count} " models configured"</p>
                                                                    </div>
                                                                    // Edit + Delete buttons
                                                                    <div class="flex items-center gap-1 shrink-0">
                                                                        <button
                                                                            on:click={
                                                                                let conn_clone = conn.clone();
                                                                                move |_| open_connection_for_edit(conn_clone.clone())
                                                                            }
                                                                            title="Edit connection"
                                                                            class="p-2 rounded-xl text-theme-muted hover:text-theme-text hover:bg-theme-bg transition-all"
                                                                        >
                                                                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                                <path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                                                                            </svg>
                                                                        </button>
                                                                        <button
                                                                            on:click=move |_| delete_connection_click(id.clone())
                                                                            title="Delete connection"
                                                                            class="p-2 rounded-xl text-theme-muted hover:text-red-400 hover:bg-theme-bg transition-all"
                                                                        >
                                                                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                                                                            </svg>
                                                                        </button>
                                                                    </div>
                                                                </div>
                                                            }
                                                        }).collect::<Vec<_>>()}
                                                    </div>
                                                }.into_any()
                                            }
                                        }}
                                    </div>
                                }.into_any()
                            >
                                <div class="p-5 bg-theme-bg/40 border border-theme-border/60 rounded-2xl space-y-4">
                                    <h4 class="text-sm font-bold text-theme-text">
                                        {move || if editing_connection_id.get().is_some() { "Edit Connection" } else { "Configure Connection" }}
                                    </h4>

                                    // Fields Grid
                                    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
                                        // Provider Selector
                                        <div class="space-y-1.5">
                                            <label class="text-xs font-semibold text-theme-muted">"Provider"</label>
                                            <select
                                                on:change=move |ev| {
                                                    let val = event_target_value(&ev);
                                                    let prov = match val.as_str() {
                                                        "Claude" => Provider::Claude,
                                                        "Gemini" => Provider::Gemini,
                                                        "OpenRouter" => Provider::OpenRouter,
                                                        "CustomOpenAICompliant" => Provider::CustomOpenAICompliant,
                                                        _ => Provider::OpenAI,
                                                    };
                                                    set_new_conn_provider.set(prov);
                                                    set_new_conn_fetched_models.set(Vec::new());
                                                    set_new_conn_enabled_models.set(Vec::new());
                                                    set_new_conn_default_model.set(String::new());
                                                }
                                                class="themed-select-lg"
                                            >
                                                <option value="OpenAI">"OpenAI"</option>
                                                <option value="Claude">"Claude (Anthropic)"</option>
                                                <option value="Gemini">"Google Gemini"</option>
                                                <option value="OpenRouter">"OpenRouter"</option>
                                                <option value="CustomOpenAICompliant">"Custom OpenAI-Compliant"</option>
                                            </select>
                                        </div>

                                        // Connection Name
                                        <div class="space-y-1.5">
                                            <label class="text-xs font-semibold text-theme-muted">"Connection Name"</label>
                                            <input
                                                type="text"
                                                placeholder="e.g. Work OpenAI"
                                                prop:value=move || new_conn_name.get()
                                                on:input=move |ev| set_new_conn_name.set(event_target_value(&ev))
                                                class="w-full bg-theme-input border border-theme-border rounded-xl px-3 py-2.5 text-sm text-theme-text outline-none focus:border-theme-accent focus:ring-1 focus:ring-theme-accent/20 placeholder-theme-muted/50 transition-all"
                                            />
                                        </div>

                                        // API Key
                                        <div class="space-y-1.5 col-span-1 md:col-span-2">
                                            <label class="text-xs font-semibold text-theme-muted">"API Key"</label>
                                            <input
                                                type="password"
                                                placeholder="Enter credential..."
                                                prop:value=move || new_conn_api_key.get()
                                                on:input=move |ev| set_new_conn_api_key.set(event_target_value(&ev))
                                                class="w-full bg-theme-input border border-theme-border rounded-xl px-3 py-2.5 text-sm text-theme-text outline-none focus:border-theme-accent focus:ring-1 focus:ring-theme-accent/20 placeholder-theme-muted/50 font-mono transition-all"
                                            />
                                        </div>

                                        // Custom Base URL (Visible for Custom and OpenRouter)
                                        <Show
                                            when=move || new_conn_provider.get() == Provider::CustomOpenAICompliant || new_conn_provider.get() == Provider::OpenRouter
                                            fallback=move || view! {}
                                        >
                                            <div class="space-y-1.5 col-span-1 md:col-span-2">
                                                <label class="text-xs font-semibold text-theme-muted">"Base URL"</label>
                                                <input
                                                    type="text"
                                                    placeholder={move || if new_conn_provider.get() == Provider::OpenRouter { "https://openrouter.ai/api" } else { "https://my-local-server:port" }}
                                                    prop:value=move || new_conn_base_url.get()
                                                    on:input=move |ev| set_new_conn_base_url.set(event_target_value(&ev))
                                                    class="w-full bg-theme-input border border-theme-border rounded-xl px-3 py-2.5 text-sm text-theme-text outline-none focus:border-theme-accent placeholder-theme-muted/50 font-mono transition-all"
                                                />
                                            </div>
                                        </Show>
                                    </div>

                                    // Action to Fetch models list
                                    <div class="flex items-center justify-between pt-2">
                                        <span class="text-xs text-theme-muted leading-relaxed">
                                            "After entering credentials, fetch the list of available models to build your selector configuration."
                                        </span>
                                        <button
                                            type="button"
                                            on:click=fetch_models_click
                                            disabled=move || fetching_models_loading.get()
                                            class="flex items-center gap-1.5 py-2 px-4 rounded-xl bg-theme-bg border border-theme-border text-theme-text hover:border-theme-accent/50 transition-all active:scale-[0.98] shrink-0 disabled:opacity-40"
                                        >
                                            <Show
                                                when=move || fetching_models_loading.get()
                                                fallback=move || view! { "Fetch Models" }
                                            >
                                                <svg class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" fill="none" viewBox="0 0 24 24">
                                                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                                                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                                                </svg>
                                                "Fetching..."
                                            </Show>
                                        </button>
                                    </div>

                                    // Fetching errors alerts
                                    <Show
                                        when=move || fetching_models_error.get().is_some()
                                        fallback=move || view! {}
                                    >
                                        <div class="p-3 bg-red-950/40 border border-red-900/60 rounded-xl text-xs text-red-300 flex items-center gap-2">
                                            <svg class="w-4 h-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"/>
                                            </svg>
                                            <span class="break-words">{move || fetching_models_error.get().unwrap_or_default()}</span>
                                        </div>
                                    </Show>

                                    // Checklist of models loaded
                                    <Show
                                        when=move || !new_conn_fetched_models.get().is_empty()
                                        fallback=move || view! {}
                                    >
                                        <div class="pt-4 border-t border-theme-border/60 space-y-3">
                                            <div class="flex items-center justify-between">
                                                <h5 class="text-xs font-semibold text-theme-muted uppercase tracking-wider">"Select Enabled Models"</h5>
                                                <div class="flex gap-2">
                                                    <button type="button" on:click=select_all_models class="text-[10px] text-theme-accent hover:underline">"Select All"</button>
                                                    <span class="text-theme-border text-[10px]">"|"</span>
                                                    <button type="button" on:click=deselect_all_models class="text-[10px] text-theme-accent hover:underline">"Deselect All"</button>
                                                </div>
                                            </div>

                                            // Search Bar for models filtering
                                            <input
                                                type="text"
                                                placeholder="Search models filter..."
                                                prop:value=move || new_conn_search_query.get()
                                                on:input=move |ev| set_new_conn_search_query.set(event_target_value(&ev))
                                                class="w-full bg-theme-input border border-theme-border rounded-xl px-3 py-2 text-xs text-theme-text outline-none focus:border-theme-accent placeholder-theme-muted/40 transition-all"
                                            />

                                            // Scrollable checklist list
                                            <div class="h-44 overflow-y-auto border border-theme-border/60 rounded-xl p-2.5 bg-theme-bg/30 space-y-1.5 scrollbar-thin">
                                                {move || {
                                                    let selected_list = new_conn_enabled_models.get();
                                                    filtered_fetched_models.get().into_iter().map(|model| {
                                                        let checked = selected_list.contains(&model);
                                                        let m_clone = model.clone();
                                                        let m_click = model.clone();
                                                        view! {
                                                            <div
                                                                on:click=move |_| toggle_model(m_click.clone())
                                                                class="flex items-center gap-2.5 px-2 py-1.5 rounded-lg hover:bg-theme-border/20 cursor-pointer text-xs"
                                                            >
                                                                <input
                                                                    type="checkbox"
                                                                    prop:checked=checked
                                                                    class="accent-theme-accent rounded cursor-pointer shrink-0"
                                                                    on:click=move |ev| ev.prevent_default()
                                                                />
                                                                <span class="font-mono text-theme-text/80 select-none break-all">{m_clone}</span>
                                                            </div>
                                                        }
                                                    }).collect::<Vec<_>>()
                                                }}
                                            </div>

                                            // Default Model Selector
                                            <Show
                                                when=move || !new_conn_enabled_models.get().is_empty()
                                                fallback=move || view! {}
                                            >
                                                <div class="space-y-1.5 pt-2">
                                                    <label class="text-xs font-semibold text-theme-muted">"Default Model"</label>
                                                    <select
                                                        on:change=move |ev| set_new_conn_default_model.set(event_target_value(&ev))
                                                        class="themed-select-lg"
                                                    >
                                                        {move || {
                                                            new_conn_enabled_models.get().into_iter().map(|m| {
                                                                let m_val = m.clone();
                                                                let m_clone1 = m.clone();
                                                                let m_clone2 = m.clone();
                                                                view! {
                                                                    <option value=m_val selected={move || new_conn_default_model.get() == m_clone1.clone()}>{m_clone2}</option>
                                                                }
                                                            }).collect::<Vec<_>>()
                                                        }}
                                                    </select>
                                                </div>
                                            </Show>

                                            // Reasoning Configs per Model
                                            <Show
                                                when=move || !new_conn_enabled_models.get().is_empty()
                                                fallback=move || view! {}
                                            >
                                                <div class="space-y-3 pt-4 border-t border-theme-border/40 mt-3">
                                                    <h5 class="text-xs font-bold text-theme-text uppercase tracking-wider select-none">"Model Reasoning Settings"</h5>
                                                    <div class="space-y-3 max-h-52 overflow-y-auto pr-1">
                                                        {move || {
                                                            let selected_models = new_conn_enabled_models.get();
                                                            let current_configs = new_conn_reasoning_configs.get();
                                                            
                                                            selected_models.into_iter().map(|model_id| {
                                                                let m_id = model_id.clone();
                                                                let m_id_checkbox = model_id.clone();
                                                                let m_id_raw = model_id.clone();
                                                                let m_id_start = model_id.clone();
                                                                let m_id_end = model_id.clone();
                                                                
                                                                let config = current_configs.iter().find(|c| c.model_id == m_id).cloned().unwrap_or_else(|| {
                                                                    ModelReasoningConfig {
                                                                        model_id: m_id.clone(),
                                                                        enabled: false,
                                                                        is_raw_stream: false,
                                                                        start_tag: "<think>".to_string(),
                                                                        end_tag: "</think>".to_string(),
                                                                    }
                                                                });
                                                                
                                                                let enabled = config.enabled;
                                                                let is_raw_stream = config.is_raw_stream;
                                                                let start_tag = config.start_tag.clone();
                                                                let end_tag = config.end_tag.clone();
                                                                
                                                                view! {
                                                                    <div class="p-3 rounded-xl border border-theme-border/60 bg-theme-bg/10 space-y-2">
                                                                        <div class="flex items-center justify-between">
                                                                            <span class="font-mono text-xs font-semibold text-theme-text truncate max-w-[60%] select-none">{m_id.clone()}</span>
                                                                            
                                                                            <label class="flex items-center gap-1.5 cursor-pointer text-xs select-none">
                                                                                <input 
                                                                                    type="checkbox"
                                                                                    prop:checked=enabled
                                                                                    on:change=move |ev| {
                                                                                        let val = event_target_checked(&ev);
                                                                                        update_model_reasoning_config(
                                                                                            set_new_conn_reasoning_configs,
                                                                                            new_conn_reasoning_configs.get_untracked(),
                                                                                            m_id_checkbox.clone(),
                                                                                            move |c| c.enabled = val,
                                                                                        );
                                                                                    }
                                                                                    class="accent-theme-accent rounded cursor-pointer"
                                                                                />
                                                                                <span class="text-theme-muted font-medium">"Reasoning"</span>
                                                                            </label>
                                                                        </div>
                                                                        
                                                                        <Show when=move || enabled fallback=move || view! {}>
                                                                            {
                                                                                let m_id_raw_inner = m_id_raw.clone();
                                                                                let m_id_start_inner = m_id_start.clone();
                                                                                let m_id_end_inner = m_id_end.clone();
                                                                                let start_tag = start_tag.clone();
                                                                                let end_tag = end_tag.clone();
                                                                                view! {
                                                                                    <div class="pt-1.5 space-y-2 border-t border-theme-border/40">
                                                                                        <label class="flex items-center gap-1.5 cursor-pointer text-xs select-none">
                                                                                            <input 
                                                                                                type="checkbox"
                                                                                                prop:checked=is_raw_stream
                                                                                                on:change=move |ev| {
                                                                                                    let val = event_target_checked(&ev);
                                                                                                    update_model_reasoning_config(
                                                                                                        set_new_conn_reasoning_configs,
                                                                                                        new_conn_reasoning_configs.get_untracked(),
                                                                                                        m_id_raw_inner.clone(),
                                                                                                        move |c| c.is_raw_stream = val,
                                                                                                    );
                                                                                                }
                                                                                                class="accent-theme-accent rounded cursor-pointer"
                                                                                            />
                                                                                            <span class="text-theme-muted font-medium">"Expect tags in raw text stream"</span>
                                                                                        </label>
                                                                                        
                                                                                        <Show when=move || is_raw_stream fallback=move || view! {}>
                                                                                            {
                                                                                                let m_id_start_innermost = m_id_start_inner.clone();
                                                                                                let m_id_end_innermost = m_id_end_inner.clone();
                                                                                                let start_tag = start_tag.clone();
                                                                                                let end_tag = end_tag.clone();
                                                                                                view! {
                                                                                                    <div class="grid grid-cols-2 gap-2 pt-1">
                                                                                                        <div class="space-y-1">
                                                                                                            <label class="text-[10px] font-semibold text-theme-muted">"Start Tag"</label>
                                                                                                            <input 
                                                                                                                type="text"
                                                                                                                prop:value=start_tag.clone()
                                                                                                                on:input=move |ev| {
                                                                                                                    let val = event_target_value(&ev);
                                                                                                                    update_model_reasoning_config(
                                                                                                                        set_new_conn_reasoning_configs,
                                                                                                                        new_conn_reasoning_configs.get_untracked(),
                                                                                                                        m_id_start_innermost.clone(),
                                                                                                                        move |c| c.start_tag = val,
                                                                                                                    );
                                                                                                                }
                                                                                                                class="w-full bg-theme-input border border-theme-border rounded-lg px-2 py-1 text-xs text-theme-text font-mono outline-none focus:border-theme-accent theme-transition"
                                                                                                            />
                                                                                                        </div>
                                                                                                        <div class="space-y-1">
                                                                                                            <label class="text-[10px] font-semibold text-theme-muted">"End Tag"</label>
                                                                                                            <input 
                                                                                                                type="text"
                                                                                                                prop:value=end_tag.clone()
                                                                                                                on:input=move |ev| {
                                                                                                                    let val = event_target_value(&ev);
                                                                                                                    update_model_reasoning_config(
                                                                                                                        set_new_conn_reasoning_configs,
                                                                                                                        new_conn_reasoning_configs.get_untracked(),
                                                                                                                        m_id_end_innermost.clone(),
                                                                                                                        move |c| c.end_tag = val,
                                                                                                                    );
                                                                                                                }
                                                                                                                class="w-full bg-theme-input border border-theme-border rounded-lg px-2 py-1 text-xs text-theme-text font-mono outline-none focus:border-theme-accent theme-transition"
                                                                                                            />
                                                                                                        </div>
                                                                                                    </div>
                                                                                                }
                                                                                            }
                                                                                        </Show>
                                                                                    </div>
                                                                                }
                                                                            }
                                                                        </Show>
                                                                    </div>
                                                                }
                                                            }).collect::<Vec<_>>()
                                                        }}
                                                    </div>
                                                </div>
                                            </Show>
                                        </div>
                                    </Show>

                                    // Add Connection form actions
                                    <div class="flex justify-end gap-3 pt-4 border-t border-theme-border/60">
                                        <button
                                            type="button"
                                            on:click=move |_| {
                                                set_show_add_connection.set(false);
                                                set_editing_connection_id.set(None);
                                                set_new_conn_name.set(String::new());
                                                set_new_conn_api_key.set(String::new());
                                                set_new_conn_base_url.set(String::new());
                                                set_new_conn_fetched_models.set(Vec::new());
                                                set_new_conn_search_query.set(String::new());
                                                set_new_conn_enabled_models.set(Vec::new());
                                                set_new_conn_default_model.set(String::new());
                                                set_new_conn_reasoning_configs.set(Vec::new());
                                                set_fetching_models_error.set(None);
                                            }
                                            class="py-2 px-4 rounded-xl border border-theme-border text-theme-muted hover:bg-theme-bg hover:text-theme-text transition-all text-xs font-semibold active:scale-[0.97]"
                                        >
                                            "Back to List"
                                        </button>
                                        <button
                                            type="button"
                                            on:click=save_new_connection
                                            class="py-2 px-4.5 rounded-xl bg-theme-text text-theme-bg hover:opacity-90 transition-all text-xs font-semibold active:scale-[0.97]"
                                        >
                                            {move || if editing_connection_id.get().is_some() { "Save Changes" } else { "Save Connection" }}
                                        </button>
                                    </div>
                                </div>
                            </Show>
                        </div>

                        // Footer actions
                        <div class="pt-4 border-t border-theme-border/60 shrink-0 flex justify-end">
                            <button
                                on:click=move |_| {
                                    set_show_settings.set(false);
                                    set_show_add_connection.set(false);
                                    set_editing_connection_id.set(None);
                                    set_new_conn_name.set(String::new());
                                    set_new_conn_api_key.set(String::new());
                                    set_new_conn_base_url.set(String::new());
                                    set_new_conn_fetched_models.set(Vec::new());
                                    set_new_conn_enabled_models.set(Vec::new());
                                    set_new_conn_default_model.set(String::new());
                                    set_new_conn_reasoning_configs.set(Vec::new());
                                    set_fetching_models_error.set(None);
                                }
                                class="py-2 px-5 rounded-xl bg-theme-bg border border-theme-border text-theme-muted hover:text-theme-text hover:border-theme-accent/50 transition-all text-xs font-semibold active:scale-[0.97]"
                            >
                                "Close"
                            </button>
                        </div>
                    </div>
                </div>
            </Show>

            // Toast Notification
            <Show when=move || toast_message.get().is_some()>
                <div class="fixed bottom-6 right-6 z-50 px-4 py-3 rounded-xl bg-theme-panel border border-theme-border/80 text-theme-text text-xs font-semibold shadow-2xl flex items-center gap-2 select-none theme-transition">
                    <svg class="w-4 h-4 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                    <span>{move || toast_message.get().unwrap_or_default()}</span>
                </div>
            </Show>
        </div>
    }
}
