use leptos::prelude::*;
use crate::app::markdown::render_message_content;

#[component]
pub fn ThinkingBlock(
    thinking: String,
    is_thinking: bool,
    duration_ms: Option<u64>,
) -> impl IntoView {
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
        <div class="mb-2 rounded-xl border border-theme-border/40 bg-theme-panel/20 overflow-hidden theme-transition w-full">
            <div
                on:click=move |_| set_collapsed.update(|c| *c = !*c)
                class="flex items-center justify-between px-3 py-2 cursor-pointer hover:bg-theme-border/10 select-none text-xs font-semibold text-theme-muted/80 theme-transition"
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
                class=move || format!("px-3 pb-3 pt-1 text-[13px] text-theme-muted/90 font-sans leading-relaxed overflow-x-auto select-text {}", if collapsed.get() { "hidden" } else { "block" })
            >
                {render_message_content(thinking.clone())}
            </div>
        </div>
    }
}
