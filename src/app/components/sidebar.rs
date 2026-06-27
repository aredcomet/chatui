use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::ChatConversation;
use crate::app::context::AppContext;
use crate::app::bindings::{invoke, SaveConversationArgs, DeleteConversationArgs};

#[component]
pub fn Sidebar() -> impl IntoView {
    let ctx = use_context::<AppContext>().expect("AppContext not found");

    let select_conversation = move |id: String| {
        if ctx.is_streaming.get_untracked() {
            return;
        }
        ctx.set_current_conversation_id.set(Some(id.clone()));
        if let Some(convo) = ctx.conversations.get_untracked().iter().find(|c| c.id == id) {
            ctx.set_messages.set(convo.messages.clone());

            let conns = ctx.connections.get_untracked();
            let resolved_conn = if let Some(ref conn_id) = convo.connection_id {
                conns.iter().find(|c| &c.id == conn_id).cloned()
            } else {
                None
            };

            let resolved_conn = resolved_conn
                .or_else(|| conns.iter().find(|c| c.provider == convo.provider).cloned())
                .or_else(|| conns.first().cloned());

            if let Some(conn) = resolved_conn {
                ctx.set_active_connection_id.set(Some(conn.id.clone()));
                ctx.set_selected_provider.set(conn.provider);

                if !convo.model.is_empty() && conn.enabled_models.contains(&convo.model) {
                    ctx.set_selected_model.set(convo.model.clone());
                } else {
                    ctx.set_selected_model.set(conn.default_model.clone());
                }
            } else {
                ctx.set_active_connection_id.set(None);
                ctx.set_selected_provider.set(convo.provider);
                ctx.set_selected_model.set(convo.model.clone());
            }

            spawn_local(async move {
                if let Some(window) = web_sys::window() {
                    if let Some(document) = window.document() {
                        if let Some(el) = document.get_element_by_id("chat-messages-container") {
                            el.set_scroll_top(el.scroll_height());
                        }
                    }
                }
            });
        }
    };

    let create_new_chat = move |_| {
        if ctx.is_streaming.get_untracked() {
            return;
        }
        let uuid = uuid::Uuid::new_v4().to_string();

        let conns = ctx.connections.get_untracked();
        let conn_id = ctx.active_connection_id.get_untracked();
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

            ctx.set_active_connection_id.set(Some(cid.clone()));
            ctx.set_selected_provider.set(prov);
            ctx.set_selected_model.set(m.clone());

            (Some(cid), prov, m)
        } else {
            (
                None,
                ctx.selected_provider.get_untracked(),
                ctx.selected_model.get_untracked(),
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

        let mut current_convs = ctx.conversations.get_untracked();
        current_convs.insert(0, new_convo.clone());
        ctx.set_conversations.set(current_convs);

        ctx.set_current_conversation_id.set(Some(uuid.clone()));
        ctx.set_messages.set(Vec::new());

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
        if ctx.is_streaming.get_untracked() {
            return;
        }

        let mut current_convs = ctx.conversations.get_untracked();
        current_convs.retain(|c| c.id != id);
        ctx.set_conversations.set(current_convs);

        if ctx.current_conversation_id.get_untracked() == Some(id.clone()) {
            ctx.set_current_conversation_id.set(None);
            ctx.set_messages.set(Vec::new());
        }

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&DeleteConversationArgs { id }).unwrap();
            invoke("delete_conversation", args).await;
        });
    };

    let rename_chat = move |id: String, new_title: String| {
        if new_title.trim().is_empty() {
            return;
        }
        let mut current_convs = ctx.conversations.get_untracked();
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
        ctx.set_conversations.set(current_convs);
        ctx.set_editing_convo_id.set(None);
        ctx.set_editing_convo_title.set(String::new());
    };

    view! {
        <aside class="flex flex-col w-72 bg-theme-panel border-r border-theme-border/60 shrink-0 theme-transition">
            // Sidebar Header
            <div class="p-4 border-b border-theme-border/60">
                <button
                    on:click=create_new_chat
                    disabled=move || ctx.is_streaming.get()
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
                {move || ctx.conversations.get().iter().map(|convo| {
                    let id = convo.id.clone();
                    let title = convo.title.clone();
                    let active = ctx.current_conversation_id.get() == Some(id.clone());
                    let is_renaming = ctx.editing_convo_id.get() == Some(id.clone());
                    let active_class = if active {
                        "bg-theme-bg text-theme-text font-semibold border-l-2 border-theme-accent"
                    } else {
                        "text-theme-muted hover:bg-theme-bg/40 hover:text-theme-text border-l-2 border-transparent"
                    };
                    let active_trash_class = if active { "text-theme-muted hover:text-red-400" } else { "text-theme-muted/70 hover:text-red-400" };
                    let delete_btn_class = format!("p-1 rounded hover:bg-theme-bg/60 transition-all {}", active_trash_class);
                    let fade_to = if active { "to-theme-bg" } else { "to-theme-panel" };
                    let fade_class = format!("absolute right-0 top-0 h-full w-8 bg-gradient-to-r from-transparent {} pointer-events-none opacity-0 group-hover:opacity-100 transition-opacity", fade_to);
                    let btns_bg = if active { "bg-theme-bg" } else { "bg-theme-panel" };

                    let id_trash2 = id.clone();
                    let id_rename_btn = id.clone();
                    let id_rename_save = id.clone();
                    let id_rename_blur = id.clone();
                    let title_for_input = title.clone();

                    view! {
                        <div
                            on:click=move |_| {
                                if ctx.editing_convo_id.get_untracked().is_none() {
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
                                        <div class=fade_class.clone()></div>
                                    }
                                >
                                    <input
                                        type="text"
                                        class="w-full bg-theme-input border border-theme-accent/60 rounded-md px-1.5 py-0.5 text-sm text-theme-text outline-none focus:border-theme-accent"
                                        prop:value=move || ctx.editing_convo_title.get()
                                        on:input=move |ev| ctx.set_editing_convo_title.set(event_target_value(&ev))
                                        on:keydown={
                                            let id_s = id_rename_save.clone();
                                            move |ev: web_sys::KeyboardEvent| {
                                                ev.stop_propagation();
                                                match ev.key().as_str() {
                                                    "Enter" => rename_chat(id_s.clone(), ctx.editing_convo_title.get_untracked()),
                                                    "Escape" => { ctx.set_editing_convo_id.set(None); ctx.set_editing_convo_title.set(String::new()); }
                                                    _ => {}
                                                }
                                            }
                                        }
                                        on:blur={
                                            let id_b = id_rename_blur.clone();
                                            move |_| rename_chat(id_b.clone(), ctx.editing_convo_title.get_untracked())
                                        }
                                        on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()
                                    />
                                </Show>
                            </div>

                            // Action buttons
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
                                            ctx.set_editing_convo_id.set(Some(id_r.clone()));
                                            ctx.set_editing_convo_title.set(t.clone());
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

            // Sidebar Footer
            <div class="p-4 border-t border-theme-border/60 bg-theme-panel">
                <button
                    on:click=move |_| ctx.set_show_settings.set(true)
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
                        {move || ctx.connections.get().len()}
                    </span>
                </button>
            </div>
        </aside>
    }
}
