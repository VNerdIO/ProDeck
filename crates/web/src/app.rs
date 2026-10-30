//! App shell: routing, the signed-in user, and the top bar.

use api_types::{Role, User};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::hooks::use_navigate;
use leptos_router::path;

use crate::api;
use crate::pages::{
    editor::Editor, login::Login, print::Print, projects::Projects, settings::Settings,
    users::Users,
};

/// The signed-in user. `None` while loading or signed out (see `loaded`).
#[derive(Clone, Copy)]
pub struct Session {
    pub user: RwSignal<Option<User>>,
    pub loaded: RwSignal<bool>,
}

impl Session {
    pub fn is_admin(&self) -> bool {
        self.user
            .with(|u| u.as_ref().is_some_and(|u| u.role == Role::Admin))
    }
}

pub fn session() -> Session {
    expect_context::<Session>()
}

/// Any 401 from the API means the session ended: go to the sign-in page.
pub fn on_unauthorized() {
    if let Some(w) = web_sys::window() {
        let path = w.location().pathname().unwrap_or_default();
        if path != "/login" {
            let _ = w.location().set_href("/login");
        }
    }
}

#[component]
pub fn App() -> impl IntoView {
    let s = Session {
        user: RwSignal::new(None),
        loaded: RwSignal::new(false),
    };
    provide_context(s);
    spawn_local(async move {
        if let Ok(u) = api::me().await {
            s.user.set(Some(u));
        }
        s.loaded.set(true);
    });

    view! {
        <Router>
            <TopBar />
            <main>
                <Routes fallback=|| view! { <p>"Page not found."</p> }>
                    <Route path=path!("/login") view=Login />
                    <Route path=path!("/") view=|| view! { <Guard><Projects /></Guard> } />
                    <Route path=path!("/projects/new") view=|| view! { <Guard><Editor /></Guard> } />
                    <Route path=path!("/projects/:id") view=|| view! { <Guard><Editor /></Guard> } />
                    <Route path=path!("/projects/:id/print") view=|| view! { <Guard><Print /></Guard> } />
                    <Route path=path!("/projects/:id/issued/:issued") view=|| view! { <Guard><Print /></Guard> } />
                    <Route path=path!("/settings") view=|| view! { <Guard><Settings /></Guard> } />
                    <Route path=path!("/admin/users") view=|| view! { <Guard><Users /></Guard> } />
                </Routes>
            </main>
        </Router>
    }
}

/// Renders children only once signed in; otherwise sends the user to sign in.
#[component]
fn Guard(children: ChildrenFn) -> impl IntoView {
    let s = session();
    let navigate = use_navigate();
    Effect::new(move |_| {
        if s.loaded.get() && s.user.with(Option::is_none) {
            navigate("/login", Default::default());
        }
    });
    move || {
        if s.user.with(Option::is_some) {
            children().into_any()
        } else {
            view! { <p class="muted">"Loading..."</p> }.into_any()
        }
    }
}

#[component]
fn TopBar() -> impl IntoView {
    let s = session();
    let sign_out = move |_| {
        spawn_local(async move {
            let _ = api::logout().await;
            s.user.set(None);
            if let Some(w) = web_sys::window() {
                let _ = w.location().set_href("/login");
            }
        });
    };
    view! {
        <header class="topbar">
            <A href="/" attr:class="brand">"ProDeck"</A>
            <Show when=move || s.user.with(Option::is_some)>
                <nav aria-label="Main">
                    <A href="/" exact=true>"Projects"</A>
                    <Show when=move || s.is_admin()>
                        <A href="/admin/users">"Users"</A>
                    </Show>
                    <A href="/settings">"Settings"</A>
                    <button on:click=sign_out>"Sign out"</button>
                </nav>
            </Show>
        </header>
    }
}
