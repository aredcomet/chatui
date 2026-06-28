use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{Connection, Provider, ModelReasoningConfig};
use crate::app::bindings::{
    invoke, invoke_raw, FetchModelsArgs, SaveConnectionsArgs, DeleteConnectionArgs,
};

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

#[component]
pub fn ConnectionsModal(
    show_settings: ReadSignal<bool>,
    set_show_settings: WriteSignal<bool>,
    connections: ReadSignal<Vec<Connection>>,
    set_connections: WriteSignal<Vec<Connection>>,
    active_connection_id: ReadSignal<Option<String>>,
    set_active_connection_id: WriteSignal<Option<String>>,
    set_selected_provider: WriteSignal<Provider>,
    set_selected_model: WriteSignal<String>,
) -> impl IntoView {
    let (show_add_connection, set_show_add_connection) = signal(false);
    let (editing_connection_id, set_editing_connection_id) = signal(None::<String>);

    let (new_conn_provider, set_new_conn_provider) = signal(Provider::OpenAI);
    let (new_conn_name, set_new_conn_name) = signal(String::new());
    let (new_conn_api_key, set_new_conn_api_key) = signal(String::new());
    let (new_conn_base_url, set_new_conn_base_url) = signal(String::new());
    let (new_conn_fetched_models, set_new_conn_fetched_models) = signal(Vec::<String>::new());
    let (new_conn_search_query, set_new_conn_search_query) = signal(String::new());
    let (new_conn_enabled_models, set_new_conn_enabled_models) = signal(Vec::<String>::new());
    let (new_conn_default_model, set_new_conn_default_model) = signal(String::new());
    let (new_conn_reasoning_configs, set_new_conn_reasoning_configs) =
        signal(Vec::<ModelReasoningConfig>::new());

    let (fetching_models_loading, set_fetching_models_loading) = signal(false);
    let (fetching_models_error, set_fetching_models_error) = signal(None::<String>);

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

    let fetch_models_click = move |_| {
        let provider = new_conn_provider.get_untracked();
        let api_key = new_conn_api_key.get_untracked().trim().to_string();
        let base_url_str = new_conn_base_url.get_untracked().trim().to_string();

        if api_key.is_empty() {
            set_fetching_models_error.set(Some("API key is required to fetch models".to_string()));
            return;
        }

        let base_url =
            if provider == Provider::CustomOpenAICompliant || provider == Provider::OpenRouter {
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
                Ok(val) => match serde_wasm_bindgen::from_value::<Vec<String>>(val) {
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
                },
                Err(err) => {
                    let err_str = err
                        .as_string()
                        .unwrap_or_else(|| "Unknown error".to_string());
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

        let base_url =
            if provider == Provider::CustomOpenAICompliant || provider == Provider::OpenRouter {
                if base_url_str.is_empty() {
                    None
                } else {
                    Some(base_url_str)
                }
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
            let args = serde_wasm_bindgen::to_value(&SaveConnectionsArgs {
                connections: current_conns,
            })
            .unwrap();
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
                                                                        class="p-2 rounded-xl text-theme-muted hover:text-theme-destructive hover:bg-theme-bg transition-all"
                                                                    >
                                                                        <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"/>
                                                                        </svg>
                                                                    </button>
                                                                </div>
                                                            </div>
                                                        }
                                                    }).collect::<Vec<_>>()
                                                    }
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
                                    <div class="p-3 bg-theme-error-bg border border-theme-error-border rounded-xl text-xs text-theme-error-text flex items-center gap-2 theme-transition">
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
    }
}
