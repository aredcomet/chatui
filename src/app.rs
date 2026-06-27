use leptos::task::spawn_local;
use leptos::prelude::*;
use shared::{
    ChatConversation, ChatMessage, Connection, ContentBlock, MessageMetadata,
    MessageRole, MessageVersion, Provider, StreamPayload,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

// Submodules
pub mod bindings;
pub mod components;
pub mod markdown;
pub mod theme;
pub mod context;

use bindings::*;
use components::connections_modal::ConnectionsModal;
use components::sidebar::Sidebar;
use components::selector_header::SelectorHeader;
use components::message_feed::MessageFeed;
use components::prompt_input::PromptInput;
use markdown::*;
use theme::*;
use context::AppContext;

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

    // True when the user has intentionally scrolled away from the bottom during streaming.
    let user_scroll_pinned = StoredValue::new(false);

    // Hierarchical chat tree signals
    let (chat_tree, set_chat_tree) = signal(Vec::<ChatTreeNode>::new());
    let (sort_alphabetical, set_sort_alphabetical) = signal(false);
    let (expanded_folders, set_expanded_folders) = signal(std::collections::HashSet::<String>::new());

    // Context sharing
    let ctx = AppContext {
        conversations,
        set_conversations,
        current_conversation_id,
        set_current_conversation_id,
        messages,
        set_messages,
        input_text,
        set_input_text,
        attached_image,
        set_attached_image,
        editing_convo_id,
        set_editing_convo_id,
        editing_convo_title,
        set_editing_convo_title,
        app_theme,
        set_app_theme,
        connections,
        set_connections,
        active_connection_id,
        set_active_connection_id,
        selected_provider,
        set_selected_provider,
        selected_model,
        set_selected_model,
        temperature,
        set_temperature,
        show_settings,
        set_show_settings,
        is_streaming,
        set_is_streaming,
        editing_message_idx,
        set_editing_message_idx,
        editing_message_text,
        set_editing_message_text,
        toast_message,
        set_toast_message,
        user_scroll_pinned,
        chat_tree,
        set_chat_tree,
        sort_alphabetical,
        set_sort_alphabetical,
        expanded_folders,
        set_expanded_folders,
    };
    provide_context(ctx);

    // Raw pixel helper — only used internally by the scroll event handler
    let is_at_bottom_raw = || -> bool {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(el) = document.get_element_by_id("chat-messages-container") {
                    let scroll_top = el.scroll_top();
                    let scroll_height = el.scroll_height();
                    let client_height = el.client_height();
                    return scroll_height - scroll_top - client_height < 10;
                }
            }
        }
        true
    };

    let is_scroll_at_bottom = move || -> bool { !user_scroll_pinned.get_value() };

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

            // Fetch settings (expanded folders, sort_alphabetical, etc.)
            let res_settings = invoke(
                "get_settings",
                serde_wasm_bindgen::to_value(&()).unwrap(),
            )
            .await;
            if let Ok(settings) = serde_wasm_bindgen::from_value::<AppSettings>(res_settings) {
                set_sort_alphabetical.set(settings.sort_alphabetical);
                let set: std::collections::HashSet<String> = settings.expanded_folders.into_iter().collect();
                set_expanded_folders.set(set);
            }

            // Fetch conversations
            let args = serde_wasm_bindgen::to_value(&()).unwrap();
            let res_convs = invoke("load_conversations", args).await;
            if let Ok(convs) = serde_wasm_bindgen::from_value::<Vec<ChatConversation>>(res_convs) {
                set_conversations.set(convs);
            }

            // Fetch chat tree
            let res_tree = invoke("get_chat_tree", serde_wasm_bindgen::to_value(&()).unwrap()).await;
            if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<ChatTreeNode>>(res_tree) {
                set_chat_tree.set(tree);
            }
        });
    };

    // Save changes to settings dynamically
    Effect::new(move |_| {
        let sort = sort_alphabetical.get();
        let expanded = expanded_folders.get();
        
        spawn_local(async move {
            let res_settings = invoke("get_settings", serde_wasm_bindgen::to_value(&()).unwrap()).await;
            let mut current = serde_wasm_bindgen::from_value::<AppSettings>(res_settings).unwrap_or_default();
            
            let expanded_vec: Vec<String> = expanded.into_iter().collect();
            if current.sort_alphabetical != sort || current.expanded_folders != expanded_vec {
                current.sort_alphabetical = sort;
                current.expanded_folders = expanded_vec;
                let args = serde_wasm_bindgen::to_value(&current).unwrap();
                let _ = invoke("save_settings", args).await;
            }
        });
    });

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
                75, // update every 75ms
            );
        }
        cb.forget();
    });

    // Effects for mounting and listening
    Effect::new(move |_| {
        load_init_data();

        let handler = Closure::wrap(Box::new(move |event_obj: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event_obj, &JsValue::from_str("payload")) {
                if let Ok(payload_struct) = serde_wasm_bindgen::from_value::<StreamPayload>(payload) {
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
                user_scroll_pinned.set_value(false);
            } else {
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

                    let mut current_msgs = pending_messages
                        .get_value()
                        .unwrap_or_else(|| messages.get_untracked());
                    pending_messages.set_value(None);

                    if let Some(last_msg) = current_msgs.last_mut() {
                        if last_msg.role == MessageRole::Assistant {
                            if let Some(version) = last_msg.versions.get_mut(last_msg.active_version) {
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
                    let mut current_msgs = pending_messages
                        .get_value()
                        .unwrap_or_else(|| messages.get_untracked());

                    if let Some(last_msg) = current_msgs.last_mut() {
                        if last_msg.role == MessageRole::Assistant {
                            if let Some(version) = last_msg.versions.get_mut(last_msg.active_version) {
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

    let handle_drag_over = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
    };

    let handle_drop = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        if is_streaming.get_untracked() {
            return;
        }

        if let Some(dt) = ev.data_transfer() {
            if let Some(files) = dt.files() {
                if files.length() > 0 {
                    if let Some(file) = files.get(0) {
                        let file_type = file.type_();
                        if file_type.starts_with("image/") {
                            spawn_local(async move {
                                if let Ok(promise) = read_file_as_data_url(&file) {
                                    if let Ok(res) = wasm_bindgen_futures::JsFuture::from(promise).await {
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
        }
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
            <Sidebar />

            <main class="flex-1 flex flex-col min-w-0 bg-theme-bg h-full relative theme-transition">
                <SelectorHeader />
                <MessageFeed />
                <PromptInput />
            </main>

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
