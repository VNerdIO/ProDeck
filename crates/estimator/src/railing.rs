//! Guard railing on open deck edges and stairs.

use crate::bom::StairDesign;
use crate::framing::Framed;
use crate::geometry::{subtract_stairs, Layout, Span, GUARD_HEIGHT};
use crate::materials::{decking_spec, Item, Stock};
use crate::model::Edge;
use crate::model::{LumberSize, Project, RailingType};
use crate::Takeoff;

use std::collections::BTreeSet;

/// Guard height above the deck surface.
const GUARD: f64 = 36.0;
/// Maximum post spacing on center.
const MAX_POST_SPACING: f64 = 72.0;
const POST_WIDTH: f64 = 3.5;
/// Baluster on-center spacing keeping gaps under 4".
const WOOD_BALUSTER_OC: f64 = 5.0;
const METAL_BALUSTER_OC: f64 = 4.25;

struct Run {
    /// Horizontal length.
    length: f64,
    /// Slope factor (1.0 for level runs).
    slope: f64,
    stair: bool,
    /// Plan endpoints in 1/16", for sharing corner posts between level runs.
    ends: Option<[(i64, i64); 2]>,
}

fn key(x: f64, y: f64) -> (i64, i64) {
    ((x * 16.0).round() as i64, (y * 16.0).round() as i64)
}

pub(crate) fn build_railing(
    p: &Project,
    layout: &Layout,
    framed: &[Framed],
    stairs: &[StairDesign],
    t: &mut Takeoff,
) {
    let m = &p.materials;
    let spec = decking_spec(m.decking);
    let mut runs = Vec::new();
    let mut max_rim_depth: f64 = 0.0;

    for (i, s) in p.sections.iter().enumerate() {
        let wants = s.railing.unwrap_or(s.height > GUARD_HEIGHT);
        if !wants {
            continue;
        }
        if s.railing == Some(false) && s.height > GUARD_HEIGHT {
            t.warn(format!(
                "{}: is over 30\" high; code requires a guard on open sides.",
                s.name
            ));
        }
        max_rim_depth = max_rim_depth.max(framed[i].joist_size.depth());
        for e in &layout.edges[i] {
            let open: Vec<Span> = subtract_stairs(&e.drop_off, i, e.edge, &p.stairs);
            for span in open {
                if span.len() > 1.0 {
                    let ends = match e.edge {
                        Edge::North => [key(s.x + span.a, s.y), key(s.x + span.b, s.y)],
                        Edge::South => [
                            key(s.x + span.a, s.y + s.depth),
                            key(s.x + span.b, s.y + s.depth),
                        ],
                        Edge::West => [key(s.x, s.y + span.a), key(s.x, s.y + span.b)],
                        Edge::East => [
                            key(s.x + s.width, s.y + span.a),
                            key(s.x + s.width, s.y + span.b),
                        ],
                    };
                    runs.push(Run {
                        length: span.len(),
                        slope: 1.0,
                        stair: false,
                        ends: Some(ends),
                    });
                }
            }
        }
    }
    for st in stairs {
        if st.handrails {
            let slope = (st.rise.powi(2) + st.total_run.powi(2)).sqrt() / st.total_run.max(1.0);
            for _ in 0..2 {
                runs.push(Run {
                    length: st.total_run,
                    slope,
                    stair: true,
                    ends: None,
                });
            }
        }
    }
    if runs.is_empty() {
        return;
    }
    if max_rim_depth == 0.0 {
        max_rim_depth = LumberSize::TwoBy8.depth();
    }

    // Posts bolt to the rim and rise to the guard height.
    let post_len = GUARD + spec.thickness + max_rim_depth + 1.5;
    let mut posts = 0usize;
    for r in &runs {
        let spans = (r.length / MAX_POST_SPACING).ceil().max(1.0) as usize;
        // Intermediate posts; end posts are counted below so corners are shared.
        posts += spans - 1;
        let clear = (r.length / spans as f64 - POST_WIDTH) * r.slope;
        let along = r.length / spans as f64 * r.slope;
        match m.railing {
            RailingType::WoodBalusters | RailingType::MetalBalusters => {
                t.cut(
                    Stock::Framing(LumberSize::TwoBy4),
                    clear,
                    "Rail (top/bottom)",
                    2 * spans,
                );
                t.cut(Stock::Framing(LumberSize::TwoBy6), along, "Rail cap", spans);
                let oc = if m.railing == RailingType::WoodBalusters {
                    WOOD_BALUSTER_OC
                } else {
                    METAL_BALUSTER_OC
                };
                let per_span = (clear / r.slope / oc).ceil() as usize;
                let n = (per_span * spans) as f64;
                let len = if r.stair { 42 } else { 36 };
                if m.railing == RailingType::WoodBalusters {
                    t.count(Item::Baluster2x2 { length_in: len }, n);
                    t.count(Item::DeckScrews { composite: false }, n * 4.0);
                } else {
                    let len = if r.stair { 32 } else { 26 };
                    t.count(Item::MetalBaluster { length_in: len }, n);
                }
                t.count(Item::DeckScrews { composite: false }, spans as f64 * 8.0);
            }
            RailingType::PanelKits => {
                let kit_ft = if r.length / spans as f64 <= 72.0 {
                    6
                } else {
                    8
                };
                t.count(
                    Item::RailingPanelKit {
                        length_ft: kit_ft,
                        stair: r.stair,
                    },
                    spans as f64,
                );
            }
        }
    }
    let corners: BTreeSet<(i64, i64)> = runs.iter().filter_map(|r| r.ends).flatten().collect();
    posts += corners.len() + 2 * runs.iter().filter(|r| r.ends.is_none()).count();

    t.cut(Stock::RailPost4x4, post_len, "Railing post", posts);
    t.count(Item::CarriageBolt, posts as f64 * 2.0);
    if m.railing == RailingType::PanelKits {
        t.count(Item::PostSleeve, posts as f64);
        t.count(Item::PostSleeveCap, posts as f64);
    }
}
