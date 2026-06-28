use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::ChatConversation;
use std::path::Path;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use crate::app::context::{AppContext, InlineCreationTarget};
use crate::app::bindings::{
    invoke, invoke_result, SaveConversationArgs, DeleteConversationArgs,
    ChatTreeNode, CreateFolderArgs, MoveItemArgs, DeleteFolderRecursiveArgs,
};

#[derive(Clone, Debug, PartialEq)]
enum ContextMenuTarget {
    Root,
    Folder { path: String, name: String },
    Chat { id: String, title: String, path: String },
}

#[derive(Clone, Debug)]
struct ContextMenuState {
    visible: bool,
    x: f64,
    y: f64,
    target: ContextMenuTarget,
}

#[derive(Clone, Debug, PartialEq)]
enum ModalAction {
    None,
    RenameFolder { path: String, current_name: String },
    RenameChat { id: String, current_title: String, path: String },
    DeleteChat { id: String, name: String },
    DeleteFolder { path: String, name: String },
}

#[component]
#[allow(unused_parens)]
pub fn Sidebar() -> impl IntoView {
    let ctx = use_context::<AppContext>().expect("AppContext not found");

    // UI state
    let (context_menu, set_context_menu) = signal(ContextMenuState {
        visible: false,
        x: 0.0,
        y: 0.0,
        target: ContextMenuTarget::Root,
    });
    let (modal_action, set_modal_action) = signal(ModalAction::None);
    let (modal_input, set_modal_input) = signal(String::new());

    // Hide context menu on click anywhere
    Effect::new(move |_| {
        let handle_click = Closure::wrap(Box::new(move || {
            set_context_menu.set(ContextMenuState {
                visible: false,
                x: 0.0,
                y: 0.0,
                target: ContextMenuTarget::Root,
            });
        }) as Box<dyn FnMut()>);
        
        if let Some(window) = web_sys::window() {
            let _ = window.add_event_listener_with_callback("click", handle_click.as_ref().unchecked_ref());
        }
        handle_click.forget();
    });

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

    let create_chat_in_folder_with_title = move |folder_path: Option<String>, title: String| {
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
            title,
            model,
            provider,
            created_at: chrono::Utc::now(),
            folder_id: folder_path.clone(),
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

        let set_chat_tree_c = ctx.set_chat_tree;
        let new_convo_c = new_convo.clone();
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                conversation: new_convo_c,
            })
            .unwrap();
            if invoke_result("save_conversation", args).await.is_ok() {
                let res_tree = invoke("get_chat_tree", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<ChatTreeNode>>(res_tree) {
                    set_chat_tree_c.set(tree);
                }
            }
        });
    };

    let duplicate_chat = move |id: String, current_title: String, _path: String| {
        if let Some(convo) = ctx.conversations.get_untracked().iter().find(|c| c.id == id) {
            let mut duplicate = convo.clone();
            duplicate.id = uuid::Uuid::new_v4().to_string();
            duplicate.title = format!("{} Copy", current_title);
            duplicate.updated_at = js_sys::Date::now() as u64;

            let dup_c = duplicate.clone();
            let set_chat_tree_c = ctx.set_chat_tree;
            let set_conversations_c = ctx.set_conversations;
            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                    conversation: dup_c,
                }).unwrap();
                if invoke_result("save_conversation", args).await.is_ok() {
                    let res_tree = invoke("get_chat_tree", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                    if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<ChatTreeNode>>(res_tree) {
                        set_chat_tree_c.set(tree);
                    }
                    let res_convs = invoke("load_conversations", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                    if let Ok(convs) = serde_wasm_bindgen::from_value::<Vec<ChatConversation>>(res_convs) {
                        set_conversations_c.set(convs);
                    }
                }
            });
        }
    };

    let commit_inline_creation = move || {
        let state = ctx.inline_creation.get_untracked();
        if state == InlineCreationTarget::None {
            return;
        }
        let input_val = ctx.inline_input_text.get_untracked();

        ctx.set_inline_creation.set(InlineCreationTarget::None);
        ctx.set_inline_input_text.set(String::new());

        if input_val.trim().is_empty() {
            return;
        }

        let title = input_val.trim().to_string();

        match state {
            InlineCreationTarget::Chat { parent_path } => {
                create_chat_in_folder_with_title(parent_path, title);
            }
            InlineCreationTarget::Folder { parent_path } => {
                let relative_path = if let Some(parent) = parent_path {
                    Path::new(&parent).join(&title).to_string_lossy().to_string()
                } else {
                    title.clone()
                };

                let set_chat_tree_c = ctx.set_chat_tree;
                spawn_local(async move {
                    let args = serde_wasm_bindgen::to_value(&CreateFolderArgs {
                        relative_path,
                    }).unwrap();
                    if invoke_result("create_folder", args).await.is_ok() {
                        let res_tree = invoke("get_chat_tree", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                        if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<ChatTreeNode>>(res_tree) {
                            set_chat_tree_c.set(tree);
                        }
                    }
                });
            }
            InlineCreationTarget::None => {}
        }
    };

    let execute_modal_action = move |_| {
        let action = modal_action.get_untracked();
        let input_val = modal_input.get_untracked();
        let set_chat_tree_c = ctx.set_chat_tree;
        let set_conversations_c = ctx.set_conversations;

        match action {
            ModalAction::RenameFolder { path, current_name } => {
                if input_val.trim().is_empty() || input_val.trim() == current_name {
                    set_modal_action.set(ModalAction::None);
                    return;
                }

                let source_path = path.clone();
                let parent = Path::new(&path).parent().unwrap_or(Path::new(""));
                let dest_path = parent.join(input_val.trim()).to_string_lossy().to_string();

                spawn_local(async move {
                    let args = serde_wasm_bindgen::to_value(&MoveItemArgs {
                        source_rel: source_path,
                        dest_rel: dest_path,
                    }).unwrap();
                    if invoke_result("move_item", args).await.is_ok() {
                        let res_tree = invoke("get_chat_tree", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                        if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<ChatTreeNode>>(res_tree) {
                            set_chat_tree_c.set(tree);
                        }
                        let res_convs = invoke("load_conversations", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                        if let Ok(convs) = serde_wasm_bindgen::from_value::<Vec<ChatConversation>>(res_convs) {
                            set_conversations_c.set(convs);
                        }
                    }
                });
            }
            ModalAction::RenameChat { id, current_title: _, path: _ } => {
                if input_val.trim().is_empty() {
                    return;
                }

                let mut current_convs = ctx.conversations.get_untracked();
                if let Some(convo) = current_convs.iter_mut().find(|c| c.id == id) {
                    convo.title = input_val.trim().to_string();
                    let convo_clone = convo.clone();
                    spawn_local(async move {
                        let args = serde_wasm_bindgen::to_value(&SaveConversationArgs {
                            conversation: convo_clone,
                        })
                        .unwrap();
                        if invoke_result("save_conversation", args).await.is_ok() {
                            let res_tree = invoke("get_chat_tree", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                            if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<ChatTreeNode>>(res_tree) {
                                set_chat_tree_c.set(tree);
                            }
                        }
                    });
                }
                ctx.set_conversations.set(current_convs);
            }
            ModalAction::DeleteChat { id, name: _ } => {
                let mut current_convs = ctx.conversations.get_untracked();
                current_convs.retain(|c| c.id != id);
                ctx.set_conversations.set(current_convs);

                if ctx.current_conversation_id.get_untracked() == Some(id.clone()) {
                    ctx.set_current_conversation_id.set(None);
                    ctx.set_messages.set(Vec::new());
                }

                spawn_local(async move {
                    let args = serde_wasm_bindgen::to_value(&DeleteConversationArgs { id }).unwrap();
                    if invoke_result("delete_conversation", args).await.is_ok() {
                        let res_tree = invoke("get_chat_tree", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                        if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<ChatTreeNode>>(res_tree) {
                            set_chat_tree_c.set(tree);
                        }
                    }
                });
            }
            ModalAction::DeleteFolder { path, name: _ } => {
                let path_c = path.clone();
                spawn_local(async move {
                    let args = serde_wasm_bindgen::to_value(&DeleteFolderRecursiveArgs {
                        relative_path: path_c,
                    }).unwrap();
                    if invoke_result("delete_folder_recursive", args).await.is_ok() {
                        let res_convs = invoke("load_conversations", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                        if let Ok(convs) = serde_wasm_bindgen::from_value::<Vec<ChatConversation>>(res_convs) {
                            set_conversations_c.set(convs.clone());
                            let active_id = ctx.current_conversation_id.get_untracked();
                            if let Some(ref aid) = active_id {
                                if !convs.iter().any(|c| &c.id == aid) {
                                    ctx.set_current_conversation_id.set(None);
                                    ctx.set_messages.set(Vec::new());
                                }
                            }
                        }

                        let res_tree = invoke("get_chat_tree", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                        if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<ChatTreeNode>>(res_tree) {
                            set_chat_tree_c.set(tree);
                        }
                    }
                });
            }
            ModalAction::None => {}
        }

        set_modal_action.set(ModalAction::None);
        set_modal_input.set(String::new());
    };

    let sorted_root_tree = move || {
        let mut list = ctx.chat_tree.get();
        if ctx.sort_alphabetical.get() {
            list.sort_by(|a, b| {
                if a.is_dir != b.is_dir {
                    b.is_dir.cmp(&a.is_dir)
                } else {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                }
            });
        } else {
            list.sort_by(|a, b| {
                if a.is_dir != b.is_dir {
                    b.is_dir.cmp(&a.is_dir)
                } else if a.is_dir {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                } else {
                    let time_a = a.updated_at.unwrap_or(0);
                    let time_b = b.updated_at.unwrap_or(0);
                    time_b.cmp(&time_a)
                }
            });
        }
        list
    };

    // Global dragover/drop zones for empty sidebar space
    let handle_root_dragover = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
    };

    let handle_root_drop = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        let set_chat_tree_c = ctx.set_chat_tree;
        let set_conversations_c = ctx.set_conversations;
        if let Some(dt) = ev.data_transfer() {
            if let Ok(data) = dt.get_data("text/plain") {
                let parts: Vec<&str> = data.split('|').collect();
                if parts.len() == 2 {
                    let src_path = parts[0].to_string();
                    let src_name = Path::new(&src_path).file_name().unwrap_or_default().to_string_lossy().to_string();
                    let dest_path = src_name;

                    if src_path != dest_path {
                        let src_c = src_path.clone();
                        let dest_c = dest_path.clone();
                        spawn_local(async move {
                            let args = serde_wasm_bindgen::to_value(&MoveItemArgs {
                                source_rel: src_c,
                                dest_rel: dest_c,
                            }).unwrap();
                            if invoke_result("move_item", args).await.is_ok() {
                                let res_tree = invoke("get_chat_tree", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                                if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<ChatTreeNode>>(res_tree) {
                                    set_chat_tree_c.set(tree);
                                }
                                let res_convs = invoke("load_conversations", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                                if let Ok(convs) = serde_wasm_bindgen::from_value::<Vec<ChatConversation>>(res_convs) {
                                    set_conversations_c.set(convs);
                                }
                            }
                        });
                    }
                }
            }
        }
    };

    // Right click menu toggle
    let handle_root_contextmenu = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        set_context_menu.set(ContextMenuState {
            visible: true,
            x: ev.client_x() as f64,
            y: ev.client_y() as f64,
            target: ContextMenuTarget::Root,
        });
    };

    let on_context_menu_cb = move |(ev, target): (web_sys::MouseEvent, ContextMenuTarget)| {
        set_context_menu.set(ContextMenuState {
            visible: true,
            x: ev.client_x() as f64,
            y: ev.client_y() as f64,
            target,
        });
    };

    let on_drag_start_cb = move |(ev, path, is_dir): (web_sys::DragEvent, String, bool)| {
        if let Some(dt) = ev.data_transfer() {
            let _ = dt.set_data("text/plain", &format!("{}|{}", path, is_dir));
            dt.set_effect_allowed("move");
        }
    };

    // Root-level inline inputs
    let view_inline_chat_input = move |_: Option<String>| {
        let input_ref = NodeRef::<leptos::html::Input>::new();
        Effect::new(move |_| {
            if let Some(el) = input_ref.get() {
                let _ = el.focus();
            }
        });
        view! {
            <div
                style="padding-left: 0.5rem;"
                class="flex items-center gap-2 py-1.5 pr-2 rounded-lg bg-theme-bg/40 text-sm border border-theme-accent/50 animate-scale-in"
            >
                <span class="text-theme-muted/50 shrink-0">
                    <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
                    </svg>
                </span>
                <input
                    type="text"
                    class="w-full bg-transparent text-theme-text outline-none text-sm"
                    placeholder="Chat name..."
                    node_ref=input_ref
                    prop:value=ctx.inline_input_text
                    on:input=move |ev| ctx.set_inline_input_text.set(event_target_value(&ev))
                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                        if ev.key() == "Enter" {
                            ev.prevent_default();
                            ev.stop_propagation();
                            if let Some(target) = ev.target() {
                                if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                                    let _ = input.blur();
                                }
                            }
                        } else if ev.key() == "Escape" {
                            ev.prevent_default();
                            ev.stop_propagation();
                            ctx.set_inline_creation.set(InlineCreationTarget::None);
                            if let Some(target) = ev.target() {
                                if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                                    let _ = input.blur();
                                }
                            }
                        }
                    }
                    on:blur=move |_| {
                        commit_inline_creation();
                    }
                />
            </div>
        }
    };

    let view_inline_folder_input = move |_: Option<String>| {
        let input_ref = NodeRef::<leptos::html::Input>::new();
        Effect::new(move |_| {
            if let Some(el) = input_ref.get() {
                let _ = el.focus();
            }
        });
        view! {
            <div
                style="padding-left: 0.5rem;"
                class="flex items-center gap-1.5 py-1.5 pr-2 rounded-lg bg-theme-bg/40 text-sm border border-theme-accent/50 animate-scale-in"
            >
                <span class="text-theme-muted/80">
                    <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
                    </svg>
                </span>
                <span class="text-theme-accent/80 shrink-0">
                    <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                    </svg>
                </span>
                <input
                    type="text"
                    class="w-full bg-transparent text-theme-text outline-none text-sm"
                    placeholder="Folder name..."
                    node_ref=input_ref
                    prop:value=ctx.inline_input_text
                    on:input=move |ev| ctx.set_inline_input_text.set(event_target_value(&ev))
                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                        if ev.key() == "Enter" {
                            ev.prevent_default();
                            ev.stop_propagation();
                            if let Some(target) = ev.target() {
                                if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                                    let _ = input.blur();
                                }
                            }
                        } else if ev.key() == "Escape" {
                            ev.prevent_default();
                            ev.stop_propagation();
                            ctx.set_inline_creation.set(InlineCreationTarget::None);
                            if let Some(target) = ev.target() {
                                if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                                    let _ = input.blur();
                                }
                            }
                        }
                    }
                    on:blur=move |_| {
                        commit_inline_creation();
                    }
                />
            </div>
        }
    };

    view! {
        <aside
            on:contextmenu=handle_root_contextmenu
            class="flex flex-col w-72 bg-theme-panel border-r border-theme-border/60 shrink-0 theme-transition relative select-none"
        >
            // Sidebar Header
            <div class="p-4 border-b border-theme-border/60 flex flex-col gap-2.5">
                <button
                    on:click=move |_| {
                        ctx.set_inline_creation.set(InlineCreationTarget::Chat { parent_path: None });
                        ctx.set_inline_input_text.set(String::new());
                    }
                    disabled=move || ctx.is_streaming.get()
                    class="w-full flex items-center justify-center gap-2 py-2 px-4 rounded-xl border border-theme-border bg-theme-bg/60 text-theme-text font-medium hover:bg-theme-bg hover:border-theme-accent/50 transition-all active:scale-[0.98] disabled:opacity-40 disabled:cursor-not-allowed theme-transition"
                >
                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"/>
                    </svg>
                    "New chat"
                </button>

                // Sidebar Toolbar
                <div class="flex items-center justify-between text-xs text-theme-muted">
                    <div class="flex items-center gap-1.5">
                        <button
                            on:click=move |_| {
                                ctx.set_inline_creation.set(InlineCreationTarget::Folder { parent_path: None });
                                ctx.set_inline_input_text.set(String::new());
                            }
                            class="p-1 rounded hover:bg-theme-bg hover:text-theme-text transition-colors"
                            title="New Folder"
                        >
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M9 13h6m-3-3v6m-9 1V7a2 2 0 012-2h6l2 2h6a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z" />
                            </svg>
                        </button>
                        <button
                            on:click=move |_| {
                                ctx.set_expanded_folders.set(std::collections::HashSet::new());
                            }
                            class="p-1 rounded hover:bg-theme-bg hover:text-theme-text transition-colors"
                            title="Collapse All"
                        >
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5v-4m0 4h-4m4 0l-5-5" />
                            </svg>
                        </button>
                    </div>

                    <button
                        on:click=move |_| {
                            ctx.set_sort_alphabetical.update(|val| *val = !*val);
                        }
                        class="flex items-center gap-1 px-1.5 py-0.5 rounded hover:bg-theme-bg hover:text-theme-text transition-colors"
                        title="Toggle sorting order"
                    >
                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M3 4h13M3 8h9m-9 4h6m4 0l4-4m0 0l4 4m-4-4v12" />
                        </svg>
                        <span>
                            {move || if ctx.sort_alphabetical.get() { "A-Z" } else { "Recent" }}
                        </span>
                    </button>
                </div>
            </div>

            // Directory and Chat Tree List Zone
            <div
                on:dragover=handle_root_dragover
                on:drop=handle_root_drop
                class="flex-1 overflow-y-auto px-2 py-3 space-y-1.5 scrollbar-thin select-none"
            >
                <Show
                    when=move || !ctx.chat_tree.get().is_empty() || ctx.inline_creation.get() != InlineCreationTarget::None
                    fallback=move || view! {
                        <div class="flex flex-col items-center justify-center h-full text-center p-4 text-theme-muted/60 select-none">
                            <svg class="w-8 h-8 mb-2 opacity-50" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
                            </svg>
                            <span class="text-xs">"Right-click or click '+' above to create chats & folders"</span>
                        </div>
                    }
                >
                    {move || {
                        let list = sorted_root_tree();
                        let mut views: Vec<AnyView> = list.into_iter().map(|node| {
                            let on_sel = Callback::new(move |id| select_conversation(id));
                            let on_ctx = Callback::new(on_context_menu_cb);
                            let on_drag = Callback::new(on_drag_start_cb);
                            let on_comm = Callback::new(move |_| commit_inline_creation());
                            view! {
                                <FolderNode
                                    node=node
                                    depth=0
                                    on_select_chat=on_sel
                                    on_context_menu=on_ctx
                                    on_drag_start=on_drag
                                    on_commit_creation=on_comm
                                />
                            }.into_any()
                        }).collect();

                        let inline_state = ctx.inline_creation.get();
                        match inline_state {
                            InlineCreationTarget::Chat { parent_path: None } => {
                                views.insert(0, view_inline_chat_input(None).into_any());
                            }
                            InlineCreationTarget::Folder { parent_path: None } => {
                                views.insert(0, view_inline_folder_input(None).into_any());
                            }
                            _ => {}
                        }
                        views
                    }}
                </Show>
            </div>

            // Sidebar Settings Footer Link
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

            // ─── CUSTOM RIGHT-CLICK CONTEXT MENU ───
            <Show when=move || context_menu.get().visible>
                <div
                    style=move || {
                        let state = context_menu.get();
                        format!("position: fixed; left: {}px; top: {}px; z-index: 1000;", state.x, state.y)
                    }
                    class="w-48 bg-theme-panel border border-theme-border/80 rounded-xl shadow-2xl p-1.5 text-xs text-theme-muted font-sans select-none animation-fade-in theme-transition"
                >
                    {move || {
                        let target = context_menu.get().target;
                        match target {
                            ContextMenuTarget::Folder { path, name } => {
                                let p1 = path.clone();
                                let p2 = path.clone();
                                let p3 = path.clone();
                                let n3 = name.clone();
                                let p4 = path.clone();
                                let n4 = name.clone();
                                view! {
                                    <button
                                        on:click=move |_| {
                                            ctx.set_inline_creation.set(InlineCreationTarget::Chat { parent_path: Some(p1.clone()) });
                                            ctx.set_inline_input_text.set(String::new());
                                            ctx.set_expanded_folders.update(|set| { set.insert(p1.clone()); });
                                        }
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg hover:text-theme-text text-left transition-colors font-medium"
                                    >
                                        <span>"New Chat"</span>
                                    </button>
                                    <button
                                        on:click=move |_| {
                                            ctx.set_inline_creation.set(InlineCreationTarget::Folder { parent_path: Some(p2.clone()) });
                                            ctx.set_inline_input_text.set(String::new());
                                            ctx.set_expanded_folders.update(|set| { set.insert(p2.clone()); });
                                        }
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg hover:text-theme-text text-left transition-colors font-medium"
                                    >
                                        <span>"Create Subdirectory"</span>
                                    </button>
                                    <button
                                        on:click=move |_| {
                                            set_modal_action.set(ModalAction::RenameFolder { path: p3.clone(), current_name: n3.clone() });
                                            set_modal_input.set(n3.clone());
                                        }
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg hover:text-theme-text text-left transition-colors font-medium"
                                    >
                                        <span>"Rename"</span>
                                    </button>
                                    <div class="h-[1px] bg-theme-border/60 my-1"></div>
                                    <button
                                        on:click=move |_| {
                                            set_modal_action.set(ModalAction::DeleteFolder { path: p4.clone(), name: n4.clone() });
                                        }
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg text-theme-destructive hover:text-theme-destructive-hover text-left transition-colors font-medium"
                                    >
                                        <span>"Delete"</span>
                                    </button>
                                }.into_any()
                            }
                            ContextMenuTarget::Chat { id, title, path } => {
                                let id_c = id.clone();
                                let id_r = id.clone();
                                let t_r = title.clone();
                                let p_r = path.clone();
                                let id_d = id.clone();
                                let t_d = title.clone();
                                let p_d = path.clone();
                                let id_del = id.clone();
                                let t_del = title.clone();
                                view! {
                                    <button
                                        on:click=move |_| select_conversation(id_c.clone())
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg hover:text-theme-text text-left transition-colors font-medium"
                                    >
                                        <span>"Select"</span>
                                    </button>
                                    <button
                                        on:click=move |_| {
                                            set_modal_action.set(ModalAction::RenameChat { id: id_r.clone(), current_title: t_r.clone(), path: p_r.clone() });
                                            set_modal_input.set(t_r.clone());
                                        }
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg hover:text-theme-text text-left transition-colors font-medium"
                                    >
                                        <span>"Rename"</span>
                                    </button>
                                    <button
                                        on:click=move |_| duplicate_chat(id_d.clone(), t_d.clone(), p_d.clone())
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg hover:text-theme-text text-left transition-colors font-medium"
                                    >
                                        <span>"Duplicate"</span>
                                    </button>
                                    <div class="h-[1px] bg-theme-border/60 my-1"></div>
                                    <button
                                        on:click=move |_| {
                                            set_modal_action.set(ModalAction::DeleteChat { id: id_del.clone(), name: t_del.clone() });
                                        }
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg text-theme-destructive hover:text-theme-destructive-hover text-left transition-colors font-medium"
                                    >
                                        <span>"Delete"</span>
                                    </button>
                                }.into_any()
                            }
                            ContextMenuTarget::Root => {
                                view! {
                                    <button
                                        on:click=move |_| {
                                            ctx.set_inline_creation.set(InlineCreationTarget::Chat { parent_path: None });
                                            ctx.set_inline_input_text.set(String::new());
                                        }
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg hover:text-theme-text text-left transition-colors font-medium"
                                    >
                                        <span>"New Chat"</span>
                                    </button>
                                    <button
                                        on:click=move |_| {
                                            ctx.set_inline_creation.set(InlineCreationTarget::Folder { parent_path: None });
                                            ctx.set_inline_input_text.set(String::new());
                                        }
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg hover:text-theme-text text-left transition-colors font-medium"
                                    >
                                        <span>"New Directory"</span>
                                    </button>
                                    <div class="h-[1px] bg-theme-border/60 my-1"></div>
                                    <button
                                        on:click=move |_| {
                                            ctx.set_expanded_folders.set(std::collections::HashSet::new());
                                        }
                                        class="w-full flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-theme-bg hover:text-theme-text text-left transition-colors font-medium"
                                    >
                                        <span>"Collapse All"</span>
                                    </button>
                                }.into_any()
                            }
                        }
                    }}
                </div>
            </Show>

            // ─── MODAL CONFIRMATION OVERLAYS (Renaming & Deleting) ───
            <Show when=move || modal_action.get() != ModalAction::None>
                <div class="fixed inset-0 bg-black/60 backdrop-blur-sm z-50 flex items-center justify-center p-4 select-text">
                    <div class="w-full max-w-sm bg-theme-panel border border-theme-border/80 rounded-2xl p-5 shadow-2xl space-y-4 theme-transition animate-scale-in">
                        {move || {
                            let action = modal_action.get();
                            match action {
                                ModalAction::RenameFolder { .. } => view! {
                                    <>
                                        <h3 class="text-sm font-semibold text-theme-text">"Rename Folder"</h3>
                                        <input
                                            type="text"
                                            placeholder="New folder name"
                                            class="w-full bg-theme-input border border-theme-border/80 rounded-xl px-3 py-2 text-sm text-theme-text outline-none focus:border-theme-accent theme-transition"
                                            prop:value=modal_input
                                            on:input=move |ev| set_modal_input.set(event_target_value(&ev))
                                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                                if ev.key() == "Enter" {
                                                    execute_modal_action(());
                                                } else if ev.key() == "Escape" {
                                                    set_modal_action.set(ModalAction::None);
                                                }
                                            }
                                        />
                                        <div class="flex items-center justify-end gap-2 pt-2">
                                            <button
                                                on:click=move |_| set_modal_action.set(ModalAction::None)
                                                class="px-3 py-1.5 rounded-lg text-xs font-semibold text-theme-muted hover:bg-theme-bg/60 transition-colors"
                                            >
                                                "Cancel"
                                            </button>
                                            <button
                                                on:click=move |_| execute_modal_action(())
                                                class="px-3 py-1.5 rounded-lg text-xs font-semibold bg-theme-accent text-theme-bg hover:opacity-90 transition-all"
                                            >
                                                "Rename"
                                            </button>
                                        </div>
                                    </>
                                }.into_any(),
                                ModalAction::RenameChat { .. } => view! {
                                    <>
                                        <h3 class="text-sm font-semibold text-theme-text">"Rename Chat"</h3>
                                        <input
                                            type="text"
                                            placeholder="New chat title"
                                            class="w-full bg-theme-input border border-theme-border/80 rounded-xl px-3 py-2 text-sm text-theme-text outline-none focus:border-theme-accent theme-transition"
                                            prop:value=modal_input
                                            on:input=move |ev| set_modal_input.set(event_target_value(&ev))
                                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                                if ev.key() == "Enter" {
                                                    execute_modal_action(());
                                                } else if ev.key() == "Escape" {
                                                    set_modal_action.set(ModalAction::None);
                                                }
                                            }
                                        />
                                        <div class="flex items-center justify-end gap-2 pt-2">
                                            <button
                                                on:click=move |_| set_modal_action.set(ModalAction::None)
                                                class="px-3 py-1.5 rounded-lg text-xs font-semibold text-theme-muted hover:bg-theme-bg/60 transition-colors"
                                            >
                                                "Cancel"
                                            </button>
                                            <button
                                                on:click=move |_| execute_modal_action(())
                                                class="px-3 py-1.5 rounded-lg text-xs font-semibold bg-theme-accent text-theme-bg hover:opacity-90 transition-all"
                                            >
                                                "Rename"
                                            </button>
                                        </div>
                                    </>
                                }.into_any(),
                                ModalAction::DeleteChat { id: _, name } => view! {
                                    <>
                                        <h3 class="text-sm font-bold text-theme-text">"Delete " {name}</h3>
                                        <p class="text-xs text-theme-muted leading-relaxed">
                                            "Are you sure? This action is irreversible and will delete all messages in this chat."
                                        </p>
                                        <div class="flex items-center justify-end gap-2 pt-2">
                                            <button
                                                on:click=move |_| set_modal_action.set(ModalAction::None)
                                                class="px-3 py-1.5 rounded-lg text-xs font-semibold text-theme-muted hover:bg-theme-bg/60 transition-colors"
                                            >
                                                "Cancel"
                                            </button>
                                            <button
                                                on:click=move |_| execute_modal_action(())
                                                class="px-3 py-1.5 rounded-lg text-xs font-semibold bg-theme-destructive text-theme-destructive-text hover:bg-theme-destructive-hover transition-colors"
                                            >
                                                "Delete"
                                            </button>
                                        </div>
                                    </>
                                }.into_any(),
                                ModalAction::DeleteFolder { path: _, name } => view! {
                                    <>
                                        <h3 class="text-sm font-bold text-theme-text">"Delete directory \"" {name} "\"?"</h3>
                                        <p class="text-xs text-theme-muted leading-relaxed">
                                            "Are you sure? This will permanently delete this directory and all its contents."
                                        </p>
                                        <div class="flex items-center justify-end gap-2 pt-2">
                                            <button
                                                on:click=move |_| set_modal_action.set(ModalAction::None)
                                                class="px-3 py-1.5 rounded-lg text-xs font-semibold text-theme-muted hover:bg-theme-bg/60 transition-colors"
                                            >
                                                "Cancel"
                                            </button>
                                            <button
                                                on:click=move |_| execute_modal_action(())
                                                class="px-3 py-1.5 rounded-lg text-xs font-semibold bg-theme-destructive text-theme-destructive-text hover:bg-theme-destructive-hover transition-colors"
                                            >
                                                "Delete"
                                            </button>
                                        </div>
                                    </>
                                }.into_any(),
                                ModalAction::None => view! {}.into_any(),
                            }
                        }}
                    </div>
                </div>
            </Show>
        </aside>
    }
}

#[component]
#[allow(unused_parens)]
fn FolderNode(
    node: ChatTreeNode,
    depth: usize,
    on_select_chat: Callback<String>,
    on_context_menu: Callback<(web_sys::MouseEvent, ContextMenuTarget)>,
    on_drag_start: Callback<(web_sys::DragEvent, String, bool)>,
    on_commit_creation: Callback<()>,
) -> impl IntoView {
    let ctx = use_context::<AppContext>().expect("AppContext not found");
    
    let path_clone = node.path.clone();
    let path_clone2 = node.path.clone();
    let path_stored = StoredValue::new(node.path.clone());
    let is_expanded = move || ctx.expanded_folders.get().contains(&path_clone);
    let is_expanded2 = move || ctx.expanded_folders.get().contains(&path_clone2);
    
    let path_for_toggle = node.path.clone();
    let toggle_expand = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        let p = path_for_toggle.clone();
        ctx.set_expanded_folders.update(|set| {
            if set.contains(&p) {
                set.remove(&p);
            } else {
                set.insert(p);
            }
        });
    };
    
    let path_drag = node.path.clone();
    let is_dir_drag = node.is_dir;
    let handle_dragstart = move |ev: web_sys::DragEvent| {
        on_drag_start.run((ev, path_drag.clone(), is_dir_drag));
    };
    
    let is_dir_dragover = node.is_dir;
    let (is_dragged_over, set_is_dragged_over) = signal(false);
    
    let handle_dragover = move |ev: web_sys::DragEvent| {
        if is_dir_dragover {
            ev.prevent_default();
            set_is_dragged_over.set(true);
        }
    };
    
    let handle_dragleave = move |_| {
        set_is_dragged_over.set(false);
    };
    
    let path_drop = node.path.clone();
    let is_dir_drop = node.is_dir;
    let set_chat_tree_c = ctx.set_chat_tree;
    let set_conversations_c = ctx.set_conversations;
    let handle_drop = move |ev: web_sys::DragEvent| {
        ev.prevent_default();
        set_is_dragged_over.set(false);
        if !is_dir_drop {
            return;
        }
        
        if let Some(dt) = ev.data_transfer() {
            if let Ok(data) = dt.get_data("text/plain") {
                let parts: Vec<&str> = data.split('|').collect();
                if parts.len() == 2 {
                    let src_path = parts[0].to_string();
                    let src_name = Path::new(&src_path).file_name().unwrap_or_default().to_string_lossy().to_string();
                    let dest_path = Path::new(&path_drop).join(src_name).to_string_lossy().to_string();
                    
                    if src_path != dest_path && !dest_path.starts_with(&format!("{}/", src_path)) {
                        let src_c = src_path.clone();
                        let dest_c = dest_path.clone();
                        spawn_local(async move {
                            let args = serde_wasm_bindgen::to_value(&MoveItemArgs {
                                source_rel: src_c,
                                dest_rel: dest_c,
                            }).unwrap();
                            if invoke_result("move_item", args).await.is_ok() {
                                let res_tree = invoke("get_chat_tree", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                                if let Ok(tree) = serde_wasm_bindgen::from_value::<Vec<ChatTreeNode>>(res_tree) {
                                    set_chat_tree_c.set(tree);
                                }
                                let res_convs = invoke("load_conversations", serde_wasm_bindgen::to_value(&()).unwrap()).await;
                                if let Ok(convs) = serde_wasm_bindgen::from_value::<Vec<ChatConversation>>(res_convs) {
                                    set_conversations_c.set(convs);
                                }
                            }
                        });
                    }
                }
            }
        }
    };
    
    let path_context = node.path.clone();
    let name_context = node.name.clone();
    let is_dir_context = node.is_dir;
    let chat_id_context = node.chat_id.clone();
    let handle_contextmenu = move |ev: web_sys::MouseEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        let target = if is_dir_context {
            ContextMenuTarget::Folder {
                path: path_context.clone(),
                name: name_context.clone(),
            }
        } else {
            ContextMenuTarget::Chat {
                id: chat_id_context.clone().unwrap_or_default(),
                title: name_context.clone(),
                path: path_context.clone(),
            }
        };
        on_context_menu.run((ev, target));
    };
    
    let indent_style = format!("padding-left: {}rem;", (depth as f32) * 0.75 + 0.5);

    let view_inline_chat_input = {
        let on_commit = on_commit_creation.clone();
        let d = depth + 1;
        move || {
            let input_ref = NodeRef::<leptos::html::Input>::new();
            Effect::new(move |_| {
                if let Some(el) = input_ref.get() {
                    let _ = el.focus();
                }
            });
            let ind = format!("padding-left: {}rem;", (d as f32) * 0.75 + 0.5);
            let on_c = on_commit.clone();
            view! {
                <div
                    style=ind
                    class="flex items-center gap-2 py-1.5 pr-2 rounded-lg bg-theme-bg/40 text-sm border border-theme-accent/50 animate-scale-in"
                >
                    <span class="text-theme-muted/50 shrink-0">
                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
                        </svg>
                    </span>
                    <input
                        type="text"
                        class="w-full bg-transparent text-theme-text outline-none text-sm"
                        placeholder="Chat name..."
                        node_ref=input_ref
                        prop:value=ctx.inline_input_text
                        on:input=move |ev| ctx.set_inline_input_text.set(event_target_value(&ev))
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            if ev.key() == "Enter" {
                                ev.prevent_default();
                                ev.stop_propagation();
                                if let Some(target) = ev.target() {
                                    if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                                        let _ = input.blur();
                                    }
                                }
                            } else if ev.key() == "Escape" {
                                ev.prevent_default();
                                ev.stop_propagation();
                                ctx.set_inline_creation.set(InlineCreationTarget::None);
                                if let Some(target) = ev.target() {
                                    if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                                        let _ = input.blur();
                                    }
                                }
                            }
                        }
                        on:blur={
                            let on_c_blur = on_c.clone();
                            move |_| {
                                on_c_blur.run(());
                            }
                        }
                    />
                </div>
            }
        }
    };

    let view_inline_folder_input = {
        let on_commit = on_commit_creation.clone();
        let d = depth + 1;
        move || {
            let input_ref = NodeRef::<leptos::html::Input>::new();
            Effect::new(move |_| {
                if let Some(el) = input_ref.get() {
                    let _ = el.focus();
                }
            });
            let ind = format!("padding-left: {}rem;", (d as f32) * 0.75 + 0.5);
            let on_c = on_commit.clone();
            view! {
                <div
                    style=ind
                    class="flex items-center gap-1.5 py-1.5 pr-2 rounded-lg bg-theme-bg/40 text-sm border border-theme-accent/50 animate-scale-in"
                >
                    <span class="text-theme-muted/80">
                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
                        </svg>
                    </span>
                    <span class="text-theme-accent/80 shrink-0">
                        <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                        </svg>
                    </span>
                    <input
                        type="text"
                        class="w-full bg-transparent text-theme-text outline-none text-sm"
                        placeholder="Folder name..."
                        node_ref=input_ref
                        prop:value=ctx.inline_input_text
                        on:input=move |ev| ctx.set_inline_input_text.set(event_target_value(&ev))
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            if ev.key() == "Enter" {
                                ev.prevent_default();
                                ev.stop_propagation();
                                if let Some(target) = ev.target() {
                                    if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                                        let _ = input.blur();
                                    }
                                }
                            } else if ev.key() == "Escape" {
                                ev.prevent_default();
                                ev.stop_propagation();
                                ctx.set_inline_creation.set(InlineCreationTarget::None);
                                if let Some(target) = ev.target() {
                                    if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                                        let _ = input.blur();
                                    }
                                }
                            }
                        }
                        on:blur={
                            let on_c_blur = on_c.clone();
                            move |_| {
                                on_c_blur.run(());
                            }
                        }
                    />
                </div>
            }
        }
    };
    
    if node.is_dir {
        let children_stored = StoredValue::new(node.children.clone().unwrap_or_default());
        let dragover_class = move || if is_dragged_over.get() {
            "bg-theme-accent/10 border-theme-accent/50 text-theme-text font-semibold"
        } else {
            "text-theme-muted hover:bg-theme-bg/40 hover:text-theme-text"
        };
        
        view! {
            <div class="space-y-0.5 select-none">
                <div
                    draggable="true"
                    on:dragstart=handle_dragstart
                    on:dragover=handle_dragover
                    on:dragleave=handle_dragleave
                    on:drop=handle_drop
                    on:contextmenu=handle_contextmenu
                    on:click=toggle_expand
                    style=indent_style.clone()
                    class={move || format!("flex items-center justify-between py-1.5 pr-2 rounded-lg cursor-pointer transition-all border border-transparent text-sm {}", dragover_class())}
                >
                    <div class="flex items-center gap-1.5 min-w-0">
                        <span class="text-theme-muted/80">
                            <Show
                                when=is_expanded
                                fallback=move || view! {
                                    <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
                                    </svg>
                                }
                            >
                                <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
                                </svg>
                            </Show>
                        </span>
                        <span class="text-theme-accent/80 shrink-0">
                            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                            </svg>
                        </span>
                        <span class="font-medium truncate">{node.name.clone()}</span>
                    </div>
                </div>
                
                <Show when=is_expanded2>
                    <div class="space-y-0.5">
                        {move || {
                            let mut list = children_stored.get_value();
                            if ctx.sort_alphabetical.get() {
                                list.sort_by(|a, b| {
                                    if a.is_dir != b.is_dir {
                                        b.is_dir.cmp(&a.is_dir)
                                    } else {
                                        a.name.to_lowercase().cmp(&b.name.to_lowercase())
                                    }
                                });
                            } else {
                                list.sort_by(|a, b| {
                                    if a.is_dir != b.is_dir {
                                        b.is_dir.cmp(&a.is_dir)
                                    } else if a.is_dir {
                                        a.name.to_lowercase().cmp(&b.name.to_lowercase())
                                    } else {
                                        let time_a = a.updated_at.unwrap_or(0);
                                        let time_b = b.updated_at.unwrap_or(0);
                                        time_b.cmp(&time_a)
                                    }
                                });
                            }
                            let mut views: Vec<AnyView> = list.into_iter().map(|child| {
                                let on_sel = on_select_chat.clone();
                                let on_ctx = on_context_menu.clone();
                                let on_drag = on_drag_start.clone();
                                let on_comm = on_commit_creation.clone();
                                view! {
                                    <FolderNode
                                        node=child
                                        depth={depth + 1}
                                        on_select_chat=on_sel
                                        on_context_menu=on_ctx
                                        on_drag_start=on_drag
                                        on_commit_creation=on_comm
                                    />
                                }.into_any()
                            }).collect();
                            
                            let path_val = path_stored.get_value();
                            let inline_state = ctx.inline_creation.get();
                            match inline_state {
                                InlineCreationTarget::Chat { parent_path: Some(ref p) } if p == &path_val => {
                                    views.insert(0, view_inline_chat_input().into_any());
                                }
                                InlineCreationTarget::Folder { parent_path: Some(ref p) } if p == &path_val => {
                                    views.insert(0, view_inline_folder_input().into_any());
                                }
                                _ => {}
                            }
                            views
                        }}
                    </div>
                </Show>
            </div>
        }.into_any()
    } else {
        let chat_id = node.chat_id.clone().unwrap_or_default();
        let is_active = move || ctx.current_conversation_id.get() == Some(chat_id.clone());
        let active_class = move || if is_active() {
            "bg-theme-bg text-theme-text font-semibold border-l-2 border-theme-accent"
        } else {
            "text-theme-muted hover:bg-theme-bg/40 hover:text-theme-text border-l-2 border-transparent"
        };
        
        let chat_id_click = node.chat_id.clone().unwrap_or_default();
        let on_sel_c = on_select_chat.clone();
        
        view! {
            <div
                draggable="true"
                on:dragstart=handle_dragstart
                on:contextmenu=handle_contextmenu
                on:click=move |ev| {
                    ev.stop_propagation();
                    on_sel_c.run(chat_id_click.clone());
                }
                style=indent_style
                class={move || format!("flex items-center justify-between py-1.5 pr-2 rounded-lg cursor-pointer transition-all border border-transparent text-sm {}", active_class())}
            >
                <div class="flex items-center gap-2 min-w-0">
                    <span class="text-theme-muted/50 shrink-0">
                        <svg class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
                        </svg>
                    </span>
                    <span class="truncate">{node.name.clone()}</span>
                </div>
            </div>
        }.into_any()
    }
}
