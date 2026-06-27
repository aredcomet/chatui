use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::Serialize;
use shared::{
    ApiConfig, ChatConversation, ChatMessage, Connection, ContentBlock, MessageMetadata,
    MessageRole, MessageVersion, Provider, StreamPayload,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// Submodules
pub mod bindings;
pub mod components;
pub mod markdown;
pub mod theme;

use bindings::*;
use components::connections_modal::ConnectionsModal;
use components::thinking_block::ThinkingBlock;
use markdown::*;
use theme::*;

fn append_assistant_token(version: &mut MessageVersion, token: &str) {
    if version.content.is_empty() {
        version.content.push(ContentBlock::Text {
            text: String::new(),
        });
    }

    let last_block = match version.content.last_mut() {
        Some(ContentBlock::Text { .. }) | Some(ContentBlock::Reasoning { .. }) => {
            version.content.last_mut().unwrap()
        }
        _ => {
            version.content.push(ContentBlock::Text {
                text: String::new(),
            });
            version.content.last_mut().unwrap()
        }
    };

    match last_block {
        ContentBlock::Text { text } => {
            text.push_str(token);
            if let Some(pos) = text.find("<think>") {
                let pre_think = text[..pos].to_string();
                let post_think = text[pos + 7..].to_string();
                *text = pre_think;
                version
                    .content
                    .push(ContentBlock::Reasoning { text: post_think });
            }
        }
        ContentBlock::Reasoning { text } => {
            text.push_str(token);
            if let Some(pos) = text.find("</think>") {
                let pre_end = text[..pos].to_string();
                let post_end = text[pos + 8..].to_string();
                *text = pre_end;
                version.content.push(ContentBlock::Text { text: post_end });
            }
        }
        _ => unreachable!(),
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Conversations state
    let (conversations, set_conversations) = signal(Vec::<ChatConversation>::new());
    let (current_conversation_id, set_current_conversation_id) = signal(None::<String>);
    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
    let pending_messages = StoredValue::new(None::<Vec<ChatMessage>>);
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



    // True when the user has intentionally scrolled away from the bottom during streaming.
    // This is a latch: set on any upward scroll, cleared only when the user returns to the bottom
    // (either by scrolling back, or when we auto-scroll and they haven't moved).
    let user_scroll_pinned = StoredValue::new(false);

    // Raw pixel helper — only used internally by the scroll event handler
    let is_at_bottom_raw = || -> bool {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(el) = document.get_element_by_id("chat-messages-container") {
                    let scroll_top = el.scroll_top();
                    let scroll_height = el.scroll_height();
                    let client_height = el.client_height();
                    // Tight threshold — only treat as "at bottom" when actually at the bottom
                    return scroll_height - scroll_top - client_height < 10;
                }
            }
        }
        true
    };

    // Kept for compatibility in places that still call is_scroll_at_bottom()
    let is_scroll_at_bottom = move || -> bool { !user_scroll_pinned.get_value() };

    // Scroll helper — programmatic scroll back to bottom also clears the pinned flag
    let scroll_chat_to_bottom = move || {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(el) = document.get_element_by_id("chat-messages-container") {
                    el.set_scroll_top(el.scroll_height());
                    user_scroll_pinned.set_value(false);
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
            if let Ok(convs) = serde_wasm_bindgen::from_value::<Vec<ChatConversation>>(res_convs) {
                set_conversations.set(convs);
            }

            // Fetch saved connections
            let res_conns = invoke(
                "load_connections",
                serde_wasm_bindgen::to_value(&()).unwrap(),
            )
            .await;
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

    // Periodic synchronization task for buffered streaming chunks to prevent UI jerkiness/lag
    Effect::new(move |_| {
        let pending = pending_messages;
        let is_scroll = is_scroll_at_bottom;
        let scroll_bottom = scroll_chat_to_bottom;

        let cb = Closure::wrap(Box::new(move || {
            if let Some(msgs) = pending.get_value() {
                let should_scroll = is_scroll();
                set_messages.set(msgs);
                if should_scroll {
                    scroll_bottom();
                }
                pending.set_value(None);
            }
        }) as Box<dyn FnMut()>);

        if let Some(window) = web_sys::window() {
            let _ = window.set_interval_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                75, // update every 75ms (approx 13 updates per second)
            );
        }
        cb.forget();
    });

    // Effects for mounting and listening
    Effect::new(move |_| {
        load_init_data();

        let handler = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload")) {
                web_sys::console::log_2(
                    &JsValue::from_str("Frontend: received chat-stream-chunk payload:"),
                    &payload,
                );
                if let Ok(payload_struct) = serde_wasm_bindgen::from_value::<StreamPayload>(payload)
                {
                    set_stream_chunks.set(Some(payload_struct));
                }
            }
        }) as Box<dyn Fn(JsValue)>);

        spawn_local(async move {
            let handler_js = handler.into_js_value();
            listen("chat-stream-chunk", handler_js.unchecked_ref()).await;
        });

        // Attach scroll listener to chat container to latch user intent
        let scroll_handler = Closure::wrap(Box::new(move || {
            if is_at_bottom_raw() {
                // User scrolled back to the very bottom — release the latch
                user_scroll_pinned.set_value(false);
            } else {
                // User has scrolled away from bottom — lock auto-scroll
                user_scroll_pinned.set_value(true);
            }
        }) as Box<dyn FnMut()>);

        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(el) = document.get_element_by_id("chat-messages-container") {
                    let _ = el.add_event_listener_with_callback(
                        "scroll",
                        scroll_handler.as_ref().unchecked_ref(),
                    );
                }
            }
        }
        scroll_handler.forget();
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
        conn.reasoning_configs
            .iter()
            .find(|rc| rc.model_id == model)
            .cloned()
    };

    // Streaming chunk listener
    Effect::new(move |_| {
        if let Some(payload) = stream_chunks.get() {
            let current_id = current_conversation_id.get_untracked();
            if Some(payload.conversation_id.clone()) == current_id {
                if payload.done {
                    set_is_streaming.set(false);

                    // Drain the pending buffer (the true in-progress state) for final metadata update
                    let mut current_msgs = pending_messages
                        .get_value()
                        .unwrap_or_else(|| messages.get_untracked());
                    pending_messages.set_value(None);

                    if let Some(last_msg) = current_msgs.last_mut() {
                        if last_msg.role == MessageRole::Assistant {
                            if let Some(version) =
                                last_msg.versions.get_mut(last_msg.active_version)
                            {
                                version.metadata.ttft_ms = payload.ttft_ms;
                                version.metadata.tokens_per_sec = payload.tokens_per_sec;
                                version.metadata.stop_reason = payload.stop_reason.clone();
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
                                let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                                    conversation: convo_clone,
                                })
                                .unwrap();
                                invoke("save_conversation", args).await;
                            });
                        }
                        set_conversations.set(convos);
                    }

                    let should_scroll = is_scroll_at_bottom();
                    set_messages.set(current_msgs);
                    if should_scroll {
                        scroll_chat_to_bottom();
                    }
                } else if let Some(err_msg) = payload.error {
                    set_is_streaming.set(false);
                    pending_messages.set_value(None);
                    let mut current_msgs = messages.get_untracked();
                    current_msgs.push(ChatMessage::new_text(
                        MessageRole::Assistant,
                        format!("⚠️ Error: {}", err_msg),
                    ));
                    set_messages.set(current_msgs);
                    scroll_chat_to_bottom();
                } else {
                    // KEY FIX: always read from the in-progress buffer first so chunks accumulate
                    // sequentially. Reading from the committed `messages` signal would give stale
                    // data (the last 75ms flush snapshot), causing chunks to overwrite each other.
                    let mut current_msgs = pending_messages
                        .get_value()
                        .unwrap_or_else(|| messages.get_untracked());

                    if let Some(last_msg) = current_msgs.last_mut() {
                        if last_msg.role == MessageRole::Assistant {
                            if let Some(version) =
                                last_msg.versions.get_mut(last_msg.active_version)
                            {
                                // Update metadata if any is returned in the payload
                                if payload.done {
                                    version.metadata.ttft_ms = payload.ttft_ms;
                                    version.metadata.tokens_per_sec = payload.tokens_per_sec;
                                    version.metadata.stop_reason = payload.stop_reason.clone();
                                }

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
                                append_assistant_token(version, &text);
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

                            let metadata = MessageMetadata {
                                model: selected_model.get_untracked(),
                                provider: selected_provider.get_untracked(),
                                connection_id: active_connection_id
                                    .get_untracked()
                                    .unwrap_or_default(),
                                created_at: chrono::Utc::now(),
                                ttft_ms: payload.ttft_ms,
                                tokens_per_sec: payload.tokens_per_sec,
                                stop_reason: payload.stop_reason.clone(),
                            };

                            let mut version = MessageVersion {
                                content: vec![],
                                metadata,
                            };
                            append_assistant_token(&mut version, &text);

                            current_msgs.push(ChatMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                role: MessageRole::Assistant,
                                versions: vec![version],
                                active_version: 0,
                            });
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

                        let metadata = MessageMetadata {
                            model: selected_model.get_untracked(),
                            provider: selected_provider.get_untracked(),
                            connection_id: active_connection_id.get_untracked().unwrap_or_default(),
                            created_at: chrono::Utc::now(),
                            ttft_ms: payload.ttft_ms,
                            tokens_per_sec: payload.tokens_per_sec,
                            stop_reason: payload.stop_reason.clone(),
                        };

                        let mut version = MessageVersion {
                            content: vec![],
                            metadata,
                        };
                        append_assistant_token(&mut version, &text);

                        current_msgs.push(ChatMessage {
                            id: uuid::Uuid::new_v4().to_string(),
                            role: MessageRole::Assistant,
                            versions: vec![version],
                            active_version: 0,
                        });
                    }
                    pending_messages.set_value(Some(current_msgs));
                }
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

        if let Some(conn) = connections.get_untracked().iter().find(|c| c.id == id_str) {
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
        let target = ev
            .target()
            .unwrap()
            .dyn_into::<web_sys::HtmlTextAreaElement>()
            .unwrap();
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
        if let Some(convo) = conversations.get_untracked().iter().find(|c| c.id == id) {
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
                .or_else(|| conns.iter().find(|c| c.provider == convo.provider).cloned())
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
            (
                None,
                selected_provider.get_untracked(),
                selected_model.get_untracked(),
            )
        };

        let new_convo = ChatConversation {
            id: uuid.clone(),
            title: format!("New Chat ({})", provider.to_string()),
            model,
            provider,
            created_at: chrono::Utc::now(),
            folder_id: None,
            system_prompt: None,
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
        if new_title.trim().is_empty() {
            return;
        }
        let mut current_convs = conversations.get_untracked();
        if let Some(convo) = current_convs.iter_mut().find(|c| c.id == id) {
            convo.title = new_title.trim().to_string();
            let convo_clone = convo.clone();
            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                    conversation: convo_clone,
                })
                .unwrap();
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
                                if let Ok(res) = wasm_bindgen_futures::JsFuture::from(promise).await
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
                    if let ContentBlock::Text { text } = part {
                        *text = new_text.clone();
                        has_text = true;
                        break;
                    }
                }
                if !has_text {
                    version.content.insert(
                        0,
                        ContentBlock::Text {
                            text: new_text.clone(),
                        },
                    );
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
                    created_at: chrono::Utc::now(),
                    folder_id: convo.folder_id.clone(),
                    system_prompt: convo.system_prompt.clone(),
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
            content: vec![ContentBlock::Text {
                text: String::new(),
            }],
            metadata: MessageMetadata {
                model: selected_model.get_untracked(),
                provider: selected_provider.get_untracked(),
                connection_id: active_connection_id.get_untracked().unwrap_or_default(),
                created_at: chrono::Utc::now(),
                ttft_ms: None,
                tokens_per_sec: None,
                stop_reason: None,
            },
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
                    created_at: chrono::Utc::now(),
                    folder_id: None,
                    system_prompt: None,
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
            model: model.clone(),
            temperature: temp,
            max_tokens: None,
            connection_id: conn_id.clone(),
        };

        let active_id_c = active_id.clone();
        spawn_local(async move {
            let mut parts = Vec::new();
            if !text.is_empty() {
                parts.push(ContentBlock::Text { text: text.clone() });
            }

            if let Some((mime, b64)) = img {
                #[derive(Serialize)]
                struct SaveThreadAssetArgs {
                    thread_id: String,
                    filename: String,
                    base64_data: String,
                }

                let ext = match mime.as_str() {
                    "image/png" => "png",
                    "image/jpeg" | "image/jpg" => "jpg",
                    "image/webp" => "webp",
                    "image/gif" => "gif",
                    _ => "bin",
                };
                let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
                let args = serde_wasm_bindgen::to_value(&SaveThreadAssetArgs {
                    thread_id: active_id_c.clone(),
                    filename,
                    base64_data: b64,
                })
                .unwrap();

                let promise = invoke_raw("save_thread_asset", args);
                if let Ok(path_val) = wasm_bindgen_futures::JsFuture::from(promise).await {
                    if let Some(path_str) = path_val.as_string() {
                        parts.push(ContentBlock::Image {
                            path: path_str,
                            mime_type: mime,
                        });
                    }
                }
            }

            let new_user_msg = ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                role: MessageRole::User,
                versions: vec![MessageVersion {
                    content: parts,
                    metadata: MessageMetadata {
                        model: model.clone(),
                        provider,
                        connection_id: conn_id.clone().unwrap_or_default(),
                        created_at: chrono::Utc::now(),
                        ttft_ms: None,
                        tokens_per_sec: None,
                        stop_reason: None,
                    },
                }],
                active_version: 0,
            };

            let mut active_msgs = messages.get_untracked();
            active_msgs.push(new_user_msg.clone());
            set_messages.set(active_msgs.clone());

            let args = serde_wasm_bindgen::to_value(&SendMessageStreamArgs {
                conversation_id: active_id_c.clone(),
                config: api_config,
                messages: active_msgs.clone(),
            })
            .unwrap();

            let stream_promise = invoke_raw("send_message_stream", args);
            let invoke_res = wasm_bindgen_futures::JsFuture::from(stream_promise).await;

            let mut convos = conversations.get_untracked();
            if let Some(convo) = convos.iter_mut().find(|c| c.id == active_id_c) {
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
                let err_str = err
                    .as_string()
                    .unwrap_or_else(|| "Unknown connection error".to_string());
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
                    class="flex-1 overflow-y-auto px-4 py-4 space-y-4 scrollbar-thin scroll-smooth"
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
                            let reasoning_duration: Option<u64> = None;

                            if is_user {
                                view! {
                                    <div class="w-full flex justify-end py-1 select-text">
                                        <div class="w-full max-w-[70ch] bg-theme-panel text-theme-text border border-theme-border/60 rounded-2xl p-3 shadow-sm transition-all theme-transition">

                                            <div class="space-y-3 max-w-full overflow-hidden">
                                                <Show
                                                    when=move || is_editing
                                                    fallback=move || {
                                                        let parts = content_parts.clone();
                                                        view! {
                                                            <div class="space-y-3 max-w-full overflow-hidden prose max-w-none font-serif leading-relaxed text-theme-text">
                                                                {parts.iter().map(|part| match part {
                                                                    ContentBlock::Text { text } => {
                                                                        render_message_content(text.clone()).into_any()
                                                                    }
                                                                    ContentBlock::Reasoning { text } => {
                                                                        view! {
                                                                            <div class="whitespace-pre-wrap border-l-2 border-theme-accent/30 pl-3 py-1 my-1 text-theme-muted/90 italic font-sans text-sm bg-theme-panel/50 rounded-r-md">
                                                                                "Thinking: " {text.clone()}
                                                                            </div>
                                                                        }.into_any()
                                                                    }
                                                                    ContentBlock::Image { path, mime_type: _ } => {
                                                                        let src_url = convert_file_src(path, None);
                                                                        view! {
                                                                            <div class="rounded-lg overflow-hidden border border-theme-border max-w-sm mt-1">
                                                                                <img
                                                                                    src=src_url
                                                                                    class="w-full object-cover max-h-60"
                                                                                    alt="Attached Image"
                                                                                />
                                                                            </div>
                                                                        }.into_any()
                                                                    }
                                                                    ContentBlock::Document { path, mime_type } => {
                                                                        let path_desc = path.as_ref().map(|p| p.as_str()).unwrap_or("missing");
                                                                        view! {
                                                                            <div class="flex items-center gap-2 p-2 bg-theme-panel rounded-lg border border-theme-border/40 text-xs text-theme-muted/90 font-sans mt-1">
                                                                                "📄 Document (" {mime_type.clone()} "): " {path_desc.to_string()}
                                                                            </div>
                                                                        }.into_any()
                                                                    }
                                                                    ContentBlock::Audio { path, duration_secs } => {
                                                                        view! {
                                                                            <div class="flex items-center gap-2 p-2 bg-theme-panel rounded-lg border border-theme-border/40 text-xs text-theme-muted/90 font-sans mt-1">
                                                                                "🎵 Audio (" {*duration_secs} "s): " {path.clone()}
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
                                    <div class="w-full py-3 transition-all theme-transition select-text">
                                        <div class="flex items-center gap-2 mb-1.5 text-xs font-semibold text-theme-muted uppercase tracking-wider select-none font-sans">
                                            "AI"
                                        </div>

                                        <div class="prose max-w-none font-serif leading-relaxed text-theme-text overflow-hidden">
                                            <Show
                                                when=move || is_editing
                                                fallback=move || {
                                                    let parts = content_parts.clone();
                                                    view! {
                                                        <div class="space-y-3 max-w-full overflow-hidden">
                                                            {parts.iter().map(|part| match part {
                                                                ContentBlock::Text { text } => {
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
                                                                ContentBlock::Reasoning { text } => {
                                                                    let is_thinking_active = is_streaming.get() && idx == messages.get().len() - 1;
                                                                    view! {
                                                                        <div class="flex flex-col w-full my-1">
                                                                            <ThinkingBlock
                                                                                thinking=text.clone()
                                                                                is_thinking=is_thinking_active
                                                                                duration_ms=reasoning_duration
                                                                            />
                                                                        </div>
                                                                    }.into_any()
                                                                }
                                                                ContentBlock::Image { path, mime_type: _ } => {
                                                                    let src_url = convert_file_src(path, None);
                                                                    view! {
                                                                        <div class="rounded-lg overflow-hidden border border-theme-border max-w-sm mt-1">
                                                                            <img
                                                                                src=src_url
                                                                                class="w-full object-cover max-h-60"
                                                                                alt="Attached Image"
                                                                            />
                                                                        </div>
                                                                    }.into_any()
                                                                }
                                                                ContentBlock::Document { path, mime_type } => {
                                                                    let path_desc = path.as_ref().map(|p| p.as_str()).unwrap_or("missing");
                                                                    view! {
                                                                        <div class="flex items-center gap-2 p-2 bg-theme-panel rounded-lg border border-theme-border/40 text-xs text-theme-muted/90 font-sans mt-1">
                                                                            "📄 Document (" {mime_type.clone()} "): " {path_desc.to_string()}
                                                                        </div>
                                                                    }.into_any()

                                                                }
                                                                ContentBlock::Audio { path, duration_secs } => {
                                                                    view! {
                                                                        <div class="flex items-center gap-2 p-2 bg-theme-panel rounded-lg border border-theme-border/40 text-xs text-theme-muted/90 font-sans mt-1">
                                                                            "🎵 Audio (" {*duration_secs} "s): " {path.clone()}
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

                                                let show_stats = active_ver.as_ref().map(|v| v.metadata.ttft_ms.is_some() || v.metadata.tokens_per_sec.is_some() || v.metadata.stop_reason.is_some()).unwrap_or(false);

                                                view! {
                                                    <div class="mt-2.5 flex flex-col gap-1.5 font-sans">
                                                        <Show when=move || show_stats>
                                                            {
                                                                let ver = active_ver.clone().unwrap();
                                                                let stop_reason_for_cond = ver.metadata.stop_reason.clone();
                                                                let stop_reason_for_text = ver.metadata.stop_reason.clone();
                                                                view! {
                                                                    <div class="flex flex-wrap items-center gap-4 mt-1.5 text-[10px] font-mono text-theme-muted select-none">
                                                                        <Show when=move || ver.metadata.tokens_per_sec.is_some()>
                                                                            <div class="flex items-center gap-1">
                                                                                <svg class="w-3 h-3 text-theme-muted" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 6V12h6a6 6 0 10-6-6z" />
                                                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 2a10 10 0 1010 10A10 10 0 0012 2zm0 18a8 8 0 118-8 8 8 0 01-8 8z" />
                                                                                </svg>
                                                                                <span>{format!("{:.1} t/s", ver.metadata.tokens_per_sec.unwrap_or(0.0))}</span>
                                                                            </div>
                                                                        </Show>

                                                                        <Show when=move || ver.metadata.ttft_ms.is_some()>
                                                                            <div class="flex items-center gap-1">
                                                                                <svg class="w-3 h-3 text-theme-muted" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                                                                                </svg>
                                                                                <span>{format!("{:.2}s TTFT", (ver.metadata.ttft_ms.unwrap_or(0) as f32) / 1000.0)}</span>
                                                                            </div>
                                                                        </Show>

                                                                        <Show when=move || {
                                                                            stop_reason_for_cond.as_deref()
                                                                                .map(|r| r != "stop" && r != "end_turn")
                                                                                .unwrap_or(false)
                                                                        }>
                                                                            {
                                                                                let sr_class = stop_reason_for_text.clone();
                                                                                let sr_icon = stop_reason_for_text.clone();
                                                                                let sr_label = stop_reason_for_text.clone();
                                                                                view! {
                                                                                    <div class={move || {
                                                                                        if sr_class.as_deref() == Some("cancelled") {
                                                                                            "flex items-center gap-1 text-amber-400/80".to_string()
                                                                                        } else {
                                                                                            "flex items-center gap-1".to_string()
                                                                                        }
                                                                                    }}>
                                                                                        <Show when=move || sr_icon.as_deref() == Some("cancelled")
                                                                                            fallback=|| view! {}
                                                                                        >
                                                                                            <svg class="w-3 h-3" fill="currentColor" viewBox="0 0 24 24">
                                                                                                <rect x="6" y="6" width="12" height="12" rx="1.5"/>
                                                                                            </svg>
                                                                                        </Show>
                                                                                        <span>{move || {
                                                                                            match sr_label.as_deref() {
                                                                                                Some("cancelled") => "Generation stopped".to_string(),
                                                                                                Some(r) => format!("Stop: {}", r),
                                                                                                None => String::new(),
                                                                                            }
                                                                                        }}</span>
                                                                                    </div>
                                                                                }
                                                                            }
                                                                        </Show>
                                                                    </div>
                                                                }
                                                            }
                                                        </Show>

                                                        <div class="flex items-center justify-between mt-1 text-theme-muted text-xs select-none">
                                                            <Show when=move || (total_versions > 1)>
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

                    // Loader when thinking is now handled in-message via ThinkingBlock
                </div>

                // Bottom Prompt Box / Input
                <footer class="p-3.5 border-t border-theme-border/60 bg-theme-bg shrink-0 theme-transition">
                                                    <form on:submit=send_message class="max-w-4xl mx-auto flex flex-col gap-2 bg-theme-panel border border-theme-border/80 rounded-2xl p-2.5 shadow-sm relative theme-transition">
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
                                placeholder={move || if is_streaming.get() { "AI is generating... Click stop to cancel." } else { "Type your prompt or drag/drop an image..." }}
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
                                type=move || if is_streaming.get() { "button" } else { "submit" }
                                id="send-btn-el"
                                on:click=move |ev| {
                                    if is_streaming.get_untracked() {
                                        ev.prevent_default();
                                        let convo_id = current_conversation_id.get_untracked();
                                        if let Some(cid) = convo_id {
                                            spawn_local(async move {
                                                let args = serde_wasm_bindgen::to_value(&CancelStreamArgs {
                                                    conversation_id: cid,
                                                }).unwrap();
                                                invoke("cancel_chat_stream", args).await;
                                            });
                                        }
                                    }
                                }
                                disabled=move || !is_streaming.get() && (input_text.get().trim().is_empty() && attached_image.get().is_none())
                                class="shrink-0 flex items-center justify-center p-2.5 rounded-xl bg-theme-accent text-theme-bg hover:opacity-95 transition-all shadow-md active:scale-[0.96] disabled:opacity-35 disabled:cursor-not-allowed theme-transition"
                            >
                                <Show
                                    when=move || is_streaming.get()
                                    fallback=move || view! {
                                        <svg class="w-5 h-5 rotate-90" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8"/>
                                        </svg>
                                    }
                                >
                                    <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                                        <rect x="6" y="6" width="12" height="12" rx="1.5" />
                                    </svg>
                                </Show>
                            </button>
                        </div>
                    </form>
                </footer>
            </main>

            // ─── CONNECTIONS SETTINGS MODAL ───
            <ConnectionsModal
                show_settings=show_settings
                set_show_settings=set_show_settings
                connections=connections
                set_connections=set_connections
                active_connection_id=active_connection_id
                set_active_connection_id=set_active_connection_id
                set_selected_provider=set_selected_provider
                set_selected_model=set_selected_model
            />

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
