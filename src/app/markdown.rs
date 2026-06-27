use leptos::prelude::*;
use super::bindings::eval_js;

pub fn render_latex_and_mermaid() {
    let script = r#"
        setTimeout(() => {
            if (window.renderMathInElement) {
                window.renderMathInElement(document.body, {
                    delimiters: [
                        {left: '$$', right: '$$', display: true},
                        {left: '$', right: '$', display: false},
                        {left: '\\(', right: '\\)', display: false},
                        {left: '\\[', right: '\\]', display: true}
                    ],
                    throwOnError: false
                });
            }
            if (window.mermaid) {
                try {
                    window.mermaid.run();
                } catch(e) {
                    console.error("Mermaid initialization failed", e);
                }
            }
        }, 50);
    "#;
    eval_js(script);
}

pub fn parse_thinking_content(text: &str) -> (Option<String>, String) {
    if let Some(start_idx) = text.find("<think>") {
        let content_start = start_idx + "<think>".len();
        if let Some(end_idx) = text[content_start..].find("</think>") {
            let actual_end = content_start + end_idx;
            let thinking = text[content_start..actual_end].to_string();
            let remaining = format!(
                "{}{}",
                &text[..start_idx],
                &text[actual_end + "</think>".len()..]
            );
            (Some(thinking), remaining)
        } else {
            let thinking = text[content_start..].to_string();
            let remaining = text[..start_idx].to_string();
            (Some(thinking), remaining)
        }
    } else {
        (None, text.to_string())
    }
}

pub fn render_message_content(text: String) -> impl IntoView {
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

            if lang == "mermaid" {
                views.push(view! {
                    <div class="mermaid my-3 p-4 bg-theme-panel/40 border border-theme-border/60 rounded-xl flex justify-center overflow-x-auto">
                        {code_content}
                    </div>
                }.into_any());
            } else {
                views.push(view! {
                    <div class="my-3 rounded-lg overflow-hidden border border-theme-border/60 bg-theme-panel font-mono text-sm max-w-full">
                        <div class="flex justify-between items-center bg-theme-panel/85 px-4 py-1.5 text-xs text-theme-muted border-b border-theme-border/60 select-none">
                            <span>{if lang.is_empty() { "code".to_string() } else { lang.clone() }}</span>
                        </div>
                        <pre class="p-4 overflow-x-auto text-theme-text font-mono">
                            <code>{code_content}</code>
                        </pre>
                    </div>
                }.into_any());
            }
        } else {
            let mut current_paragraph = Vec::new();
            let mut current_list = Vec::new();

            let flush_paragraph = |para: &mut Vec<String>, views: &mut Vec<AnyView>| {
                if !para.is_empty() {
                    let para_text = para.join("\n");
                    views.push(view! {
                        <p class="whitespace-pre-wrap text-theme-text my-1 py-0.5 leading-relaxed break-words">{render_inline(para_text)}</p>
                    }.into_any());
                    para.clear();
                }
            };

            let flush_list = |list: &mut Vec<String>, views: &mut Vec<AnyView>| {
                if !list.is_empty() {
                    let list_items: Vec<_> = list.drain(..).map(|item| {
                        view! {
                            <li class="list-disc ml-6 text-theme-text py-0.5">{render_inline(item)}</li>
                        }
                    }).collect();
                    views.push(
                        view! {
                            <ul class="space-y-1 my-1">
                                {list_items}
                            </ul>
                        }
                        .into_any(),
                    );
                }
            };

            let lines: Vec<&str> = part.lines().collect();
            let mut i = 0;

            while i < lines.len() {
                let trimmed = lines[i].trim();
                if trimmed.is_empty() {
                    // Look ahead to see if the next non-empty line is a list item
                    let mut next_list_item = false;
                    let mut j = i + 1;
                    while j < lines.len() {
                        let next_trimmed = lines[j].trim();
                        if !next_trimmed.is_empty() {
                            if next_trimmed.starts_with("- ") || next_trimmed.starts_with("* ") {
                                next_list_item = true;
                            }
                            break;
                        }
                        j += 1;
                    }

                    if !next_list_item {
                        flush_list(&mut current_list, &mut views);
                        flush_paragraph(&mut current_paragraph, &mut views);
                    }
                    i += 1;
                    continue;
                }

                if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                    flush_paragraph(&mut current_paragraph, &mut views);
                    let content = trimmed[2..].to_string();
                    current_list.push(content);
                } else if trimmed.starts_with("### ") {
                    flush_list(&mut current_list, &mut views);
                    flush_paragraph(&mut current_paragraph, &mut views);
                    let content = trimmed[4..].to_string();
                    views.push(view! {
                        <h4 class="text-md font-bold text-theme-text mt-4 mb-2">{render_inline(content)}</h4>
                    }.into_any());
                } else if trimmed.starts_with("## ") {
                    flush_list(&mut current_list, &mut views);
                    flush_paragraph(&mut current_paragraph, &mut views);
                    let content = trimmed[3..].to_string();
                    views.push(view! {
                        <h3 class="text-lg font-bold text-theme-text mt-4 mb-2">{render_inline(content)}</h3>
                    }.into_any());
                } else if trimmed.starts_with("# ") {
                    flush_list(&mut current_list, &mut views);
                    flush_paragraph(&mut current_paragraph, &mut views);
                    let content = trimmed[2..].to_string();
                    views.push(view! {
                        <h2 class="text-xl font-bold text-theme-text mt-4 mb-2">{render_inline(content)}</h2>
                    }.into_any());
                } else {
                    flush_list(&mut current_list, &mut views);
                    current_paragraph.push(trimmed.to_string());
                }
                i += 1;
            }

            flush_list(&mut current_list, &mut views);
            flush_paragraph(&mut current_paragraph, &mut views);
        }
        is_code = !is_code;
    }

    view! {
        <div class="space-y-1 overflow-hidden max-w-full">
            {views}
        </div>
    }
}

pub fn render_inline(text: String) -> Vec<AnyView> {
    let mut views = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            if !current.is_empty() {
                let c = current.clone();
                views.push(view! { <span>{c}</span> }.into_any());
                current.clear();
            }
            let mut j = i + 2;
            let mut found = false;
            while j + 1 < chars.len() {
                if chars[j] == '*' && chars[j + 1] == '*' {
                    found = true;
                    break;
                }
                j += 1;
            }
            if found {
                let bold_text: String = chars[i + 2..j].iter().collect();
                views.push(
                    view! { <strong class="font-bold text-theme-text">{bold_text}</strong> }
                        .into_any(),
                );
                i = j + 2;
            } else {
                current.push('*');
                current.push('*');
                i += 2;
            }
        } else if chars[i] == '`' {
            if !current.is_empty() {
                let c = current.clone();
                views.push(view! { <span>{c}</span> }.into_any());
                current.clear();
            }
            let mut j = i + 1;
            let mut found = false;
            while j < chars.len() {
                if chars[j] == '`' {
                    found = true;
                    break;
                }
                j += 1;
            }
            if found {
                let code_text: String = chars[i + 1..j].iter().collect();
                views.push(view! { <code class="px-1.5 py-0.5 rounded bg-theme-panel font-mono text-[13px] text-theme-accent">{code_text}</code> }.into_any());
                i = j + 1;
            } else {
                current.push('`');
                i += 1;
            }
        } else if chars[i] == '*' {
            if !current.is_empty() {
                let c = current.clone();
                views.push(view! { <span>{c}</span> }.into_any());
                current.clear();
            }
            let mut j = i + 1;
            let mut found = false;
            while j < chars.len() {
                if chars[j] == '*' {
                    if j + 1 < chars.len() && chars[j + 1] == '*' {
                        j += 2;
                        continue;
                    }
                    found = true;
                    break;
                }
                j += 1;
            }
            if found {
                let italic_text: String = chars[i + 1..j].iter().collect();
                views.push(
                    view! { <em class="italic text-theme-text">{italic_text}</em> }.into_any(),
                );
                i = j + 1;
            } else {
                current.push('*');
                i += 1;
            }
        } else {
            current.push(chars[i]);
            i += 1;
        }
    }
    if !current.is_empty() {
        views.push(view! { <span>{current}</span> }.into_any());
    }
    views
}
