//! Account settings: password and API keys.

use api_types::{
    ApiKey, ApiKeyStatus, CreateApiKeyRequest, CreatedApiKey, API_KEY_MAX_DAYS, API_KEY_WARN_DAYS,
};
use chrono::{NaiveDate, TimeZone, Utc};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::app::session;
use crate::components::{confirm, fmt_date, ErrorNotice};

#[component]
pub fn Settings() -> impl IntoView {
    let s = session();
    view! {
        <h1>"Settings"</h1>
        <div class="card">
            <h2>"Account"</h2>
            {move || s.user.get().map(|u| view! {
                <p>{u.name.clone()}<br/><span class="muted">{u.email.clone()}</span></p>
            })}
        </div>
        <ApiKeys />
        <ChangePassword />
    }
}

#[component]
fn ChangePassword() -> impl IntoView {
    let current = RwSignal::new(String::new());
    let new = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let ok = RwSignal::new(false);
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        ok.set(false);
        spawn_local(async move {
            match api::change_password(&current.get_untracked(), &new.get_untracked()).await {
                Ok(()) => {
                    ok.set(true);
                    current.set(String::new());
                    new.set(String::new());
                }
                Err(e) => error.set(Some(e.message())),
            }
        });
    };
    view! {
        <div class="card">
            <h2>"Change password"</h2>
            <form on:submit=submit>
                <div class="field">
                    <label>"Current password"</label>
                    <input type="password" autocomplete="current-password" required prop:value=current
                        on:input=move |ev| current.set(event_target_value(&ev)) />
                </div>
                <div class="field">
                    <label>"New password (10+ characters)"</label>
                    <input type="password" autocomplete="new-password" minlength="10" required prop:value=new
                        on:input=move |ev| new.set(event_target_value(&ev)) />
                </div>
                <ErrorNotice message=error />
                <Show when=move || ok.get()>
                    <p class="notice tip">"Password changed. Other devices have been signed out."</p>
                </Show>
                <button class="btn primary" type="submit">"Change password"</button>
            </form>
        </div>
    }
}

fn status_badge(k: &ApiKey) -> impl IntoView {
    let days_left = (k.expires_at - Utc::now()).num_days();
    match k.status {
        ApiKeyStatus::Revoked => view! { <span class="badge bad">"Revoked"</span> }.into_any(),
        ApiKeyStatus::Expired => view! { <span class="badge bad">"Expired"</span> }.into_any(),
        ApiKeyStatus::Active if days_left < API_KEY_WARN_DAYS => {
            view! { <span class="badge warn">{format!("Expires in {} days", days_left.max(0))}</span> }.into_any()
        }
        ApiKeyStatus::Active => view! { <span class="badge ok">"Active"</span> }.into_any(),
    }
}

#[component]
fn ApiKeys() -> impl IntoView {
    let s = session();
    let show_all = RwSignal::new(false);
    let reload = RwSignal::new(0u32);
    let keys = LocalResource::new(move || {
        reload.track();
        let all = show_all.get();
        async move { api::api_keys(all).await }
    });
    let name = RwSignal::new(String::new());
    let expiry = RwSignal::new("90".to_string());
    let custom = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let created = RwSignal::new(None::<CreatedApiKey>);
    let copied = RwSignal::new(false);

    let create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let mut req = CreateApiKeyRequest {
            name: name.get_untracked(),
            expires_in_days: None,
            expires_at: None,
        };
        if expiry.get_untracked() == "custom" {
            match NaiveDate::parse_from_str(&custom.get_untracked(), "%Y-%m-%d") {
                Ok(d) => {
                    req.expires_at = Utc
                        .from_local_datetime(&d.and_hms_opt(23, 59, 0).unwrap())
                        .single()
                }
                Err(_) => return error.set(Some("Pick an expiry date.".into())),
            }
        } else {
            req.expires_in_days = expiry.get_untracked().parse().ok();
        }
        spawn_local(async move {
            match api::create_api_key(&req).await {
                Ok(c) => {
                    name.set(String::new());
                    copied.set(false);
                    created.set(Some(c));
                    reload.update(|n| *n += 1);
                }
                Err(e) => error.set(Some(e.message())),
            }
        });
    };

    let revoke = move |id: String, label: String| {
        if !confirm(&format!(
            "Revoke \"{label}\"? Anything using it stops working immediately."
        )) {
            return;
        }
        spawn_local(async move {
            match api::revoke_api_key(&id).await {
                Ok(()) => reload.update(|n| *n += 1),
                Err(e) => error.set(Some(e.message())),
            }
        });
    };

    let copy = move |token: String| {
        if let Some(w) = web_sys::window() {
            let _ = w.navigator().clipboard().write_text(&token);
            copied.set(true);
        }
    };

    let max_date = (Utc::now() + chrono::Duration::days(API_KEY_MAX_DAYS))
        .format("%Y-%m-%d")
        .to_string();
    let min_date = (Utc::now() + chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();

    view! {
        <div class="card">
            <h2>"API keys"</h2>
            <p class="muted small">
                "Keys let other tools use the ProDeck API as you ("<a href="/api/docs" target="_blank" rel="noopener">"API docs"</a>"). "
                "Every key expires; the longest allowed is " {API_KEY_MAX_DAYS} " days. A key is shown once when it's created."
            </p>
            <form on:submit=create>
                <div class="field">
                    <label>"Key name"</label>
                    <input required maxlength="80" placeholder="e.g. Estimating spreadsheet" prop:value=name
                        on:input=move |ev| name.set(event_target_value(&ev)) />
                </div>
                <div class="grid2">
                    <div class="field">
                        <label>"Expires"</label>
                        <select on:change=move |ev| expiry.set(event_target_value(&ev))>
                            <option value="30">"In 30 days"</option>
                            <option value="90" selected>"In 90 days"</option>
                            <option value="180">"In 180 days"</option>
                            <option value="365">"In 1 year"</option>
                            <option value="custom">"On a date..."</option>
                        </select>
                    </div>
                    <Show when=move || expiry.get() == "custom">
                        <div class="field">
                            <label>"Expiry date"</label>
                            <input type="date" min=min_date.clone() max=max_date.clone() prop:value=custom
                                on:input=move |ev| custom.set(event_target_value(&ev)) />
                        </div>
                    </Show>
                </div>
                <ErrorNotice message=error />
                <button class="btn primary" type="submit">"Create key"</button>
            </form>

            <Show when=move || s.is_admin()>
                <label class="check" style="margin-top:0.75rem">
                    <input type="checkbox" prop:checked=show_all on:change=move |ev| show_all.set(event_target_checked(&ev)) />
                    "Show every user's keys"
                </label>
            </Show>

            {move || match keys.get() {
                None => view! { <p class="muted">"Loading..."</p> }.into_any(),
                Some(Err(e)) => view! { <p class="notice error">{e.message()}</p> }.into_any(),
                Some(Ok(list)) if list.is_empty() => view! { <p class="muted" style="margin-top:1rem">"No API keys yet."</p> }.into_any(),
                Some(Ok(list)) => view! {
                    <table class="stack-table" style="margin-top:1rem">
                        <thead><tr><th>"Name"</th><th>"Key"</th><th>"Created"</th><th>"Expires"</th><th>"Last used"</th><th>"Status"</th><th></th></tr></thead>
                        <tbody>
                            {list.into_iter().map(|k| {
                                let id = k.id.clone();
                                let label = k.name.clone();
                                let active = k.status == ApiKeyStatus::Active;
                                let owner = show_all.get_untracked().then(|| view! { <div class="muted small">{k.owner_email.clone()}</div> });
                                view! {
                                    <tr>
                                        <td><strong>{k.name.clone()}</strong>{owner}</td>
                                        <td class="mono small">{format!("{}_...", k.prefix)}</td>
                                        <td class="small">{fmt_date(k.created_at)}</td>
                                        <td class="small">{format!("Expires {}", fmt_date(k.expires_at))}</td>
                                        <td class="small muted">{k.last_used_at.map(|t| format!("Used {}", fmt_date(t))).unwrap_or_else(|| "Never used".into())}</td>
                                        <td>{status_badge(&k)}</td>
                                        <td>{active.then(|| view! {
                                            <button class="btn small danger" on:click=move |_| revoke(id.clone(), label.clone())>"Revoke"</button>
                                        })}</td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any(),
            }}
        </div>

        {move || created.get().map(|c| {
            let token = c.token.clone();
            view! {
                <div class="modal-bg" role="dialog" aria-modal="true" aria-labelledby="new-key-title">
                    <div class="modal">
                        <h2 id="new-key-title">"Copy your new key"</h2>
                        <p class="small">"This is the only time the full key is shown. Store it somewhere safe, such as a password manager."</p>
                        <p class="token mono">{c.token.clone()}</p>
                        <p class="muted small">{format!("{} - expires {}", c.key.name, fmt_date(c.key.expires_at))}</p>
                        <div class="row">
                            <button class="btn primary" on:click=move |_| copy(token.clone())>
                                {move || if copied.get() { "Copied" } else { "Copy key" }}
                            </button>
                            <button class="btn" on:click=move |_| created.set(None)>"Done"</button>
                        </div>
                    </div>
                </div>
            }
        })}
    }
}
