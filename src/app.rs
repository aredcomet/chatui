use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::Serialize;
use shared::{
    ApiConfig, ChatConversation, ChatMessage, ContentPart, MessageRole, Provider, StreamPayload,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;
}

// Arguments for Tauri commands
#[derive(Serialize)]
struct GetKeyArgs {
    provider: Provider,
}

#[derive(Serialize)]
struct SetKeyArgs {
    provider: Provider,
    key: String,
}

#[derive(Serialize)]
struct DeleteKeyArgs {
    provider: Provider,
}

#[derive(Serialize)]
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

            views.push(view! {
                <div class="my-3 rounded-lg overflow-hidden border border-slate-700/50 bg-black/40 font-mono text-sm max-w-full">
                    <div class="flex justify-between items-center bg-slate-900 px-4 py-1.5 text-xs text-slate-400 border-b border-slate-800">
                        <span>{if lang.is_empty() { "code".to_string() } else { lang.clone() }}</span>
                    </div>
                    <pre class="p-4 overflow-x-auto text-indigo-200">
                        <code>{code_content}</code>
                    </pre>
                </div>
            }.into_any());
        } else {
            for line in part.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                    views.push(view! {
                        <li class="ml-6 list-disc text-slate-300 py-0.5">{trimmed[2..].to_string()}</li>
                    }.into_any());
                } else if trimmed.starts_with("### ") {
                    views.push(view! {
                        <h4 class="text-md font-bold text-white mt-4 mb-2">{trimmed[4..].to_string()}</h4>
                    }.into_any());
                } else if trimmed.starts_with("## ") {
                    views.push(view! {
                        <h3 class="text-lg font-bold text-white mt-4 mb-2">{trimmed[3..].to_string()}</h3>
                    }.into_any());
                } else if trimmed.starts_with("# ") {
                    views.push(view! {
                        <h2 class="text-xl font-bold text-white mt-4 mb-2">{trimmed[2..].to_string()}</h2>
                    }.into_any());
                } else {
                    views.push(view! {
                        <p class="text-slate-300 py-1 leading-relaxed break-words">{trimmed.to_string()}</p>
                    }.into_any());
                }
            }
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
    // Core state signals
    let (conversations, set_conversations) = signal(Vec::<ChatConversation>::new());
    let (current_conversation_id, set_current_conversation_id) = signal(None::<String>);
    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
    let (input_text, set_input_text) = signal(String::new());
    let (attached_image, set_attached_image) = signal(None::<(String, String)>); // (mime, base64)

    // Streaming state
    let (is_streaming, set_is_streaming) = signal(false);
    let (stream_chunks, set_stream_chunks) = signal(None::<StreamPayload>);

    // Config settings
    let (selected_provider, set_selected_provider) = signal(Provider::OpenAi);
    let (selected_model, set_selected_model) = signal("gpt-4o-mini".to_string());
    let (temperature, set_temperature) = signal(0.7f32);

    // Modal control
    let (show_settings, set_show_settings) = signal(false);
    let (openai_configured, set_openai_configured) = signal(false);
    let (anthropic_configured, set_anthropic_configured) = signal(false);
    let (openai_input, set_openai_input) = signal(String::new());
    let (anthropic_input, set_anthropic_input) = signal(String::new());

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

    // 1. Listen for stream events on mount
    let _ = Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(payload) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload")) {
            if let Ok(payload_struct) = serde_wasm_bindgen::from_value::<StreamPayload>(payload) {
                set_stream_chunks.set(Some(payload_struct));
            }
        }
    }) as Box<dyn Fn(JsValue)>);

    // Mount logic: fetch credentials status and conversations
    let load_init_data = move || {
        spawn_local(async move {
            // Load conversations from backend
            let args = serde_wasm_bindgen::to_value(&()).unwrap();
            let result_js = invoke("load_conversations", args).await;
            if let Ok(convs) =
                serde_wasm_bindgen::from_value::<Vec<ChatConversation>>(result_js)
            {
                set_conversations.set(convs);
            }

            // Check keys status
            let oa_args = serde_wasm_bindgen::to_value(&GetKeyArgs {
                provider: Provider::OpenAi,
            })
            .unwrap();
            let oa_res = invoke("get_api_key", oa_args).await;
            if let Ok(Some(key)) = serde_wasm_bindgen::from_value::<Option<String>>(oa_res) {
                if !key.is_empty() {
                    set_openai_configured.set(true);
                }
            }

            let ant_args = serde_wasm_bindgen::to_value(&GetKeyArgs {
                provider: Provider::Anthropic,
            })
            .unwrap();
            let ant_res = invoke("get_api_key", ant_args).await;
            if let Ok(Some(key)) = serde_wasm_bindgen::from_value::<Option<String>>(ant_res) {
                if !key.is_empty() {
                    set_anthropic_configured.set(true);
                }
            }
        });
    };

    // Trigger initial load
    Effect::new(move |_| {
        load_init_data();

        // Bind streaming listener
        let handler = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload")) {
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

    // 2. Stream chunk processor effect
    Effect::new(move |_| {
        if let Some(payload) = stream_chunks.get() {
            let current_id = current_conversation_id.get_untracked();
            if Some(payload.conversation_id.clone()) == current_id {
                let mut current_msgs = messages.get_untracked();

                if payload.done {
                    set_is_streaming.set(false);
                    // Update chat list conversation messages and save
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
                } else if let Some(err_msg) = payload.error {
                    set_is_streaming.set(false);
                    current_msgs.push(ChatMessage::new_text(
                        MessageRole::Assistant,
                        format!("⚠️ API Error: {}", err_msg),
                    ));
                    set_messages.set(current_msgs);
                } else {
                    // Stream token chunk
                    if let Some(last_msg) = current_msgs.last_mut() {
                        if last_msg.role == MessageRole::Assistant {
                            if let Some(ContentPart::Text {
                                text: ref mut existing_text,
                            }) = last_msg.content.first_mut()
                            {
                                existing_text.push_str(&payload.text);
                            }
                        } else {
                            current_msgs.push(ChatMessage::new_text(
                                MessageRole::Assistant,
                                payload.text,
                            ));
                        }
                    } else {
                        current_msgs.push(ChatMessage::new_text(
                            MessageRole::Assistant,
                            payload.text,
                        ));
                    }
                    set_messages.set(current_msgs);
                }
                scroll_chat_to_bottom();
            }
        }
    });

    // 3. Selection handler when model selection changes
    let on_provider_change = move |ev| {
        let prov_str = event_target_value(&ev);
        if prov_str == "OpenAI" {
            set_selected_provider.set(Provider::OpenAi);
            set_selected_model.set("gpt-4o-mini".to_string());
        } else {
            set_selected_provider.set(Provider::Anthropic);
            set_selected_model.set("claude-3-5-sonnet-20241022".to_string());
        }
    };

    let on_model_change = move |ev| {
        set_selected_model.set(event_target_value(&ev));
    };

    // 4. Handle text inputs
    let update_input = move |ev| {
        set_input_text.set(event_target_value(&ev));
    };

    // 5. Select conversation from list
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
            set_selected_provider.set(convo.provider);
            set_selected_model.set(convo.model.clone());
            // Delay scroll slightly to allow rendering
            spawn_local(async move {
                scroll_chat_to_bottom();
            });
        }
    };

    // 6. Create new conversation
    let create_new_chat = move |_| {
        if is_streaming.get_untracked() {
            return;
        }
        let uuid = uuid::Uuid::new_v4().to_string();
        let provider = selected_provider.get_untracked();
        let model = selected_model.get_untracked();
        let new_convo = ChatConversation {
            id: uuid.clone(),
            title: format!("New Chat ({})", provider.to_string()),
            model,
            provider,
            messages: Vec::new(),
            updated_at: js_sys::Date::now() as u64,
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

    // 7. Delete conversation
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

    // 8. Upload files/images
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

    // 9. Send active message
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

        // 9a. Check or auto-create conversation
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
                };

                let mut current_convs = conversations.get_untracked();
                current_convs.insert(0, new_convo.clone());
                set_conversations.set(current_convs);

                set_current_conversation_id.set(Some(uuid.clone()));
                uuid
            }
        };

        // 9b. Construct the message content parts
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
            content: parts,
        };

        let mut active_msgs = messages.get_untracked();
        active_msgs.push(new_user_msg);
        set_messages.set(active_msgs.clone());

        // Reset input fields
        set_input_text.set(String::new());
        set_attached_image.set(None);
        set_is_streaming.set(true);

        // 9c. Call stream completions endpoint
        let provider = selected_provider.get_untracked();
        let model = selected_model.get_untracked();
        let temp = temperature.get_untracked();

        let api_config = ApiConfig {
            provider,
            model,
            temperature: temp,
            max_tokens: None,
        };

        let messages_c = active_msgs.clone();
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&SendMessageStreamArgs {
                conversation_id: active_id.clone(),
                config: api_config,
                messages: messages_c,
            })
            .unwrap();

            let invoke_res = invoke("send_message_stream", args).await;

            // Update chat details title based on first query
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
                invoke("save_conversation", serde_wasm_bindgen::to_value(&SaveConversationArgs {
                    conversation: convo_clone
                }).unwrap()).await;
            }
            set_conversations.set(convos);

            // Handle pre-stream connection error
            if let Some(err_str) = invoke_res.as_string() {
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

    // 10. API keys configuration saving
    let save_settings = move |_| {
        let op_key = openai_input.get_untracked();
        let ant_key = anthropic_input.get_untracked();

        spawn_local(async move {
            if !op_key.is_empty() {
                let args = serde_wasm_bindgen::to_value(&SetKeyArgs {
                    provider: Provider::OpenAi,
                    key: op_key,
                })
                .unwrap();
                invoke("set_api_key", args).await;
                set_openai_configured.set(true);
            }

            if !ant_key.is_empty() {
                let args = serde_wasm_bindgen::to_value(&SetKeyArgs {
                    provider: Provider::Anthropic,
                    key: ant_key,
                })
                .unwrap();
                invoke("set_api_key", args).await;
                set_anthropic_configured.set(true);
            }

            set_openai_input.set(String::new());
            set_anthropic_input.set(String::new());
            set_show_settings.set(false);
        });
    };

    let delete_key_click = move |provider: Provider| {
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&DeleteKeyArgs { provider }).unwrap();
            invoke("delete_api_key", args).await;
            match provider {
                Provider::OpenAi => set_openai_configured.set(false),
                Provider::Anthropic => set_anthropic_configured.set(false),
            }
        });
    };

    view! {
        <div
            class="flex h-screen w-screen overflow-hidden bg-bg-darkest text-slate-100 font-sans select-none"
            on:dragover=handle_drag_over
            on:drop=handle_drop
        >
            // ─── SIDEBAR ───
            <aside class="flex flex-col w-72 bg-bg-darkest border-r border-slate-800/80 shrink-0">
                // Sidebar Header
                <div class="p-4 border-b border-slate-800/80">
                    <button
                        on:click=create_new_chat
                        disabled=move || is_streaming.get()
                        class="w-full flex items-center justify-center gap-2 py-2.5 px-4 rounded-xl border border-slate-700/60 bg-slate-800/40 text-slate-200 font-medium hover:bg-slate-800 hover:text-white transition-all active:scale-[0.98] disabled:opacity-40 disabled:cursor-not-allowed"
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
                        let active_class = if active {
                            "bg-slate-800/80 text-white font-medium shadow-md shadow-black/10 border-l-2 border-accent-indigo"
                        } else {
                            "text-slate-400 hover:bg-slate-800/30 hover:text-slate-200 border-l-2 border-transparent"
                        };
                        let active_trash_class = if active { "text-slate-400 hover:text-red-400" } else { "text-slate-500 hover:text-red-400" };


                        let id_trash = id.clone();

                        view! {
                            <div
                                on:click=move |_| select_conversation(id.clone())
                                class={format!("group flex items-center justify-between px-3 py-2.5 rounded-xl cursor-pointer transition-all {}", active_class)}
                            >
                                <div class="flex items-center gap-2.5 min-w-0 pr-2">
                                    <svg class="w-4 h-4 shrink-0 opacity-60" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"/>
                                    </svg>
                                    <span class="text-sm truncate select-none">{title}</span>
                                </div>
                                <button
                                    on:click=move |ev| delete_chat(id_trash.clone(), ev)
                                    class={format!("opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-slate-700/50 transition-all {}", active_trash_class)}
                                >
                                    <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                                    </svg>
                                </button>
                            </div>
                        }
                    }).collect::<Vec<_>>()}
                </div>

                // Sidebar Footer (Settings Trigger)
                <div class="p-4 border-t border-slate-800/80 bg-slate-950/40">
                    <button
                        on:click=move |_| set_show_settings.set(true)
                        class="w-full flex items-center justify-between px-3 py-2 rounded-xl text-slate-400 hover:bg-slate-850 hover:text-slate-200 transition-all"
                    >
                        <div class="flex items-center gap-2.5">
                            <svg class="w-5 h-5 opacity-70" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/>
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                            </svg>
                            <span class="text-sm font-medium">"Settings"</span>
                        </div>
                        <div class="flex gap-1">
                            <span class={format!("w-2 h-2 rounded-full {}", if openai_configured.get() { "bg-emerald-500" } else { "bg-amber-500" })}></span>
                            <span class={format!("w-2 h-2 rounded-full {}", if anthropic_configured.get() { "bg-emerald-500" } else { "bg-amber-500" })}></span>
                        </div>
                    </button>
                </div>
            </aside>

            // ─── MAIN CHAT AREA ───
            <main class="flex-1 flex flex-col min-w-0 bg-bg-dark h-full relative">
                // Chat Header
                <header class="flex items-center justify-between h-16 border-b border-slate-800/80 px-6 shrink-0 bg-bg-dark bg-opacity-70 backdrop-blur-md">
                    <div class="flex items-center gap-4">
                        // Provider selector
                        <div class="relative">
                            <select
                                on:change=on_provider_change
                                class="bg-slate-800 border border-slate-700/60 rounded-xl px-3 py-1.5 text-sm font-medium text-slate-200 outline-none cursor-pointer hover:border-slate-650 transition-all select-none"
                            >
                                <option value="OpenAI" selected={move || selected_provider.get() == Provider::OpenAi}>"OpenAI"</option>
                                <option value="Anthropic" selected={move || selected_provider.get() == Provider::Anthropic}>"Anthropic"</option>
                            </select>
                        </div>

                        // Model selector
                        <div class="relative">
                            <select
                                on:change=on_model_change
                                class="bg-slate-800 border border-slate-700/60 rounded-xl px-3 py-1.5 text-sm font-medium text-slate-200 outline-none cursor-pointer hover:border-slate-650 transition-all select-none"
                            >
                                {move || {
                                    let model_val = selected_model.get();
                                    match selected_provider.get() {
                                        Provider::OpenAi => {
                                            view! {
                                                <option value="gpt-4o-mini" selected={model_val == "gpt-4o-mini"}>"gpt-4o-mini (Fast)"</option>
                                                <option value="gpt-4o" selected={model_val == "gpt-4o"}>"gpt-4o (Powerful & Multimodal)"</option>
                                            }.into_any()
                                        }
                                        Provider::Anthropic => {
                                            view! {
                                                <option value="claude-3-5-sonnet-20241022" selected={model_val == "claude-3-5-sonnet-20241022"}>"Claude 3.5 Sonnet (VLM)"</option>
                                                <option value="claude-3-5-haiku-20241022" selected={model_val == "claude-3-5-haiku-20241022"}>"Claude 3.5 Haiku (Fast)"</option>
                                            }.into_any()
                                        }
                                    }
                                }}
                            </select>
                        </div>
                    </div>

                    // Temperature indicator
                    <div class="flex items-center gap-2.5">
                        <span class="text-xs text-slate-450 font-medium font-mono select-none">
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
                            class="w-20 accent-accent-indigo h-1 rounded bg-slate-700 outline-none cursor-pointer"
                        />
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
                            <div class="flex flex-col items-center justify-center h-full max-w-lg mx-auto text-center space-y-4 select-none">
                                <div class="p-4 rounded-3xl bg-indigo-500/10 border border-indigo-500/20 text-indigo-400">
                                    <svg class="w-10 h-10" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"/>
                                    </svg>
                                </div>
                                <h2 class="text-xl font-bold text-white">"Ask anything"</h2>
                                <p class="text-sm text-slate-400 leading-relaxed">
                                    "Select a model, write your query, or drop an image file directly into the editor below to execute multimodal VLM inference securely."
                                </p>
                            </div>
                        }
                    >
                        {move || messages.get().iter().map(|msg| {
                            let is_user = msg.role == MessageRole::User;
                            let bg = if is_user { "bg-slate-800/90 text-slate-100 border border-slate-700/40 ml-auto" } else { "bg-slate-900/60 border border-slate-800/60 mr-auto" };
                            let header = if is_user { "You" } else { "AI" };

                            view! {
                                <div class={format!("flex flex-col max-w-[85%] rounded-2xl p-4 shadow-lg shadow-black/5 {}", bg)}>
                                    // Header metadata
                                    <div class="flex items-center gap-2 mb-2 text-xs font-semibold text-slate-400 uppercase tracking-wider select-none">
                                        {header}
                                    </div>

                                    // Render Content Parts
                                    <div class="space-y-3 max-w-full overflow-hidden">
                                        {msg.content.iter().map(|part| match part {
                                            ContentPart::Text { text } => {
                                                render_message_content(text.clone()).into_any()
                                            }
                                            ContentPart::Image { mime_type, base64 } => {
                                                view! {
                                                    <div class="rounded-lg overflow-hidden border border-slate-700 max-w-sm mt-1">
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
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </Show>

                    // Loader when thinking
                    <Show
                        when=move || is_streaming.get()
                        fallback=move || view! {}
                    >
                        <div class="flex flex-col max-w-[85%] rounded-2xl p-4 bg-slate-900/60 border border-slate-800/60 mr-auto shadow-lg shadow-black/5">
                            <div class="flex items-center gap-2 mb-2 text-xs font-semibold text-slate-400 uppercase tracking-wider select-none">
                                "AI is thinking..."
                            </div>
                            <div class="flex space-x-1.5 py-2.5">
                                <div class="w-2 h-2 bg-indigo-500 rounded-full animate-bounce" style="animation-delay: 0ms"></div>
                                <div class="w-2 h-2 bg-indigo-500 rounded-full animate-bounce" style="animation-delay: 150ms"></div>
                                <div class="w-2 h-2 bg-indigo-500 rounded-full animate-bounce" style="animation-delay: 300ms"></div>
                            </div>
                        </div>
                    </Show>
                </div>

                // Bottom Prompt Box / Input
                <footer class="p-6 border-t border-slate-800/80 bg-bg-dark shrink-0">
                    <form on:submit=send_message class="max-w-4xl mx-auto flex flex-col gap-3 bg-slate-900/90 border border-slate-850 rounded-2xl p-3 shadow-2xl relative">
                        // Attached Image Thumbnail Preview
                        <Show
                            when=move || attached_image.get().is_some()
                            fallback=move || view! {}
                        >
                            <div class="relative w-20 h-20 rounded-xl overflow-hidden border border-slate-700/80 group">
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
                            // Image Upload button trigger
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
                                    class="flex items-center justify-center p-2.5 rounded-xl border border-slate-800 bg-slate-850 hover:bg-slate-800 text-slate-400 hover:text-slate-200 transition-all cursor-pointer shadow shadow-black/10 active:scale-[0.96]"
                                >
                                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"/>
                                    </svg>
                                </label>
                            </div>

                            // Text prompt
                            <textarea
                                id="prompt-input-box"
                                placeholder={move || if is_streaming.get() { "AI is generating..." } else { "Type your prompt or drag/drop an image..." }}
                                prop:value=move || input_text.get()
                                on:input=update_input
                                disabled=move || is_streaming.get()
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    if ev.key() == "Enter" && !ev.shift_key() {
                                        ev.prevent_default();
                                        // Dispatch a submit click to send the message
                                        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                            if let Some(btn) = doc.get_element_by_id("send-btn-el") {
                                                btn.dyn_into::<web_sys::HtmlButtonElement>().unwrap().click();
                                            }
                                        }
                                    }
                                }
                                rows="1"
                                class="flex-1 min-h-[42px] max-h-48 resize-none bg-transparent border-0 py-2.5 text-sm text-slate-100 placeholder-slate-500 outline-none scrollbar-none"
                            ></textarea>

                            // Send Button
                            <button
                                type="submit"
                                id="send-btn-el"
                                disabled=move || is_streaming.get() || (input_text.get().trim().is_empty() && attached_image.get().is_none())
                                class="shrink-0 flex items-center justify-center p-2.5 rounded-xl bg-accent-indigo hover:bg-accent-indigo_hover text-white transition-all shadow-md shadow-indigo-650/10 active:scale-[0.96] disabled:opacity-35 disabled:cursor-not-allowed"
                            >
                                <svg class="w-5 h-5 rotate-90" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8"/>
                                </svg>
                            </button>
                        </div>
                    </form>
                </footer>
            </main>

            // ─── SETTINGS MODAL ───
            <Show
                when=move || show_settings.get()
                fallback=move || view! {}
            >
                <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm animate-fade-in p-4">
                    <div class="w-full max-w-md bg-slate-900 border border-slate-800 rounded-3xl shadow-2xl p-6 relative overflow-hidden select-none">
                        // Header
                        <div class="flex justify-between items-center mb-6">
                            <h3 class="text-lg font-bold text-white">"Secure Settings"</h3>
                            <button
                                on:click=move |_| set_show_settings.set(false)
                                class="text-slate-400 hover:text-white p-1 rounded-lg hover:bg-slate-800 transition-all"
                            >
                                <svg class="w-5.5 h-5.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
                                </svg>
                            </button>
                        </div>

                        // Description
                        <p class="text-xs text-slate-400 leading-relaxed mb-6">
                            "API keys are stored directly inside your system's secure OS keychain (macOS Keychain, Windows Credential Manager) and never in plain text."
                        </p>

                        <div class="space-y-5">
                            // OpenAI Configuration
                            <div class="space-y-2">
                                <div class="flex items-center justify-between text-sm font-semibold text-slate-350">
                                    <span>"OpenAI API Key"</span>
                                    <div class="flex items-center gap-1.5">
                                        <Show
                                            when=move || openai_configured.get()
                                            fallback=move || view! {
                                                <span class="flex items-center gap-1 text-[11px] text-amber-500 font-medium">
                                                    <span class="w-1.5 h-1.5 rounded-full bg-amber-500"></span>
                                                    "Not configured"
                                                </span>
                                            }
                                        >
                                            <div class="flex items-center gap-1.5">
                                                <span class="flex items-center gap-1 text-[11px] text-emerald-400 font-medium">
                                                    <span class="w-1.5 h-1.5 rounded-full bg-emerald-500"></span>
                                                    "Configured securely"
                                                </span>
                                                <button
                                                    on:click=move |_| delete_key_click(Provider::OpenAi)
                                                    class="text-[10px] text-red-400 hover:underline ml-1"
                                                >
                                                    "Remove"
                                                </button>
                                            </div>
                                        </Show>
                                    </div>
                                </div>
                                <input
                                    type="password"
                                    placeholder="sk-proj-..."
                                    prop:value=move || openai_input.get()
                                    on:input=move |ev| set_openai_input.set(event_target_value(&ev))
                                    class="w-full px-3 py-2.5 text-sm bg-slate-950 border border-slate-800 rounded-xl outline-none text-slate-200 placeholder-slate-650 focus:border-indigo-650/80 transition-all font-mono"
                                />
                            </div>

                            // Anthropic Configuration
                            <div class="space-y-2">
                                <div class="flex items-center justify-between text-sm font-semibold text-slate-350">
                                    <span>"Anthropic Claude API Key"</span>
                                    <div class="flex items-center gap-1.5">
                                        <Show
                                            when=move || anthropic_configured.get()
                                            fallback=move || view! {
                                                <span class="flex items-center gap-1 text-[11px] text-amber-500 font-medium">
                                                    <span class="w-1.5 h-1.5 rounded-full bg-amber-500"></span>
                                                    "Not configured"
                                                </span>
                                            }
                                        >
                                            <div class="flex items-center gap-1.5">
                                                <span class="flex items-center gap-1 text-[11px] text-emerald-400 font-medium">
                                                    <span class="w-1.5 h-1.5 rounded-full bg-emerald-500"></span>
                                                    "Configured securely"
                                                </span>
                                                <button
                                                    on:click=move |_| delete_key_click(Provider::Anthropic)
                                                    class="text-[10px] text-red-400 hover:underline ml-1"
                                                >
                                                    "Remove"
                                                </button>
                                            </div>
                                        </Show>
                                    </div>
                                </div>
                                <input
                                    type="password"
                                    placeholder="sk-ant-..."
                                    prop:value=move || anthropic_input.get()
                                    on:input=move |ev| set_anthropic_input.set(event_target_value(&ev))
                                    class="w-full px-3 py-2.5 text-sm bg-slate-950 border border-slate-800 rounded-xl outline-none text-slate-200 placeholder-slate-650 focus:border-indigo-650/80 transition-all font-mono"
                                />
                            </div>
                        </div>

                        // Actions
                        <div class="flex justify-end gap-3 mt-8">
                            <button
                                on:click=move |_| set_show_settings.set(false)
                                class="py-2 px-4 rounded-xl border border-slate-800 hover:bg-slate-800 text-slate-400 hover:text-slate-250 transition-all text-sm font-semibold active:scale-[0.97]"
                            >
                                "Cancel"
                            </button>
                            <button
                                on:click=save_settings
                                class="py-2 px-4.5 rounded-xl bg-accent-indigo hover:bg-accent-indigo_hover text-white transition-all text-sm font-semibold shadow-md active:scale-[0.97]"
                            >
                                "Save Keys"
                            </button>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}
