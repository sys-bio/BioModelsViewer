//! Visitor counting with GoatCounter (hsauro-biomodels.goatcounter.com).
//!
//! The counting script is loaded by `index.html`. It records each page
//! load, and ignores visits from localhost and private networks, so
//! `trunk serve` is not counted. Here the page also records each model
//! opened, as an event, and reads the page's visit count to show it.
//!
//! An ad blocker may block GoatCounter; then nothing is counted or shown,
//! and nothing else is affected.

use wasm_bindgen::{JsCast, JsValue};

const SITE: &str = "https://hsauro-biomodels.goatcounter.com";

/// Records that a model was opened, as the event `model/<id>`, titled with
/// the model's name, so the dashboard lists the models people look at.
pub fn count_model(id: &str, name: &str) {
    let _ = try_count_model(id, name);
}

fn try_count_model(id: &str, name: &str) -> Option<()> {
    let window = web_sys::window()?;
    let goatcounter = js_sys::Reflect::get(&window, &JsValue::from_str("goatcounter")).ok()?;
    let count = js_sys::Reflect::get(&goatcounter, &JsValue::from_str("count"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    let vars = js_sys::Object::new();
    js_sys::Reflect::set(&vars, &"path".into(), &format!("model/{id}").into()).ok()?;
    js_sys::Reflect::set(&vars, &"title".into(), &name.into()).ok()?;
    js_sys::Reflect::set(&vars, &"event".into(), &JsValue::TRUE).ok()?;
    count.call1(&goatcounter, &vars).ok()?;
    Some(())
}

/// The number of visits to this page, formatted by GoatCounter ("1,234"),
/// or `None` if it cannot be read (the site's "allow using the visitor
/// counter" setting is off, or GoatCounter is blocked).
pub async fn visit_count() -> Option<String> {
    let path = web_sys::window()?.location().pathname().ok()?;
    let encoded: String = js_sys::encode_uri_component(&path).into();
    let text = crate::cache::fetch_text(&format!("{SITE}/counter/{encoded}.json"))
        .await
        .ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    match &json["count"] {
        serde_json::Value::String(s) => Some(s.trim().to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
