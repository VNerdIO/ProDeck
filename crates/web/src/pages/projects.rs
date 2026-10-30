use api_types::ProjectStatus;
use leptos::prelude::*;
use leptos_router::components::A;

use crate::api;
use crate::components::fmt_date;

#[component]
pub fn Projects() -> impl IntoView {
    let q = RwSignal::new(String::new());
    let mine = RwSignal::new(false);
    let list = LocalResource::new(move || {
        let q = q.get();
        let mine = mine.get();
        async move { api::projects(&q, mine).await }
    });

    view! {
        <div class="row" style="margin-bottom:0.75rem">
            <h1 style="margin:0">"Projects"</h1>
            <span class="spacer"></span>
            <A href="/projects/new" attr:class="btn primary">"New project"</A>
        </div>
        <div class="card">
            <div class="field">
                <label for="search">"Search"</label>
                <input id="search" type="search" placeholder="Name, customer or address"
                    prop:value=q on:input=move |ev| q.set(event_target_value(&ev)) />
            </div>
            <label class="check">
                <input type="checkbox" prop:checked=mine on:change=move |ev| mine.set(event_target_checked(&ev)) />
                "Only mine"
            </label>
        </div>
        <div class="card">
            {move || match list.get() {
                None => view! { <p class="muted">"Loading..."</p> }.into_any(),
                Some(Err(e)) => view! { <p class="notice error">{e.message()}</p> }.into_any(),
                Some(Ok(items)) if items.is_empty() => view! {
                    <p class="muted">"No projects yet. Start one with New project."</p>
                }.into_any(),
                Some(Ok(items)) => view! {
                    <ul class="list">
                        {items.into_iter().map(|p| {
                            let href = format!("/projects/{}", p.id);
                            let sub = [p.customer.clone(), p.address.clone()]
                                .into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" - ");
                            view! {
                                <li>
                                    <A href=href attr:class="project-link">
                                        <div class="row">
                                            <span class="title">{p.name.clone()}</span>
                                            <span class="spacer"></span>
                                            {(p.status == ProjectStatus::Issued).then(|| view! { <span class="badge ok">"Issued"</span> })}
                                        </div>
                                        <div class="muted small">{sub}</div>
                                        <div class="muted small">
                                            {format!("{:.0} sq ft - {} - updated {}", p.deck_area_sqft, p.owner_name, fmt_date(p.updated_at))}
                                        </div>
                                    </A>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                }.into_any(),
            }}
        </div>
    }
}
