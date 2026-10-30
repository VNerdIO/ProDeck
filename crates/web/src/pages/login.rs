use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

use crate::api;
use crate::app::session;
use crate::components::ErrorNotice;

#[component]
pub fn Login() -> impl IntoView {
    let s = session();
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let navigate = use_navigate();

    // Already signed in (e.g. back button): go home.
    let nav = navigate.clone();
    Effect::new(move |_| {
        if s.user.with(Option::is_some) {
            nav("/", Default::default());
        }
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        error.set(None);
        let navigate = navigate.clone();
        spawn_local(async move {
            match api::login(&email.get_untracked(), &password.get_untracked()).await {
                Ok(u) => {
                    s.user.set(Some(u));
                    navigate("/", Default::default());
                }
                Err(e) => error.set(Some(e.message())),
            }
            busy.set(false);
        });
    };

    view! {
        <div class="login-wrap">
            <div class="card">
                <h1>"Sign in"</h1>
                <p class="muted small">"Deck material estimates for the crew and the store run."</p>
                <form on:submit=submit>
                    <div class="field">
                        <label for="email">"Email"</label>
                        <input id="email" type="email" autocomplete="username" inputmode="email" required
                            prop:value=email on:input=move |ev| email.set(event_target_value(&ev)) />
                    </div>
                    <div class="field">
                        <label for="password">"Password"</label>
                        <input id="password" type="password" autocomplete="current-password" required
                            prop:value=password on:input=move |ev| password.set(event_target_value(&ev)) />
                    </div>
                    <ErrorNotice message=error />
                    <button class="btn primary block" type="submit" disabled=busy>
                        {move || if busy.get() { "Signing in..." } else { "Sign in" }}
                    </button>
                </form>
                <p class="muted small" style="margin-top:1rem">"Accounts are created by your admin."</p>
            </div>
        </div>
    }
}
