//! User administration (admins). No self sign-up: admins create every account.

use api_types::{CreateUserRequest, Role, UpdateUserRequest, User};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::app::session;
use crate::components::{confirm, fmt_date, prompt, ErrorNotice};

#[component]
pub fn Users() -> impl IntoView {
    let s = session();
    let reload = RwSignal::new(0u32);
    let users = LocalResource::new(move || {
        reload.track();
        async move { api::users().await }
    });
    let email = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let role = RwSignal::new(Role::Estimator);
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);

    let create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        notice.set(None);
        let req = CreateUserRequest {
            email: email.get_untracked(),
            name: name.get_untracked(),
            role: role.get_untracked(),
            password: password.get_untracked(),
        };
        spawn_local(async move {
            match api::create_user(&req).await {
                Ok(u) => {
                    notice.set(Some(format!(
                        "Created {}. Share the initial password with them directly.",
                        u.email
                    )));
                    email.set(String::new());
                    name.set(String::new());
                    password.set(String::new());
                    reload.update(|n| *n += 1);
                }
                Err(e) => error.set(Some(e.message())),
            }
        });
    };

    let update = move |id: String, req: UpdateUserRequest, done: &'static str| {
        spawn_local(async move {
            match api::update_user(&id, &req).await {
                Ok(_) => {
                    notice.set(Some(done.into()));
                    reload.update(|n| *n += 1);
                }
                Err(e) => error.set(Some(e.message())),
            }
        });
    };

    let row = move |u: User| {
        let me = s.user.with_untracked(|m| m.as_ref().map(|m| m.id.clone())) == Some(u.id.clone());
        let (id1, id2, id3) = (u.id.clone(), u.id.clone(), u.id.clone());
        let disabled = u.disabled;
        let is_admin = u.role == Role::Admin;
        view! {
            <tr>
                <td><strong>{u.name.clone()}</strong>{me.then_some(" (you)")}<div class="muted small">{u.email.clone()}</div></td>
                <td>{if is_admin { "Admin" } else { "Estimator" }}</td>
                <td class="small">{u.last_login_at.map(|t| format!("Last sign-in {}", fmt_date(t))).unwrap_or_else(|| "Never signed in".into())}</td>
                <td>{if disabled { view! { <span class="badge bad">"Disabled"</span> }.into_any() } else { view! { <span class="badge ok">"Active"</span> }.into_any() }}</td>
                <td>
                    <div class="row">
                        <button class="btn small" on:click=move |_| update(id1.clone(),
                            UpdateUserRequest { role: Some(if is_admin { Role::Estimator } else { Role::Admin }), ..Default::default() },
                            "Role updated.")>
                            {if is_admin { "Make estimator" } else { "Make admin" }}
                        </button>
                        <button class="btn small" on:click=move |_| {
                            if let Some(pw) = prompt("New password (10+ characters). They'll be signed out everywhere.") {
                                update(id2.clone(), UpdateUserRequest { password: Some(pw), ..Default::default() }, "Password reset.");
                            }
                        }>"Reset password"</button>
                        {(!me).then(|| view! {
                            <button class="btn small danger" on:click=move |_| {
                                if disabled || confirm("Disable this user? They'll be signed out and their API keys stop working.") {
                                    update(id3.clone(), UpdateUserRequest { disabled: Some(!disabled), ..Default::default() },
                                        if disabled { "User enabled." } else { "User disabled." });
                                }
                            }>{if disabled { "Enable" } else { "Disable" }}</button>
                        })}
                    </div>
                </td>
            </tr>
        }
    };

    view! {
        <h1>"Users"</h1>
        <Show when=move || s.is_admin() fallback=|| view! { <p class="notice error">"Admins only."</p> }>
            <div class="card">
                <h2>"Add a user"</h2>
                <form on:submit=create>
                    <div class="field">
                        <label>"Name"</label>
                        <input required prop:value=name on:input=move |ev| name.set(event_target_value(&ev)) />
                    </div>
                    <div class="field">
                        <label>"Email"</label>
                        <input type="email" inputmode="email" required prop:value=email on:input=move |ev| email.set(event_target_value(&ev)) />
                    </div>
                    <div class="grid2">
                        <div class="field">
                            <label>"Role"</label>
                            <select on:change=move |ev| role.set(if event_target_value(&ev) == "admin" { Role::Admin } else { Role::Estimator })>
                                <option value="estimator" selected>"Estimator"</option>
                                <option value="admin">"Admin"</option>
                            </select>
                        </div>
                        <div class="field">
                            <label>"Initial password"</label>
                            <input type="text" autocomplete="off" minlength="10" required prop:value=password
                                on:input=move |ev| password.set(event_target_value(&ev)) />
                        </div>
                    </div>
                    <button class="btn primary" type="submit">"Create user"</button>
                </form>
            </div>
            <ErrorNotice message=error />
            {move || notice.get().map(|n| view! { <p class="notice tip">{n}</p> })}
            <div class="card">
                {move || match users.get() {
                    None => view! { <p class="muted">"Loading..."</p> }.into_any(),
                    Some(Err(e)) => view! { <p class="notice error">{e.message()}</p> }.into_any(),
                    Some(Ok(list)) => view! {
                        <table class="stack-table">
                            <thead><tr><th>"User"</th><th>"Role"</th><th>"Activity"</th><th>"Status"</th><th></th></tr></thead>
                            <tbody>{list.into_iter().map(row).collect_view()}</tbody>
                        </table>
                    }.into_any(),
                }}
            </div>
        </Show>
    }
}
