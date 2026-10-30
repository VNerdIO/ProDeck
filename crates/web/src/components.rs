//! Small reusable form controls tuned for phones (big targets, numeric keypads).

use leptos::prelude::*;
use serde::de::DeserializeOwned;
use serde::Serialize;

fn parse_num(s: &str) -> f64 {
    s.trim().parse::<f64>().unwrap_or(0.0)
}

fn trim_num(x: f64) -> String {
    if x == 0.0 {
        String::new()
    } else if x.fract() == 0.0 {
        format!("{x:.0}")
    } else {
        format!("{}", (x * 1000.0).round() / 1000.0)
    }
}

/// Feet + inches entry for a length stored in inches.
#[component]
pub fn FtIn(
    label: &'static str,
    #[prop(into)] value: Signal<f64>,
    #[prop(into)] on_change: Callback<f64>,
) -> impl IntoView {
    let feet = move || trim_num((value.get() / 12.0).floor());
    let inches = move || trim_num(value.get() - (value.get() / 12.0).floor() * 12.0);
    let set_ft = move |ev| {
        let ft = parse_num(&event_target_value(&ev)).max(0.0);
        let inch = value.get_untracked() - (value.get_untracked() / 12.0).floor() * 12.0;
        on_change.run(ft.floor() * 12.0 + inch);
    };
    let set_in = move |ev| {
        let inch = parse_num(&event_target_value(&ev)).max(0.0);
        let ft = (value.get_untracked() / 12.0).floor();
        on_change.run(ft * 12.0 + inch);
    };
    view! {
        <div class="field">
            <label>{label}</label>
            <div class="ftin">
                <div class="unit">
                    <input type="number" inputmode="numeric" min="0" step="1" aria-label=format!("{label} feet")
                        prop:value=feet on:change=set_ft />
                    <span>"ft"</span>
                </div>
                <div class="unit">
                    <input type="number" inputmode="decimal" min="0" step="0.25" aria-label=format!("{label} inches")
                        prop:value=inches on:change=set_in />
                    <span>"in"</span>
                </div>
            </div>
        </div>
    }
}

/// Plain number entry (inches or psf).
#[component]
pub fn Num(
    label: &'static str,
    #[prop(into)] value: Signal<f64>,
    #[prop(into)] on_change: Callback<f64>,
    #[prop(optional)] suffix: &'static str,
    #[prop(default = 1.0)] step: f64,
) -> impl IntoView {
    view! {
        <div class="field">
            <label>{label}</label>
            <div class="ftin" style="grid-template-columns:1fr">
                <div class="unit">
                    <input type="number" inputmode="decimal" step=step
                        prop:value=move || trim_num(value.get())
                        on:change=move |ev| on_change.run(parse_num(&event_target_value(&ev))) />
                    <span>{suffix}</span>
                </div>
            </div>
        </div>
    }
}

/// Option key for a select value: the serialized enum variant (`"south"`, `"2x8"`) or the
/// JSON form of a number/bool.
pub fn enum_key<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::new(),
    }
}

fn enum_from<T: DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .ok()
        .or_else(|| serde_json::from_str(s).ok())
}

/// `<select>` over enum variants.
#[component]
pub fn EnumSelect<T>(
    label: &'static str,
    options: Vec<(T, &'static str)>,
    #[prop(into)] value: Signal<T>,
    #[prop(into)] on_change: Callback<T>,
) -> impl IntoView
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    let opts = options
        .into_iter()
        .map(|(v, text)| {
            let key = enum_key(&v);
            let k2 = key.clone();
            view! { <option value=key selected=move || enum_key(&value.get()) == k2>{text}</option> }
        })
        .collect_view();
    view! {
        <div class="field">
            <label>{label}</label>
            <select on:change=move |ev| {
                if let Some(v) = enum_from::<T>(&event_target_value(&ev)) {
                    on_change.run(v);
                }
            }>{opts}</select>
        </div>
    }
}

/// Like `EnumSelect` but with an "Auto" / none choice.
#[component]
pub fn OptEnumSelect<T>(
    label: &'static str,
    none_label: &'static str,
    options: Vec<(T, &'static str)>,
    #[prop(into)] value: Signal<Option<T>>,
    #[prop(into)] on_change: Callback<Option<T>>,
) -> impl IntoView
where
    T: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
{
    let opts = options
        .into_iter()
        .map(|(v, text)| {
            let key = enum_key(&v);
            let k2 = key.clone();
            view! {
                <option value=key selected=move || value.get().map(|x| enum_key(&x)).as_deref() == Some(k2.as_str())>
                    {text}
                </option>
            }
        })
        .collect_view();
    view! {
        <div class="field">
            <label>{label}</label>
            <select on:change=move |ev| on_change.run(enum_from::<T>(&event_target_value(&ev)))>
                <option value="" selected=move || value.get().is_none()>{none_label}</option>
                {opts}
            </select>
        </div>
    }
}

#[component]
pub fn Check(
    label: &'static str,
    #[prop(into)] value: Signal<bool>,
    #[prop(into)] on_change: Callback<bool>,
) -> impl IntoView {
    view! {
        <label class="check">
            <input type="checkbox" prop:checked=move || value.get()
                on:change=move |ev| on_change.run(event_target_checked(&ev)) />
            {label}
        </label>
    }
}

#[component]
pub fn ErrorNotice(#[prop(into)] message: Signal<Option<String>>) -> impl IntoView {
    move || {
        message
            .get()
            .map(|m| view! { <p class="notice error" role="alert">{m}</p> })
    }
}

pub fn confirm(msg: &str) -> bool {
    web_sys::window()
        .and_then(|w| w.confirm_with_message(msg).ok())
        .unwrap_or(false)
}

pub fn prompt(msg: &str) -> Option<String> {
    web_sys::window().and_then(|w| w.prompt_with_message(msg).ok().flatten())
}

pub fn fmt_date(t: chrono::DateTime<chrono::Utc>) -> String {
    t.with_timezone(&chrono::Local)
        .format("%b %-d, %Y")
        .to_string()
}

pub fn fmt_datetime(t: chrono::DateTime<chrono::Utc>) -> String {
    t.with_timezone(&chrono::Local)
        .format("%b %-d, %Y %-I:%M %p")
        .to_string()
}

/// localStorage get/set that never panics (private mode, blocked storage).
pub fn local_get(key: &str) -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(key)
        .ok()?
}

pub fn local_set(key: &str, value: &str) {
    if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = s.set_item(key, value);
    }
}
