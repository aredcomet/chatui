use leptos::prelude::*;
use leptos::ev::SubmitEvent;
use leptos::task::spawn_local;
use shared::{
    ChatConversation, ChatMessage, MessageRole, MessageVersion, MessageMetadata,
    ApiConfig, ContentBlock,
};
use crate::app::context::AppContext;
use crate::app::bindings::{
    invoke_raw, read_file_as_data_url, CancelStreamArgs, SaveConversationArgs, SendMessageStreamArgs,
};
use wasm_bindgen::JsCast;

#[component]
pub fn PromptInput() -> impl IntoView {
    let ctx = use_context::<AppContext>().expect("AppContext not found");

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
                                            ctx.set_attached_image.set(Some((mime, base64)));
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

    let update_input = move |ev: web_sys::Event| {
        let target = ev
            .target()
            .unwrap()
            .dyn_into::<web_sys::HtmlTextAreaElement>()
            .unwrap();
        ctx.set_input_text.set(target.value());
        let style = web_sys::HtmlElement::style(&target);
        let _ = style.set_property("height", "auto");
        let scroll_height = target.scroll_height();
        let _ = style.set_property("height", &format!("{}px", scroll_height));
    };

    let send_message = move |ev: SubmitEvent| {
        ev.prevent_default();
        if ctx.is_streaming.get_untracked() {
            return;
        }

        let text = ctx.input_text.get_untracked();
        let img = ctx.attached_image.get_untracked();

        if text.is_empty() && img.is_none() {
            return;
        }

        let active_id = match ctx.current_conversation_id.get_untracked() {
            Some(id) => id,
            None => {
                let uuid = uuid::Uuid::new_v4().to_string();
                let provider = ctx.selected_provider.get_untracked();
                let model = ctx.selected_model.get_untracked();
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
                    connection_id: ctx.active_connection_id.get_untracked(),
                };

                let mut current_convs = ctx.conversations.get_untracked();
                current_convs.insert(0, new_convo.clone());
                ctx.set_conversations.set(current_convs);

                ctx.set_current_conversation_id.set(Some(uuid.clone()));
                uuid
            }
        };

        ctx.set_input_text.set(String::new());
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            if let Some(el) = doc.get_element_by_id("prompt-input-box") {
                if let Ok(textarea) = el.dyn_into::<web_sys::HtmlTextAreaElement>() {
                    let _ = web_sys::HtmlElement::style(&textarea).set_property("height", "auto");
                }
            }
        }
        ctx.set_attached_image.set(None);
        ctx.set_is_streaming.set(true);

        let provider = ctx.selected_provider.get_untracked();
        let model = ctx.selected_model.get_untracked();
        let temp = ctx.temperature.get_untracked();
        let conn_id = ctx.active_connection_id.get_untracked();

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
                #[derive(serde::Serialize)]
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

            let mut active_msgs = ctx.messages.get_untracked();
            active_msgs.push(new_user_msg.clone());
            ctx.set_messages.set(active_msgs.clone());

            let args = serde_wasm_bindgen::to_value(&SendMessageStreamArgs {
                conversation_id: active_id_c.clone(),
                config: api_config,
                messages: active_msgs.clone(),
            })
            .unwrap();

            let stream_promise = invoke_raw("send_message_stream", args);
            let invoke_res = wasm_bindgen_futures::JsFuture::from(stream_promise).await;

            let mut convos = ctx.conversations.get_untracked();
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
            ctx.set_conversations.set(convos);

            if let Err(err) = invoke_res {
                let err_str = err
                    .as_string()
                    .unwrap_or_else(|| "Unknown connection error".to_string());
                ctx.set_is_streaming.set(false);
                let mut current_msgs = ctx.messages.get_untracked();
                current_msgs.push(ChatMessage::new_text(
                    MessageRole::Assistant,
                    format!("⚠️ Connection error: {}", err_str),
                ));
                ctx.set_messages.set(current_msgs);
            }
        });

        spawn_local(async move {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(el) = document.get_element_by_id("chat-messages-container") {
                        el.set_scroll_top(el.scroll_height());
                        ctx.user_scroll_pinned.set_value(false);
                    }
                }
            }
        });
    };

    view! {
        <footer class="p-3.5 border-t border-theme-border/60 bg-theme-bg shrink-0 theme-transition">
            <form on:submit=send_message class="max-w-4xl mx-auto flex flex-col gap-2 bg-theme-panel border border-theme-border/80 rounded-2xl p-2.5 shadow-sm relative theme-transition">
                // Attached Image Thumbnail Preview
                <Show
                    when=move || ctx.attached_image.get().is_some()
                    fallback=move || view! {}
                >
                    <div class="relative w-20 h-20 rounded-xl overflow-hidden border border-theme-border group">
                        <img
                            src={move || {
                                if let Some((mime, b64)) = ctx.attached_image.get() {
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
                            on:click=move |_| ctx.set_attached_image.set(None)
                            class="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-all text-white rounded-xl"
                        >
                            <svg class="w-5 h-5 hover:text-theme-destructive" fill="none" viewBox="0 0 24 24" stroke="currentColor">
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
                        placeholder={move || if ctx.is_streaming.get() { "AI is generating... Click stop to cancel." } else { "Type your prompt or drag/drop an image..." }}
                        prop:value=move || ctx.input_text.get()
                        on:input=update_input
                        disabled=move || ctx.is_streaming.get()
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
                        type=move || if ctx.is_streaming.get() { "button" } else { "submit" }
                        id="send-btn-el"
                        on:click=move |ev| {
                            if ctx.is_streaming.get_untracked() {
                                ev.prevent_default();
                                let convo_id = ctx.current_conversation_id.get_untracked();
                                if let Some(cid) = convo_id {
                                    spawn_local(async move {
                                        let args = serde_wasm_bindgen::to_value(&CancelStreamArgs {
                                            conversation_id: cid,
                                        }).unwrap();
                                        let _ = invoke_raw("cancel_chat_stream", args).await;
                                    });
                                }
                            }
                        }
                        disabled=move || !ctx.is_streaming.get() && (ctx.input_text.get().trim().is_empty() && ctx.attached_image.get().is_none())
                        class="shrink-0 flex items-center justify-center p-2.5 rounded-xl bg-theme-accent text-theme-bg hover:opacity-95 transition-all shadow-md active:scale-[0.96] disabled:opacity-35 disabled:cursor-not-allowed theme-transition"
                    >
                        <Show
                            when=move || ctx.is_streaming.get()
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
    }
}
