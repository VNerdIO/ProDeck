//! Printable pick list + cut list. Serves both the live project (`/projects/:id/print`) and
//! issued snapshots (`/projects/:id/issued/:issued`), which never change after issuing.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api;
use crate::components::fmt_datetime;
use crate::pages::editor::EstimateState;
use crate::pages::views::{CutListView, NotesView, PickListView, PlanView};

#[component]
pub fn Print() -> impl IntoView {
    let params = use_params_map();
    let id = params.read_untracked().get("id").unwrap_or_default();
    let issued_id = params.read_untracked().get("issued");

    let title = RwSignal::new(String::new());
    let subtitle = RwSignal::new(String::new());
    let stamp = RwSignal::new(String::new());
    let project = RwSignal::new(crate::pages::editor::new_project());
    let est = RwSignal::new(None::<Result<estimator::Estimate, api::Error>>);

    {
        let id = id.clone();
        spawn_local(async move {
            let p = match api::project(&id).await {
                Ok(p) => p,
                Err(e) => return est.set(Some(Err(e))),
            };
            title.set(p.name.clone());
            subtitle.set(
                [p.customer, p.address]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" - "),
            );
            match issued_id {
                Some(iid) => match api::issued(&id, &iid).await {
                    Ok(i) => {
                        stamp.set(format!(
                            "{} - issued {} by {}",
                            i.meta.label,
                            fmt_datetime(i.meta.issued_at),
                            i.meta.issued_by
                        ));
                        project.set(i.input);
                        est.set(Some(Ok(i.estimate)));
                    }
                    Err(e) => est.set(Some(Err(e))),
                },
                None => {
                    stamp.set(format!(
                        "Current design - printed {}",
                        fmt_datetime(chrono::Utc::now())
                    ));
                    let r = api::estimate(&p.input).await;
                    project.set(p.input);
                    est.set(Some(r));
                }
            }
        });
    }

    let est_sig: Signal<EstimateState> = est.into();
    let key: Signal<Option<String>> = Signal::stored(Some(id.clone()));
    let back = format!("/projects/{id}");
    view! {
        <div class="row no-print" style="margin-bottom:0.75rem">
            <A href=back attr:class="btn small">"Back to project"</A>
            <span class="spacer"></span>
            <button class="btn primary" on:click=move |_| { if let Some(w) = web_sys::window() { let _ = w.print(); } }>"Print"</button>
        </div>
        <div class="card">
            <h1>{title}</h1>
            <p class="muted">{subtitle}</p>
            <p class="small">{stamp}</p>
        </div>
        {move || est.with(|e| match e { Some(Err(err)) => Some(view! { <p class="notice error">{err.message()}</p> }), _ => None })}
        <PickListView est=est_sig storage_key=key />
        <div class="page-break"></div>
        <PlanView project=project.into() est=est_sig />
        <CutListView est=est_sig />
        <NotesView est=est_sig />
    }
}
