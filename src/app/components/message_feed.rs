use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{
    MessageRole, ContentBlock, MessageVersion, MessageMetadata,
    ApiConfig,
};
use crate::app::context::AppContext;
use crate::app::bindings::{
    invoke, invoke_raw, convert_file_src, SaveConversationArgs, SendMessageStreamArgs,
};
use crate::app::markdown::{render_message_content, parse_thinking_content};
use crate::app::components::thinking_block::ThinkingBlock;
use wasm_bindgen::JsCast;

#[component]
#[allow(unused_parens)]
pub fn MessageFeed() -> impl IntoView {
    let ctx = use_context::<AppContext>().expect("AppContext not found");

    let show_toast = move |msg: String| {
        ctx.set_toast_message.set(Some(msg));
        spawn_local(async move {
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                if let Some(w) = web_sys::window() {
                    let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 2000);
                }
            });
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            ctx.set_toast_message.set(None);
        });
    };

    let copy_message_text = move |text: String| {
        if let Some(window) = web_sys::window() {
            let navigator = window.navigator();
            let clipboard = navigator.clipboard();
            let _ = clipboard.write_text(&text);
            show_toast("Copied to clipboard!".to_string());
        }
    };

    let delete_message = move |idx: usize| {
        if ctx.is_streaming.get_untracked() {
            return;
        }
        let mut current_msgs = ctx.messages.get_untracked();
        if idx < current_msgs.len() {
            current_msgs.remove(idx);
            ctx.set_messages.set(current_msgs.clone());

            if let Some(convo_id) = ctx.current_conversation_id.get_untracked() {
                let mut convos = ctx.conversations.get_untracked();
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
        let mut current_msgs = ctx.messages.get_untracked();
        if let Some(msg) = current_msgs.get_mut(idx) {
            let new_text = ctx.editing_message_text.get_untracked();

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

            ctx.set_messages.set(current_msgs.clone());
            ctx.set_editing_message_idx.set(None);

            if let Some(convo_id) = ctx.current_conversation_id.get_untracked() {
                let mut convos = ctx.conversations.get_untracked();
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
        let mut current_msgs = ctx.messages.get_untracked();
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
            ctx.set_messages.set(current_msgs.clone());

            if let Some(convo_id) = ctx.current_conversation_id.get_untracked() {
                let mut convos = ctx.conversations.get_untracked();
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
        if ctx.is_streaming.get_untracked() {
            return;
        }
        let current_convo_id = ctx.current_conversation_id.get_untracked();
        if let Some(convo_id) = current_convo_id {
            let mut convos = ctx.conversations.get_untracked();
            if let Some(convo) = convos.iter().find(|c| c.id == convo_id) {
                let uuid = uuid::Uuid::new_v4().to_string();
                let branched_messages = convo.messages[..=idx].to_vec();

                let new_convo = shared::ChatConversation {
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
                ctx.set_conversations.set(convos);

                // Select the branched conversation
                ctx.set_current_conversation_id.set(Some(uuid.clone()));
                ctx.set_messages.set(branched_messages);

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
        if ctx.is_streaming.get_untracked() {
            return;
        }
        let mut current_msgs = ctx.messages.get_untracked();
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
                model: ctx.selected_model.get_untracked(),
                provider: ctx.selected_provider.get_untracked(),
                connection_id: ctx.active_connection_id.get_untracked().unwrap_or_default(),
                created_at: chrono::Utc::now(),
                ttft_ms: None,
                tokens_per_sec: None,
                stop_reason: None,
            },
        };
        current_msgs[last_idx].versions.push(new_version);
        let new_active = current_msgs[last_idx].versions.len() - 1;
        current_msgs[last_idx].active_version = new_active;

        ctx.set_messages.set(current_msgs.clone());
        ctx.set_is_streaming.set(true);

        let messages_history = current_msgs[..last_idx].to_vec();

        let active_id = match ctx.current_conversation_id.get_untracked() {
            Some(id) => id,
            None => return,
        };

        let provider = ctx.selected_provider.get_untracked();
        let model = ctx.selected_model.get_untracked();
        let temp = ctx.temperature.get_untracked();
        let conn_id = ctx.active_connection_id.get_untracked();

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
        if ctx.is_streaming.get_untracked() {
            return;
        }
        let current_msgs = ctx.messages.get_untracked();
        if current_msgs.is_empty() {
            return;
        }

        let last_idx = current_msgs.len() - 1;
        if current_msgs[last_idx].role != MessageRole::Assistant {
            return;
        }

        ctx.set_is_streaming.set(true);

        let active_id = match ctx.current_conversation_id.get_untracked() {
            Some(id) => id,
            None => return,
        };

        let provider = ctx.selected_provider.get_untracked();
        let model = ctx.selected_model.get_untracked();
        let temp = ctx.temperature.get_untracked();
        let conn_id = ctx.active_connection_id.get_untracked();

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



    view! {
        <div
            id="chat-messages-container"
            class="flex-1 overflow-y-auto px-4 py-4 space-y-4 scrollbar-thin scroll-smooth"
        >
            <Show
                when=move || !ctx.messages.get().is_empty()
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
                {move || ctx.messages.get().into_iter().enumerate().map(|(idx, msg)| {
                    let is_user = msg.role == MessageRole::User;
                    let is_editing = ctx.editing_message_idx.get() == Some(idx);
                    let is_last_msg = idx == ctx.messages.get().len() - 1;

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
                                                    prop:value=ctx.editing_message_text
                                                    on:input=move |ev: web_sys::Event| {
                                                        let target = ev.target().unwrap().dyn_into::<web_sys::HtmlTextAreaElement>().unwrap();
                                                        ctx.set_editing_message_text.set(target.value());
                                                        let style = web_sys::HtmlElement::style(&target);
                                                        let _ = style.set_property("height", "auto");
                                                        let _ = style.set_property("height", &format!("{}px", target.scroll_height()));
                                                    }
                                                />
                                                <div class="flex items-center gap-2 justify-end">
                                                    <button
                                                        type="button"
                                                        class="px-3 py-1.5 rounded-lg text-xs font-semibold bg-theme-panel hover:bg-theme-border/80 text-theme-text border border-theme-border/60 transition-colors theme-transition"
                                                        on:click=move |_| ctx.set_editing_message_idx.set(None)
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
                                            <button
                                                type="button"
                                                title="Edit message"
                                                class="p-1.5 rounded-lg hover:bg-theme-bg hover:text-theme-text text-theme-muted transition-colors disabled:opacity-30 theme-transition"
                                                disabled=move || ctx.is_streaming.get()
                                                on:click={
                                                    let text = msg.get_text();
                                                    move |_| {
                                                        ctx.set_editing_message_idx.set(Some(idx));
                                                        ctx.set_editing_message_text.set(text.clone());
                                                    }
                                                }
                                            >
                                                <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                                                </svg>
                                            </button>
                                            <button
                                                type="button"
                                                title="Delete message"
                                                class="p-1.5 rounded-lg hover:bg-theme-bg hover:text-theme-destructive text-theme-muted transition-colors disabled:opacity-30 theme-transition"
                                                disabled=move || ctx.is_streaming.get()
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
                                                                        fallback=|| view! {}
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
                                                            let is_thinking_active = ctx.is_streaming.get() && idx == ctx.messages.get().len() - 1;
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
                                                prop:value=ctx.editing_message_text
                                                on:input=move |ev: web_sys::Event| {
                                                    let target = ev.target().unwrap().dyn_into::<web_sys::HtmlTextAreaElement>().unwrap();
                                                    ctx.set_editing_message_text.set(target.value());
                                                    let style = web_sys::HtmlElement::style(&target);
                                                    let _ = style.set_property("height", "auto");
                                                    let _ = style.set_property("height", &format!("{}px", target.scroll_height()));
                                                }
                                            />
                                            <div class="flex items-center gap-2 justify-end">
                                                <button
                                                    type="button"
                                                    class="px-3 py-1.5 rounded-lg text-xs font-semibold bg-theme-panel hover:bg-theme-border/80 text-theme-text border border-theme-border/60 transition-colors theme-transition"
                                                    on:click=move |_| ctx.set_editing_message_idx.set(None)
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
                                                                                    "flex items-center gap-1 text-theme-warning/80".to_string()
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
                                                    <Show when=move || total_versions.gt(&1)>
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
                                                                disabled=move || ctx.is_streaming.get()
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
                                                                disabled=move || ctx.is_streaming.get()
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
                                                            disabled=move || ctx.is_streaming.get()
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
                                                            disabled=move || ctx.is_streaming.get()
                                                            on:click={
                                                                let text = msg.get_text();
                                                                move |_| {
                                                                    ctx.set_editing_message_idx.set(Some(idx));
                                                                    ctx.set_editing_message_text.set(text.clone());
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
                                                            class="p-1.5 rounded-lg hover:bg-theme-panel hover:text-theme-destructive text-theme-muted transition-colors disabled:opacity-30 theme-transition"
                                                            disabled=move || ctx.is_streaming.get()
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
        </div>
    }
}
