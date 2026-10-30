//! Estimate result views shared by the editor and the print page.

use std::collections::{BTreeMap, HashSet};

use estimator::materials::Category;
use estimator::{fmt_ft_in, CutList, Edge, Estimate, Project};
use leptos::prelude::*;

use crate::components::{local_get, local_set};
use crate::pages::editor::EstimateState;

fn ready(est: &EstimateState) -> Option<&Estimate> {
    est.as_ref().and_then(|r| r.as_ref().ok())
}

// ---- Plan view ----

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn stair_rect(p: &Project, e: &Estimate, i: usize) -> Option<(Rect, Edge, usize)> {
    let st = p.stairs.get(i)?;
    let d = e.stairs.get(i)?;
    let s = p.sections.get(st.section)?;
    let run = d.total_run.max(12.0);
    let r = match st.edge {
        Edge::South => Rect {
            x: s.x + st.offset,
            y: s.y + s.depth,
            w: st.width,
            h: run,
        },
        Edge::North => Rect {
            x: s.x + st.offset,
            y: s.y - run,
            w: st.width,
            h: run,
        },
        Edge::East => Rect {
            x: s.x + s.width,
            y: s.y + st.offset,
            w: run,
            h: st.width,
        },
        Edge::West => Rect {
            x: s.x - run,
            y: s.y + st.offset,
            w: run,
            h: st.width,
        },
    };
    Some((r, st.edge, d.treads))
}

#[component]
pub fn PlanView(project: Signal<Project>, est: Signal<EstimateState>) -> impl IntoView {
    move || {
        let est = est.get();
        let Some(e) = ready(&est) else {
            return view! { <div class="card"><p class="muted">"The plan appears once the design is valid."</p></div> }.into_any();
        };
        let p = project.get();
        let mut rects: Vec<Rect> = p
            .sections
            .iter()
            .map(|s| Rect {
                x: s.x,
                y: s.y,
                w: s.width,
                h: s.depth,
            })
            .collect();
        let stairs: Vec<_> = (0..p.stairs.len())
            .filter_map(|i| stair_rect(&p, e, i))
            .collect();
        rects.extend(stairs.iter().map(|(r, _, _)| *r));
        let minx = rects.iter().map(|r| r.x).fold(f64::MAX, f64::min);
        let miny = rects.iter().map(|r| r.y).fold(f64::MAX, f64::min);
        let maxx = rects.iter().map(|r| r.x + r.w).fold(f64::MIN, f64::max);
        let maxy = rects.iter().map(|r| r.y + r.h).fold(f64::MIN, f64::max);
        let pad = 30.0;
        let span = (maxx - minx).max(maxy - miny);
        let fs = (span / 28.0).max(6.0);
        let post = (span / 70.0).max(5.5);
        let vb = format!(
            "{} {} {} {}",
            minx - pad,
            miny - pad - fs,
            maxx - minx + 2.0 * pad,
            maxy - miny + 2.0 * pad + fs
        );

        let sections = e.sections.iter().zip(p.sections.iter()).map(|(d, s)| {
            let pts = d.plan.outline.iter().map(|q| format!("{},{}", q.x, q.y)).collect::<Vec<_>>().join(" ");
            let joists = d.plan.joists.iter().map(|j| view! {
                <line class="joist" x1=j.a.x y1=j.a.y x2=j.b.x y2=j.b.y />
            }).collect_view();
            let beams = d.plan.beams.iter().map(|b| view! {
                <line class="beam" x1=b.a.x y1=b.a.y x2=b.b.x y2=b.b.y />
            }).collect_view();
            let posts = d.plan.posts.iter().map(|q| view! {
                <rect class="post" x=q.x - post / 2.0 y=q.y - post / 2.0 width=post height=post />
            }).collect_view();
            let ledger = d.plan.ledger.map(|l| view! { <line class="ledger" x1=l.a.x y1=l.a.y x2=l.b.x y2=l.b.y /> });
            let cx = s.x + s.width / 2.0;
            let cy = s.y + s.depth / 2.0;
            view! {
                <g>
                    <polygon class="deck" points=pts />
                    {joists}{beams}{posts}{ledger}
                    <text class="label" x=cx y=cy - fs * 0.2 text-anchor="middle" font-size=fs>{s.name.clone()}</text>
                    <text class="label" x=cx y=cy + fs * 1.0 text-anchor="middle" font-size=fs * 0.8 style="font-weight:400">
                        {format!("{} x {}", fmt_ft_in(s.width), fmt_ft_in(s.depth))}
                    </text>
                </g>
            }
        }).collect_view();

        let stair_views = stairs.into_iter().map(|(r, edge, treads)| {
            let lines = (1..=treads).map(|k| {
                let f = k as f64 / (treads + 1) as f64;
                match edge {
                    Edge::North | Edge::South => view! { <line class="joist" x1=r.x y1=r.y + r.h * f x2=r.x + r.w y2=r.y + r.h * f /> },
                    Edge::East | Edge::West => view! { <line class="joist" x1=r.x + r.w * f y1=r.y x2=r.x + r.w * f y2=r.y + r.h /> },
                }
            }).collect_view();
            view! { <g><rect class="stair" x=r.x y=r.y width=r.w height=r.h />{lines}</g> }
        }).collect_view();

        view! {
            <div class="card plan">
                <svg viewBox=vb role="img" aria-label="Deck framing plan">
                    <text class="label" x=minx y=miny - pad / 2.0 font-size=fs * 0.8>"N (house side by default)"</text>
                    {sections}
                    {stair_views}
                </svg>
                <div class="legend">
                    <span><i style="background:var(--danger)"></i>"Ledger"</span>
                    <span><i style="background:var(--accent)"></i>"Beam"</span>
                    <span><i style="background:var(--muted)"></i>"Joists"</span>
                    <span><i style="background:var(--text);height:8px;width:8px"></i>"Posts/footings"</span>
                </div>
            </div>
        }.into_any()
    }
}

// ---- Pick list ----

fn item_key(desc: &str, size: &Option<String>) -> String {
    format!("{desc}|{}", size.clone().unwrap_or_default())
}

const CATEGORIES: [Category; 5] = [
    Category::Lumber,
    Category::Decking,
    Category::Railing,
    Category::Concrete,
    Category::Hardware,
];

/// Store pick list with tap-to-check rows. Checks are remembered on this device per project.
#[component]
pub fn PickListView(
    est: Signal<EstimateState>,
    storage_key: Signal<Option<String>>,
) -> impl IntoView {
    let checked = RwSignal::new(HashSet::<String>::new());
    Effect::new(move |_| {
        if let Some(k) = storage_key.get() {
            let set = local_get(&format!("picklist:{k}"))
                .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
                .unwrap_or_default();
            checked.set(set.into_iter().collect());
        }
    });
    let persist = move || {
        if let Some(k) = storage_key.get_untracked() {
            let v: Vec<String> = checked.with_untracked(|c| c.iter().cloned().collect());
            local_set(
                &format!("picklist:{k}"),
                &serde_json::to_string(&v).unwrap_or_default(),
            );
        }
    };
    let toggle = move |key: String| {
        checked.update(|c| {
            if !c.remove(&key) {
                c.insert(key);
            }
        });
        persist();
    };

    move || {
        let est = est.get();
        let Some(e) = ready(&est) else {
            return view! { <div class="card"><p class="muted">"Fix the design to see the pick list."</p></div> }.into_any();
        };
        let total = e.line_items.len();
        let done = Memo::new({
            let keys: Vec<String> = e
                .line_items
                .iter()
                .map(|l| item_key(&l.description, &l.size))
                .collect();
            move |_| checked.with(|c| keys.iter().filter(|k| c.contains(*k)).count())
        });
        let groups = CATEGORIES.iter().filter_map(|cat| {
            let items: Vec<_> = e.line_items.iter().filter(|l| l.category == *cat).cloned().collect();
            (!items.is_empty()).then(|| view! {
                <h3 class="cat-title">{cat.label()}</h3>
                {items.into_iter().map(|l| {
                    let key = item_key(&l.description, &l.size);
                    let k1 = key.clone();
                    let k2 = key.clone();
                    let is_done = Signal::derive(move || checked.with(|c| c.contains(&k1)));
                    view! {
                        <label class="pick" class:done=is_done>
                            <input type="checkbox" prop:checked=is_done on:change=move |_| toggle(k2.clone()) />
                            <span class="qty">{l.quantity}<small>{l.unit.clone()}</small></span>
                            <span class="what">
                                {l.size.clone().map(|s| view! { <span class="size">{s}" "</span> })}
                                {l.description.clone()}
                                {l.notes.clone().map(|n| view! { <span class="muted small">" ("{n}")"</span> })}
                            </span>
                        </label>
                    }
                }).collect_view()}
            })
        }).collect_view();
        view! {
            <div class="card">
                <div class="row">
                    <h2 style="margin:0">"Pick list"</h2>
                    <span class="spacer"></span>
                    <span class="muted small no-print">{move || format!("{} of {total} picked", done.get())}</span>
                    <button class="btn small no-print" on:click=move |_| { checked.set(HashSet::new()); persist(); }>"Reset"</button>
                </div>
                <div class="progress no-print"><div style=move || format!("width:{}%", (done.get() * 100).checked_div(total).unwrap_or(0))></div></div>
                {groups}
                <p class="muted small" style="margin-top:1rem">{e.disclaimer.clone()}</p>
            </div>
        }.into_any()
    }
}

// ---- Cut list ----

/// Identical boards (same stock and cuts) are shown once with a count.
fn grouped(c: &CutList) -> Vec<(usize, &estimator::cutopt::StockBoard)> {
    let mut out: Vec<(usize, &estimator::cutopt::StockBoard)> = Vec::new();
    for b in &c.boards {
        match out
            .iter_mut()
            .find(|(_, x)| x.stock_length == b.stock_length && x.cuts == b.cuts)
        {
            Some((n, _)) => *n += 1,
            None => out.push((1, b)),
        }
    }
    out
}

fn cut_summary(b: &estimator::cutopt::StockBoard) -> String {
    let mut counts: BTreeMap<(String, i64), usize> = BTreeMap::new();
    for c in &b.cuts {
        *counts
            .entry((c.label.clone(), (c.length * 16.0).round() as i64))
            .or_default() += 1;
    }
    counts
        .into_iter()
        .rev()
        .map(|((label, len16), n)| {
            let len = fmt_ft_in(len16 as f64 / 16.0);
            if n > 1 {
                format!("{n} x {label} {len}")
            } else {
                format!("{label} {len}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[component]
pub fn CutListView(est: Signal<EstimateState>) -> impl IntoView {
    move || {
        let est = est.get();
        let Some(e) = ready(&est) else {
            return view! { <div class="card"><p class="muted">"Fix the design to see the cut list."</p></div> }.into_any();
        };
        e.cut_lists.iter().map(|c| {
            let waste_pct = if c.total_stock > 0.0 { c.total_waste / c.total_stock * 100.0 } else { 0.0 };
            let boards = grouped(c).into_iter().map(|(n, b)| {
                let segs = b.cuts.iter().map(|cut| {
                    let w = cut.length / b.stock_length * 100.0;
                    let title = format!("{} {}", cut.label, fmt_ft_in(cut.length));
                    let text = if w > 14.0 { fmt_ft_in(cut.length) } else { String::new() };
                    view! { <div class="cut" style=format!("width:{w:.3}%") title=title>{text}</div> }
                }).collect_view();
                view! {
                    <div class="board">
                        <div class="meta">
                            <span><strong>{if n > 1 { format!("{n} x ") } else { String::new() }}{fmt_ft_in(b.stock_length)}" board"</strong></span>
                            <span>{format!("offcut {}", fmt_ft_in(b.waste))}</span>
                        </div>
                        <div class="bar">{segs}</div>
                        <div class="cut-labels">{cut_summary(b)}</div>
                    </div>
                }
            }).collect_view();
            view! {
                <div class="card">
                    <h2>{c.material.clone()}</h2>
                    <p class="muted small">
                        {format!("{} {}, {:.0} linear ft, {:.1}% offcut", c.boards.len(), if c.boards.len() == 1 { "board" } else { "boards" },
                            c.total_stock / 12.0, waste_pct)}
                        {(c.reusable_offcuts > 0).then(|| format!(", {} reusable offcuts", c.reusable_offcuts))}
                    </p>
                    {boards}
                </div>
            }
        }).collect_view().into_any()
    }
}

// ---- Notes ----

#[component]
pub fn NotesView(est: Signal<EstimateState>) -> impl IntoView {
    move || {
        let est = est.get();
        let Some(e) = ready(&est) else {
            return view! { <div class="card"><p class="muted">"Notes appear once the design is valid."</p></div> }.into_any();
        };
        view! {
            <div class="card">
                <h2>"Notes"</h2>
                {e.tips.iter().map(|t| view! { <p class="notice tip">{t.clone()}</p> }).collect_view()}
                {e.warnings.iter().map(|w| view! { <p class="notice warn">{w.clone()}</p> }).collect_view()}
                {e.sections.iter().map(|s| view! {
                    <h3>{s.name.clone()}</h3>
                    <ul class="small">{s.rationale.iter().map(|r| view! { <li>{r.clone()}</li> }).collect_view()}</ul>
                }).collect_view()}
                {e.stairs.iter().enumerate().map(|(i, s)| view! {
                    <h3>{format!("Stairs {}", i + 1)}</h3>
                    <ul class="small">{s.rationale.iter().map(|r| view! { <li>{r.clone()}</li> }).collect_view()}</ul>
                }).collect_view()}
                <p class="muted small">{e.disclaimer.clone()}</p>
            </div>
        }.into_any()
    }
}
