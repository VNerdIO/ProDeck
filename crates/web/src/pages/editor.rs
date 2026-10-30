//! Project editor: deck design form with a live estimate.
//!
//! Phones get a tab per view (Design / Plan / Pick list / Cuts / Notes). From 720px up the
//! design form sits beside the results.

use std::time::Duration;

use api_types::{IssuedEstimateMeta, ProjectStatus, SaveProjectRequest};
use estimator::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::api;
use crate::components::*;
use crate::pages::views::{CutListView, NotesView, PickListView, PlanView};

pub fn new_project() -> Project {
    Project {
        sections: vec![Section {
            name: "Main deck".into(),
            x: 0.0,
            y: 0.0,
            width: 16.0 * 12.0,
            depth: 12.0 * 12.0,
            height: 36.0,
            joists_along: Axis::NorthSouth,
            ledger_edge: Some(Edge::North),
            railing: None,
        }],
        stairs: vec![],
        materials: MaterialChoices {
            framing: FramingSpecies::SouthernPine,
            decking: DeckingType::Pt54x6,
            fastening: Fastening::FaceScrews,
            railing: RailingType::WoodBalusters,
            footing: FootingType::PouredBags,
            ledger_fastener: LedgerFastener::StructuralScrews,
            joist_spacing: None,
            min_joist: LumberSize::TwoBy8,
            picture_frame: false,
            joist_tape: true,
            fascia: false,
        },
        site: SiteConditions::default(),
        options: EstimateOptions::default(),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Design,
    Plan,
    List,
    Cuts,
    Notes,
}

pub type EstimateState = Option<Result<Estimate, api::Error>>;

#[component]
pub fn Editor() -> impl IntoView {
    let params = use_params_map();
    let navigate = use_navigate();
    let id = RwSignal::new(params.read_untracked().get("id"));

    let name = RwSignal::new("New deck".to_string());
    let customer = RwSignal::new(String::new());
    let address = RwSignal::new(String::new());
    let notes = RwSignal::new(String::new());
    let project = RwSignal::new(new_project());
    let issued = RwSignal::new(Vec::<IssuedEstimateMeta>::new());
    let status = RwSignal::new(ProjectStatus::Draft);
    let loaded = RwSignal::new(id.get_untracked().is_none());
    let dirty = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);
    let tab = RwSignal::new(Tab::Design);
    let estimate = RwSignal::new(None::<Result<Estimate, api::Error>>);

    if let Some(pid) = id.get_untracked() {
        spawn_local(async move {
            match api::project(&pid).await {
                Ok(p) => {
                    name.set(p.name);
                    customer.set(p.customer);
                    address.set(p.address);
                    notes.set(p.notes);
                    project.set(p.input);
                    issued.set(p.issued);
                    status.set(p.status);
                    loaded.set(true);
                    dirty.set(false);
                }
                Err(e) => error.set(Some(e.message())),
            }
        });
    }

    // Live estimate, debounced so typing doesn't flood the API.
    let version = StoredValue::new(0u64);
    Effect::new(move |_| {
        let p = project.get();
        if !loaded.get() {
            return;
        }
        version.update_value(|v| *v += 1);
        let mine = version.get_value();
        spawn_local(async move {
            gloo_timers::future::sleep(Duration::from_millis(350)).await;
            if version.get_value() != mine {
                return;
            }
            let r = api::estimate(&p).await;
            if version.get_value() == mine {
                estimate.set(Some(r));
            }
        });
    });

    let edit = move |f: &dyn Fn(&mut Project)| {
        project.update(|p| f(p));
        dirty.set(true);
    };

    let save = {
        let navigate = navigate.clone();
        move || {
            saving.set(true);
            error.set(None);
            let req = SaveProjectRequest {
                name: name.get_untracked(),
                customer: customer.get_untracked(),
                address: address.get_untracked(),
                notes: notes.get_untracked(),
                input: project.get_untracked(),
            };
            let navigate = navigate.clone();
            spawn_local(async move {
                let r = match id.get_untracked() {
                    Some(pid) => api::save_project(&pid, &req).await,
                    None => api::create_project(&req).await,
                };
                match r {
                    Ok(p) => {
                        let was_new = id.get_untracked().is_none();
                        id.set(Some(p.id.clone()));
                        issued.set(p.issued);
                        status.set(p.status);
                        dirty.set(false);
                        if was_new {
                            navigate(&format!("/projects/{}", p.id), Default::default());
                        }
                    }
                    Err(e) => error.set(Some(e.message())),
                }
                saving.set(false);
            });
        }
    };

    let duplicate = move |_| {
        let Some(pid) = id.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match api::duplicate_project(&pid).await {
                // Full navigation so the editor reloads with the copy.
                Ok(p) => {
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href(&format!("/projects/{}", p.id));
                    }
                }
                Err(e) => error.set(Some(e.message())),
            }
        });
    };

    let delete = {
        let navigate = navigate.clone();
        move |_| {
            let Some(pid) = id.get_untracked() else {
                return;
            };
            if !confirm("Delete this project and its issued estimates? This can't be undone.") {
                return;
            }
            let navigate = navigate.clone();
            spawn_local(async move {
                match api::delete_project(&pid).await {
                    Ok(()) => navigate("/", Default::default()),
                    Err(e) => error.set(Some(e.message())),
                }
            });
        }
    };

    let issue = move |_| {
        let Some(pid) = id.get_untracked() else {
            return;
        };
        if dirty.get_untracked() {
            error.set(Some("Save your changes before issuing.".into()));
            return;
        }
        let Some(label) =
            prompt("Label for this estimate (e.g. Customer quote, Final pickup list):")
        else {
            return;
        };
        spawn_local(async move {
            match api::issue(&pid, &label).await {
                Ok(i) => {
                    issued.update(|v| v.insert(0, i.meta));
                    status.set(ProjectStatus::Issued);
                }
                Err(e) => error.set(Some(e.message())),
            }
        });
    };

    let save_click = save.clone();
    let tab_btn = move |t: Tab, label: &'static str| {
        view! {
            <button role="tab" aria-selected=move || (tab.get() == t).to_string() on:click=move |_| tab.set(t)>{label}</button>
        }
    };
    let pane = move |t: Tab| {
        move || {
            if tab.get() == t {
                "pane"
            } else {
                "pane hidden-pane"
            }
        }
    };
    let est: Signal<EstimateState> = estimate.into();
    let pid_signal: Signal<Option<String>> = id.into();

    view! {
        <style>".hidden-pane{display:none}"</style>
        <div class="row no-print" style="margin-bottom:0.75rem">
            <A href="/" attr:class="btn small">"Back"</A>
            <span class="spacer"></span>
            <Show when=move || id.get().is_some()>
                <A href=move || format!("/projects/{}/print", id.get().unwrap_or_default()) attr:class="btn small">"Print"</A>
                <button class="btn small" on:click=issue>"Issue"</button>
                <button class="btn small" on:click=duplicate>"Duplicate"</button>
                <button class="btn small danger" on:click=delete.clone()>"Delete"</button>
            </Show>
            <button class="btn primary" disabled=move || saving.get() || !loaded.get()
                on:click=move |_| save_click()>
                {move || if saving.get() { "Saving..." } else if dirty.get() || id.get().is_none() { "Save" } else { "Saved" }}
            </button>
        </div>
        <ErrorNotice message=error />

        <div class="editor-wide">
            <div class="tabs" role="tablist">
                {tab_btn(Tab::Design, "Design")}
                {tab_btn(Tab::Plan, "Plan")}
                {tab_btn(Tab::List, "Pick list")}
                {tab_btn(Tab::Cuts, "Cuts")}
                {tab_btn(Tab::Notes, "Notes")}
            </div>

            <div class=pane(Tab::Design)>
                <div class="card">
                    <h2>"Job"</h2>
                    <div class="field">
                        <label>"Project name"</label>
                        <input prop:value=name on:input=move |ev| { name.set(event_target_value(&ev)); dirty.set(true); } />
                    </div>
                    <div class="field">
                        <label>"Customer"</label>
                        <input prop:value=customer autocomplete="name"
                            on:input=move |ev| { customer.set(event_target_value(&ev)); dirty.set(true); } />
                    </div>
                    <div class="field">
                        <label>"Address"</label>
                        <input prop:value=address autocomplete="street-address"
                            on:input=move |ev| { address.set(event_target_value(&ev)); dirty.set(true); } />
                    </div>
                    <Show when=move || !issued.with(Vec::is_empty)>
                        <h3>"Issued estimates"</h3>
                        <ul class="list small">
                            {move || issued.get().into_iter().map(|m| {
                                let href = format!("/projects/{}/issued/{}", id.get_untracked().unwrap_or_default(), m.id);
                                view! { <li style="padding:0.4rem 0"><A href=href>{m.label}</A>
                                    <span class="muted">{format!(" - {} by {}", fmt_datetime(m.issued_at), m.issued_by)}</span></li> }
                            }).collect_view()}
                        </ul>
                    </Show>
                </div>
                <SectionsForm project edit />
                <StairsForm project edit />
                <MaterialsForm project edit />
                <div class="card">
                    <h2>"Notes"</h2>
                    <textarea prop:value=notes placeholder="Access, haul-away, customer preferences..."
                        on:input=move |ev| { notes.set(event_target_value(&ev)); dirty.set(true); }></textarea>
                </div>
            </div>

            <div class="pane-right">
                <EstimateStatus est />
                <div class=pane(Tab::Plan)><PlanView project=project.into() est /></div>
                <div class=pane(Tab::List)><PickListView est storage_key=pid_signal /></div>
                <div class=pane(Tab::Cuts)><CutListView est /></div>
                <div class=pane(Tab::Notes)><NotesView est /></div>
            </div>
        </div>
    }
}

/// Validation problems with the design, shown above every results pane.
#[component]
fn EstimateStatus(est: Signal<EstimateState>) -> impl IntoView {
    move || match est.get() {
        None => Some(view! { <p class="muted small">"Calculating..."</p> }.into_any()),
        Some(Err(e)) => Some(view! { <p class="notice error">{e.message()}</p> }.into_any()),
        Some(Ok(e)) => {
            let s = e.summary.clone();
            Some(
                view! {
                    <p class="muted small">
                        {format!("{:.0} sq ft - {} pieces of lumber - {:.0} linear ft - {:.1}% offcut waste",
                            s.deck_area_sqft, s.lumber_pieces, s.linear_feet, s.waste_pct)}
                    </p>
                }
                .into_any(),
            )
        }
    }
}

type Edit = Callback<Box<dyn Fn(&mut Project) + Send + Sync>>;

fn edit_cb(edit: impl Fn(&dyn Fn(&mut Project)) + Copy + Send + Sync + 'static) -> Edit {
    Callback::new(move |f: Box<dyn Fn(&mut Project) + Send + Sync>| edit(&*f))
}

fn sec_num(project: RwSignal<Project>, i: usize, get: fn(&Section) -> f64) -> Signal<f64> {
    Signal::derive(move || project.with(|p| p.sections.get(i).map(get).unwrap_or(0.0)))
}

fn sec_set(e: Edit, i: usize, set: fn(&mut Section, f64)) -> Callback<f64> {
    Callback::new(move |v: f64| {
        e.run(Box::new(move |p: &mut Project| {
            if let Some(s) = p.sections.get_mut(i) {
                set(s, v)
            }
        }))
    })
}

const EDGES: [(Edge, &str); 4] = [
    (Edge::North, "North"),
    (Edge::South, "South"),
    (Edge::East, "East"),
    (Edge::West, "West"),
];

#[component]
fn SectionsForm(
    project: RwSignal<Project>,
    edit: impl Fn(&dyn Fn(&mut Project)) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let e = edit_cb(edit);
    let count = Memo::new(move |_| project.with(|p| p.sections.len()));
    let add = move |_| {
        e.run(Box::new(|p: &mut Project| {
            let x = p.sections.iter().map(|s| s.x + s.width).fold(0.0, f64::max);
            let h = p.sections.last().map(|s| s.height).unwrap_or(36.0);
            let n = p.sections.len() + 1;
            p.sections.push(Section {
                name: format!("Section {n}"),
                x,
                y: 0.0,
                width: 96.0,
                depth: 96.0,
                height: h,
                joists_along: Axis::NorthSouth,
                ledger_edge: None,
                railing: None,
            });
        }))
    };
    view! {
        <div class="card">
            <h2>"Deck sections"</h2>
            <p class="muted small">"One rectangle per section. Combine sections for L, T or multi-level decks. North is the house side by default."</p>
            {move || (0..count.get()).map(|i| view! { <SectionCard project e i count /> }).collect_view()}
            <button class="btn block" on:click=add>"Add section"</button>
        </div>
    }
}

#[component]
fn SectionCard(project: RwSignal<Project>, e: Edit, i: usize, count: Memo<usize>) -> impl IntoView {
    let name = Signal::derive(move || {
        project.with(|p| {
            p.sections
                .get(i)
                .map(|s| s.name.clone())
                .unwrap_or_default()
        })
    });
    let axis = Signal::derive(move || {
        project.with(|p| {
            p.sections
                .get(i)
                .map(|s| s.joists_along)
                .unwrap_or(Axis::NorthSouth)
        })
    });
    let ledger =
        Signal::derive(move || project.with(|p| p.sections.get(i).and_then(|s| s.ledger_edge)));
    let railing =
        Signal::derive(move || project.with(|p| p.sections.get(i).and_then(|s| s.railing)));
    let show_pos = RwSignal::new(i > 0);
    let multi = move || count.get() > 1;
    let positioned = move || multi() || show_pos.get();
    let remove = move |_| {
        if confirm("Remove this section?") {
            e.run(Box::new(move |p: &mut Project| {
                if i < p.sections.len() {
                    p.sections.remove(i);
                    p.stairs.retain(|s| s.section != i);
                    for s in p.stairs.iter_mut().filter(|s| s.section > i) {
                        s.section -= 1;
                    }
                }
            }))
        }
    };
    view! {
        <div class="card section-card">
            <div class="field">
                <label>"Section name"</label>
                <input prop:value=name on:change=move |ev| {
                    let v = event_target_value(&ev);
                    e.run(Box::new(move |p: &mut Project| if let Some(s) = p.sections.get_mut(i) { s.name = v.clone() }))
                } />
            </div>
            <FtIn label="Width (east-west)" value=sec_num(project, i, |s| s.width) on_change=sec_set(e, i, |s, v| s.width = v) />
            <FtIn label="Depth (north-south)" value=sec_num(project, i, |s| s.depth) on_change=sec_set(e, i, |s, v| s.depth = v) />
            <FtIn label="Height, top of deck to grade" value=sec_num(project, i, |s| s.height) on_change=sec_set(e, i, |s, v| s.height = v) />
            <div class="grid2">
                <EnumSelect label="Joists run" value=axis
                    options=vec![(Axis::NorthSouth, "North-south"), (Axis::EastWest, "East-west")]
                    on_change=Callback::new(move |v: Axis| e.run(Box::new(move |p: &mut Project| {
                        if let Some(s) = p.sections.get_mut(i) {
                            s.joists_along = v;
                            // Keep the ledger perpendicular to the joists.
                            if s.ledger_edge.is_some_and(|l| l.runs_along() == v) {
                                s.ledger_edge = Some(if v == Axis::NorthSouth { Edge::North } else { Edge::West });
                            }
                        }
                    }))) />
                <OptEnumSelect label="Attached to house" none_label="Freestanding" value=ledger
                    options=EDGES.to_vec()
                    on_change=Callback::new(move |v: Option<Edge>| e.run(Box::new(move |p: &mut Project| {
                        if let Some(s) = p.sections.get_mut(i) { s.ledger_edge = v }
                    }))) />
            </div>
            <OptEnumSelect label="Guard railing" none_label="Automatic (over 30\")" value=railing
                options=vec![(true, "Yes, on open sides"), (false, "No")]
                on_change=Callback::new(move |v: Option<bool>| e.run(Box::new(move |p: &mut Project| {
                    if let Some(s) = p.sections.get_mut(i) { s.railing = v }
                }))) />
            <Show when=positioned
                fallback=move || view! { <button class="btn small" on:click=move |_| show_pos.set(true)>"Position..."</button> }>
                <div class="grid2">
                    <FtIn label="From west" value=sec_num(project, i, |s| s.x) on_change=sec_set(e, i, |s, v| s.x = v) />
                    <FtIn label="From north" value=sec_num(project, i, |s| s.y) on_change=sec_set(e, i, |s, v| s.y = v) />
                </div>
            </Show>
            <Show when=multi>
                <button class="btn small danger" on:click=remove>"Remove section"</button>
            </Show>
        </div>
    }
}

#[component]
fn StairsForm(
    project: RwSignal<Project>,
    edit: impl Fn(&dyn Fn(&mut Project)) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let e = edit_cb(edit);
    let count = Memo::new(move |_| project.with(|p| p.stairs.len()));
    let add = move |_| {
        e.run(Box::new(|p: &mut Project| {
            let (section, edge, len) = p
                .sections
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let edge = s.ledger_edge.map(Edge::opposite).unwrap_or(Edge::South);
                    let len = if edge.runs_along() == Axis::EastWest {
                        s.width
                    } else {
                        s.depth
                    };
                    (i, edge, len)
                })
                .next()
                .unwrap_or((0, Edge::South, 96.0));
            p.stairs.push(Stair {
                section,
                edge,
                offset: ((len - 48.0) / 2.0).max(0.0).round(),
                width: 48.0,
                rise: None,
            });
        }))
    };
    view! {
        <div class="card">
            <h2>"Stairs"</h2>
            {move || (0..count.get()).map(|i| view! { <StairCard project e i /> }).collect_view()}
            <button class="btn block" on:click=add>"Add stairs"</button>
        </div>
    }
}

#[component]
fn StairCard(project: RwSignal<Project>, e: Edit, i: usize) -> impl IntoView {
    let st = move |f: fn(&Stair) -> f64| {
        Signal::derive(move || project.with(|p| p.stairs.get(i).map(f).unwrap_or(0.0)))
    };
    let set = move |f: fn(&mut Stair, f64)| {
        Callback::new(move |v: f64| {
            e.run(Box::new(move |p: &mut Project| {
                if let Some(s) = p.stairs.get_mut(i) {
                    f(s, v)
                }
            }))
        })
    };
    let edge = Signal::derive(move || {
        project.with(|p| p.stairs.get(i).map(|s| s.edge).unwrap_or(Edge::South))
    });
    let section =
        Signal::derive(move || project.with(|p| p.stairs.get(i).map(|s| s.section).unwrap_or(0)));
    let names = Memo::new(move |_| {
        project.with(|p| {
            p.sections
                .iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>()
        })
    });
    view! {
        <div class="card section-card">
            <div class="grid2">
                <div class="field">
                    <label>"From section"</label>
                    <select on:change=move |ev| {
                        let v: usize = event_target_value(&ev).parse().unwrap_or(0);
                        e.run(Box::new(move |p: &mut Project| if let Some(s) = p.stairs.get_mut(i) { s.section = v }))
                    }>
                        {move || names.get().into_iter().enumerate().map(|(k, n)| view! {
                            <option value=k.to_string() selected=move || section.get() == k>{n}</option>
                        }).collect_view()}
                    </select>
                </div>
                <EnumSelect label="Edge" value=edge options=EDGES.to_vec()
                    on_change=Callback::new(move |v: Edge| e.run(Box::new(move |p: &mut Project| if let Some(s) = p.stairs.get_mut(i) { s.edge = v }))) />
            </div>
            <div class="grid2">
                <FtIn label="Width" value=st(|s| s.width) on_change=set(|s, v| s.width = v) />
                <FtIn label="Offset along edge" value=st(|s| s.offset) on_change=set(|s, v| s.offset = v) />
            </div>
            <button class="btn small danger" on:click=move |_| e.run(Box::new(move |p: &mut Project| { if i < p.stairs.len() { p.stairs.remove(i); } }))>
                "Remove stairs"
            </button>
        </div>
    }
}

#[component]
fn MaterialsForm(
    project: RwSignal<Project>,
    edit: impl Fn(&dyn Fn(&mut Project)) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let e = edit_cb(edit);
    macro_rules! field {
        ($get:expr) => {
            Signal::derive(move || project.with($get))
        };
    }
    macro_rules! setter {
        ($t:ty, $set:expr) => {
            Callback::new(move |v: $t| e.run(Box::new(move |p: &mut Project| ($set)(p, v.clone()))))
        };
    }
    view! {
        <div class="card">
            <h2>"Materials"</h2>
            <EnumSelect label="Decking" value=field!(|p: &Project| p.materials.decking)
                options=vec![(DeckingType::Pt54x6, "PT 5/4x6"), (DeckingType::Pt2x6, "PT 2x6"),
                    (DeckingType::Cedar54x6, "Cedar 5/4x6"), (DeckingType::Composite, "Composite")]
                on_change=setter!(DeckingType, |p: &mut Project, v| p.materials.decking = v) />
            <EnumSelect label="Decking fasteners" value=field!(|p: &Project| p.materials.fastening)
                options=vec![(Fastening::FaceScrews, "Face screws"), (Fastening::HiddenClips, "Hidden clips")]
                on_change=setter!(Fastening, |p: &mut Project, v| p.materials.fastening = v) />
            <EnumSelect label="Framing lumber" value=field!(|p: &Project| p.materials.framing)
                options=vec![(FramingSpecies::SouthernPine, "PT Southern Pine #2"),
                    (FramingSpecies::DouglasFirLarch, "PT Douglas Fir-Larch #2"), (FramingSpecies::HemFir, "PT Hem-Fir #2")]
                on_change=setter!(FramingSpecies, |p: &mut Project, v| p.materials.framing = v) />
            <div class="grid2">
                <OptEnumSelect label="Joist spacing" none_label="Widest allowed" value=field!(|p: &Project| p.materials.joist_spacing)
                    options=vec![(JoistSpacing::Oc12, "12\" OC"), (JoistSpacing::Oc16, "16\" OC"), (JoistSpacing::Oc24, "24\" OC")]
                    on_change=setter!(Option<JoistSpacing>, |p: &mut Project, v| p.materials.joist_spacing = v) />
                <EnumSelect label="Smallest joist" value=field!(|p: &Project| p.materials.min_joist)
                    options=vec![(LumberSize::TwoBy6, "2x6"), (LumberSize::TwoBy8, "2x8"), (LumberSize::TwoBy10, "2x10"), (LumberSize::TwoBy12, "2x12")]
                    on_change=setter!(LumberSize, |p: &mut Project, v| p.materials.min_joist = v) />
            </div>
            <EnumSelect label="Railing" value=field!(|p: &Project| p.materials.railing)
                options=vec![(RailingType::WoodBalusters, "Wood 2x2 balusters"), (RailingType::MetalBalusters, "Aluminum balusters"),
                    (RailingType::PanelKits, "Railing kits (composite/vinyl)")]
                on_change=setter!(RailingType, |p: &mut Project, v| p.materials.railing = v) />
            <div class="grid2">
                <EnumSelect label="Footings" value=field!(|p: &Project| p.materials.footing)
                    options=vec![(FootingType::PouredBags, "Poured (bags)"), (FootingType::PrecastBlocks, "Precast blocks")]
                    on_change=setter!(FootingType, |p: &mut Project, v| p.materials.footing = v) />
                <EnumSelect label="Ledger fasteners" value=field!(|p: &Project| p.materials.ledger_fastener)
                    options=vec![(LedgerFastener::StructuralScrews, "Structural screws"), (LedgerFastener::HalfInchLags, "1/2\" lags")]
                    on_change=setter!(LedgerFastener, |p: &mut Project, v| p.materials.ledger_fastener = v) />
            </div>
            <Check label="Picture-frame border" value=field!(|p: &Project| p.materials.picture_frame)
                on_change=setter!(bool, |p: &mut Project, v| p.materials.picture_frame = v) />
            <Check label="Fascia on exposed rims" value=field!(|p: &Project| p.materials.fascia)
                on_change=setter!(bool, |p: &mut Project, v| p.materials.fascia = v) />
            <Check label="Joist tape" value=field!(|p: &Project| p.materials.joist_tape)
                on_change=setter!(bool, |p: &mut Project, v| p.materials.joist_tape = v) />

            <h3 style="margin-top:1rem">"Site"</h3>
            <FtIn label="Frost depth" value=field!(|p: &Project| p.site.frost_depth)
                on_change=setter!(f64, |p: &mut Project, v| p.site.frost_depth = v) />
            <div class="grid2">
                <Num label="Live load" suffix="psf" value=field!(|p: &Project| p.site.live_load_psf)
                    on_change=setter!(f64, |p: &mut Project, v| p.site.live_load_psf = v) />
                <Num label="Soil bearing" suffix="psf" step=100.0 value=field!(|p: &Project| p.site.soil_bearing_psf)
                    on_change=setter!(f64, |p: &mut Project, v| p.site.soil_bearing_psf = v) />
            </div>

            <h3 style="margin-top:1rem">"Buying"</h3>
            <div class="grid2">
                <Num label="Decking spares" suffix="%" value=field!(|p: &Project| p.options.decking_overage_pct)
                    on_change=setter!(f64, |p: &mut Project, v| p.options.decking_overage_pct = v) />
                <EnumSelect label="Longest board" value=field!(|p: &Project| p.options.max_stock_ft)
                    options=vec![(12u32, "12'"), (14, "14'"), (16, "16'"), (18, "18'"), (20, "20'")]
                    on_change=setter!(u32, |p: &mut Project, v| p.options.max_stock_ft = v) />
            </div>
        </div>
    }
}
