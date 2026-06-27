use leptos::prelude::*;
use leptos::task::spawn_local;
use crate::app::context::AppContext;
use crate::app::theme::{AppTheme, save_theme};
use crate::app::bindings::{invoke, SaveConversationArgs};

#[component]
pub fn SelectorHeader() -> impl IntoView {
    let ctx = use_context::<AppContext>().expect("AppContext not found");

    let on_connection_change = move |ev| {
        let id_str = event_target_value(&ev);
        if id_str.is_empty() {
            ctx.set_active_connection_id.set(None);
            if let Some(convo_id) = ctx.current_conversation_id.get_untracked() {
                let mut convos = ctx.conversations.get_untracked();
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
                ctx.set_conversations.set(convos);
            }
            return;
        }
        ctx.set_active_connection_id.set(Some(id_str.clone()));

        if let Some(conn) = ctx.connections.get_untracked().iter().find(|c| c.id == id_str) {
            let provider = conn.provider;
            let model = conn.default_model.clone();
            let connection_id = Some(id_str);

            ctx.set_selected_provider.set(provider);
            ctx.set_selected_model.set(model.clone());

            // Save connection change to current conversation
            if let Some(convo_id) = ctx.current_conversation_id.get_untracked() {
                let mut convos = ctx.conversations.get_untracked();
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
                ctx.set_conversations.set(convos);
            }
        }
    };

    let on_model_change = move |ev| {
        let model = event_target_value(&ev);
        ctx.set_selected_model.set(model.clone());

        // Save model change to current conversation
        if let Some(convo_id) = ctx.current_conversation_id.get_untracked() {
            let mut convos = ctx.conversations.get_untracked();
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
            ctx.set_conversations.set(convos);
        }
    };

    view! {
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
                            let conns = ctx.connections.get();
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
                                        <option value=id_val selected={move || ctx.active_connection_id.get() == Some(id_clone.clone())}>{name}</option>
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
                            let active_id = ctx.active_connection_id.get();
                            if let Some(conn_id) = active_id {
                                if let Some(conn) = ctx.connections.get().into_iter().find(|c| c.id == conn_id) {
                                    return conn.enabled_models.into_iter().map(|m| {
                                        let m_val = m.clone();
                                        let m_clone1 = m.clone();
                                        let m_clone2 = m.clone();
                                        view! {
                                            <option value=m_val selected={move || ctx.selected_model.get() == m_clone1.clone()}>{m_clone2}</option>
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
                        "Temp: " {move || format!("{:.1}", ctx.temperature.get())}
                    </span>
                    <input
                        type="range"
                        min="0.0"
                        max="1.5"
                        step="0.1"
                        prop:value=move || ctx.temperature.get()
                        on:input=move |ev| {
                            if let Ok(val) = event_target_value(&ev).parse::<f32>() {
                                ctx.set_temperature.set(val);
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
                            if ctx.app_theme.get() == AppTheme::Light {
                                "bg-theme-bg text-theme-text shadow-sm border border-theme-border/20"
                            } else {
                                "text-theme-muted hover:text-theme-text"
                            }
                        )
                        on:click=move |_| {
                            ctx.set_app_theme.set(AppTheme::Light);
                            save_theme(AppTheme::Light);
                        }
                    >
                        "Light"
                    </button>
                    <button
                        type="button"
                        title="Dark Mode"
                        class=move || format!("px-2.5 py-1 text-[11px] font-semibold rounded-lg transition-all {}",
                            if ctx.app_theme.get() == AppTheme::Dark {
                                "bg-theme-bg text-theme-text shadow-sm border border-theme-border/20"
                            } else {
                                "text-theme-muted hover:text-theme-text"
                            }
                        )
                        on:click=move |_| {
                            ctx.set_app_theme.set(AppTheme::Dark);
                            save_theme(AppTheme::Dark);
                        }
                    >
                        "Dark"
                    </button>
                </div>
            </div>
        </header>
    }
}
