use serde::Serialize;
use shared::{ApiConfig, ChatConversation, ChatMessage, Connection, Provider};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    pub fn invoke_raw(cmd: &str, args: JsValue) -> js_sys::Promise;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = convertFileSrc)]
    pub fn convert_file_src(path: &str, protocol: Option<&str>) -> String;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    pub async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;

    #[wasm_bindgen(js_name = eval)]
    pub fn eval_js(s: &str);
}

// Helper function to call Tauri commands safely and catch exceptions without panicking Wasm
pub async fn invoke(cmd: &str, args: JsValue) -> JsValue {
    let promise = invoke_raw(cmd, args);
    match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(val) => val,
        Err(err) => {
            web_sys::console::error_1(&err);
            JsValue::NULL
        }
    }
}

// Arguments for Tauri commands
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchModelsArgs {
    pub provider: Provider,
    pub api_key: String,
    pub base_url: Option<String>,
}

#[derive(Serialize)]
pub struct SaveConnectionsArgs {
    pub connections: Vec<Connection>,
}

#[derive(Serialize)]
pub struct DeleteConnectionArgs {
    pub id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageStreamArgs {
    pub conversation_id: String,
    pub config: ApiConfig,
    pub messages: Vec<ChatMessage>,
}

#[derive(Serialize)]
pub struct SaveConversationArgs {
    pub conversation: ChatConversation,
}

#[derive(Serialize)]
pub struct DeleteConversationArgs {
    pub id: String,
}

#[derive(Serialize)]
pub struct CancelStreamArgs {
    pub conversation_id: String,
}

pub fn read_file_as_data_url(file: &web_sys::File) -> Result<js_sys::Promise, JsValue> {
    let reader = web_sys::FileReader::new()?;
    let reader_c = reader.clone();
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let reader_inner = reader_c.clone();
        let onload = Closure::wrap(Box::new(move |_: web_sys::Event| {
            if let Ok(result) = reader_inner.result() {
                let _ = resolve.call1(&JsValue::UNDEFINED, &result);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);

        let onerror = Closure::wrap(Box::new(move |_: web_sys::Event| {
            let _ = reject.call1(
                &JsValue::UNDEFINED,
                &JsValue::from_str("Error reading file"),
            );
        }) as Box<dyn FnMut(web_sys::Event)>);

        reader_c.set_onload(Some(onload.as_ref().unchecked_ref()));
        reader_c.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        onload.forget();
        onerror.forget();
    });
    reader.read_as_data_url(file)?;
    Ok(promise)
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, PartialEq)]
pub struct ChatTreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub chat_id: Option<String>,
    pub updated_at: Option<u64>,
    pub children: Option<Vec<ChatTreeNode>>,
}

#[derive(Serialize)]
pub struct CreateFolderArgs {
    pub relative_path: String,
}

#[derive(Serialize)]
pub struct MoveItemArgs {
    pub source_rel: String,
    pub dest_rel: String,
}

#[derive(Serialize)]
pub struct DeleteFolderRecursiveArgs {
    pub relative_path: String,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub custom_storage_path: Option<String>,
    pub expanded_folders: Vec<String>,
    pub sort_alphabetical: bool,
}


