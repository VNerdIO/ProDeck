//! Structural framing for one section: joists, ledger, rims, blocking, beams, posts, footings,
//! and the connectors that tie them together.

use std::f64::consts::PI;

use crate::bom::{BeamDesign, FootingDesign, SectionDesign, SectionPlan};
use crate::cutopt::split_at_supports;
use crate::geometry::{EdgeInfo, Frame, Point};
use crate::materials::{decking_spec, Item, Stock};
use crate::model::{FootingType, JoistSpacing, LumberSize, Project, Section};
use crate::span_tables::{self, BeamSize};
use crate::{fmt_ft_in, EstimateError, Takeoff};

const EPS: f64 = 1e-6;
const PLY: f64 = 1.5;
/// Standard footing diameters (inches).
const FOOTING_DIAMETERS: [f64; 7] = [12.0, 16.0, 18.0, 20.0, 24.0, 30.0, 36.0];
/// Post base standoff plus pier top above grade.
const POST_BASE_ALLOWANCE: f64 = 3.0;

/// What later stages (decking, stairs, railing) need from the framing.
pub(crate) struct Framed {
    pub design: SectionDesign,
    /// Joist centerlines across the frame (`u` coordinate).
    pub joist_u: Vec<f64>,
    pub joist_size: LumberSize,
}

/// Spans reduce for live loads over the 40 psf the tables assume (bending-governed, conservative).
fn load_factor(live_psf: f64) -> f64 {
    if live_psf <= 40.0 {
        1.0
    } else {
        (50.0 / (live_psf + 10.0)).sqrt()
    }
}

fn joist_spacing(p: &Project, t: &mut Takeoff) -> JoistSpacing {
    let max = decking_spec(p.materials.decking).max_joist_spacing;
    let widest = JoistSpacing::ALL
        .iter()
        .copied()
        .filter(|s| s.inches() <= max + EPS)
        .max()
        .unwrap_or(JoistSpacing::Oc12);
    match p.materials.joist_spacing {
        Some(s) if s.inches() <= max + EPS => s,
        Some(s) => {
            t.warn(format!(
                "Joist spacing {}\" exceeds the decking's {}\" maximum; using {}\".",
                s.inches(),
                max,
                widest.inches()
            ));
            widest
        }
        None => widest,
    }
}

/// Joist positions (left face) across `cross`, with end joists flush to both sides.
fn joist_faces(cross: f64, spacing: f64) -> Vec<f64> {
    let last = cross - PLY;
    let mut v = Vec::new();
    let mut u = 0.0;
    while u < last - EPS {
        v.push(u);
        u += spacing;
    }
    v.push(last);
    v
}

fn pieces_for(len: f64, supports: &[f64], max: f64) -> Vec<f64> {
    split_at_supports(len, supports, max, max)
}

pub(crate) fn frame_section(
    idx: usize,
    s: &Section,
    frame: &Frame,
    edges: &[EdgeInfo],
    p: &Project,
    t: &mut Takeoff,
) -> Result<Framed, EstimateError> {
    let m = &p.materials;
    let spec = decking_spec(m.decking);
    let factor = load_factor(p.site.live_load_psf);
    let max_stock = p.options.max_stock_ft as f64 * 12.0;
    let spacing = joist_spacing(p, t);
    let sp = spacing.inches();
    let attached = s.ledger_edge.is_some();
    let run = frame.run;
    let cross = frame.cross;
    let mut why = Vec::new();

    if factor < 1.0 {
        t.warn(format!(
            "{}: {} psf live load exceeds the 40 psf span tables; spans reduced by {:.0}%. Have snow-load framing reviewed.",
            s.name,
            p.site.live_load_psf,
            (1.0 - factor) * 100.0
        ));
    }

    // Beam setback from the outer end so the rim hides it.
    let cantilever = if run >= 96.0 { 12.0 } else { 0.0 };
    let start = if attached { 0.0 } else { cantilever };
    let end = run - cantilever;
    let joist_len = run - 2.0 * PLY;

    // Fewest bays first (fewer beams and footings), then the smallest joist that spans.
    let mut chosen = None;
    'search: for bays in 1..=6usize {
        let bay = (end - start) / bays as f64;
        if bay > span_tables::max_tabulated_joist_span() + EPS {
            continue;
        }
        let interior: Vec<f64> = (1..bays).map(|k| start + k as f64 * bay - PLY).collect();
        let segs = pieces_for(joist_len, &interior, max_stock);
        if segs.iter().any(|l| *l > max_stock + EPS) {
            continue;
        }
        for size in LumberSize::JOISTS.into_iter().filter(|z| *z >= m.min_joist) {
            let Some(max) = span_tables::joist_max_span(m.framing, size, spacing) else {
                continue;
            };
            let max = max * factor;
            if bay <= max + EPS {
                chosen = Some((bays, bay, size, max, segs));
                break 'search;
            }
        }
    }
    let Some((bays, bay, joist_size, joist_max, joist_segs)) = chosen else {
        return Err(EstimateError::new(
            "span",
            format!(
                "{}: no prescriptive joist/beam layout found for a {} run.",
                s.name,
                fmt_ft_in(run)
            ),
            Some(idx),
        ));
    };
    if cantilever > bay / 4.0 + EPS {
        t.warn(format!(
            "{}: {} cantilever exceeds 1/4 of the {} bay.",
            s.name,
            fmt_ft_in(cantilever),
            fmt_ft_in(bay)
        ));
    }
    why.push(format!(
        "Joists {} @ {}\" OC, {} bay(s) of {} (max {} for {:?} #2).",
        joist_size.label(),
        sp,
        bays,
        fmt_ft_in(bay),
        fmt_ft_in(joist_max),
        m.framing
    ));
    if !attached || bays > 1 || cantilever > 0.0 {
        why.push(format!(
            "Joists cantilever {} past the outer beam.",
            fmt_ft_in(cantilever)
        ));
    }

    // Joists.
    let faces = joist_faces(cross, sp);
    let joist_count = faces.len();
    let joist_u: Vec<f64> = faces.iter().map(|u| u + PLY / 2.0).collect();
    let jl = joist_size;
    for len in &joist_segs {
        t.cut(Stock::Framing(jl), *len, "Joist", joist_count);
    }

    // Ledger and rims run across the joists; splice them on a joist.
    let joist_supports: Vec<f64> = joist_u.clone();
    if attached {
        for len in pieces_for(cross, &joist_supports, max_stock) {
            t.cut(Stock::Framing(jl), len, "Ledger", 1);
        }
    }
    let rims = if attached { 1 } else { 2 };
    for len in pieces_for(cross, &joist_supports, max_stock) {
        t.cut(Stock::Framing(jl), len, "Rim joist", rims);
    }

    // Mid-span blocking on bays over 8'.
    let block_rows = if bay > 96.0 { bays } else { 0 };
    for w in faces.windows(2) {
        let gap = w[1] - w[0] - PLY;
        if gap > 1.0 {
            t.cut(Stock::Framing(jl), gap, "Blocking", block_rows);
        }
    }
    if block_rows > 0 {
        why.push("Mid-span blocking on bays over 8'.".into());
    }

    // Beam lines (v positions) and the joist span each one carries.
    let mut beam_lines: Vec<(f64, f64)> = Vec::new();
    let first_k = if attached { 1 } else { 0 };
    for k in first_k..=bays {
        let v = start + k as f64 * bay;
        let interior = k > 0 && k < bays;
        beam_lines.push((v, if interior { 2.0 * bay } else { bay }));
    }

    // Post heights: top of decking -> top of joist -> bottom of beam.
    let top_of_joist = s.height - spec.thickness;

    let mut beams = Vec::new();
    let mut plan_posts = Vec::new();
    let mut plan_beams = Vec::new();
    let mut total_posts = 0usize;
    let mut post_len_out: Option<f64> = None;
    let mut footing_d = FOOTING_DIAMETERS[0];
    let footing_depth = p.site.frost_depth.max(12.0);

    for (v, carried) in &beam_lines {
        let best = span_tables::beam_sizes(m.framing)
            .into_iter()
            .filter_map(|b: BeamSize| {
                let max = span_tables::beam_max_span(m.framing, b, *carried)? * factor;
                if max < 36.0 {
                    return None;
                }
                let inset = (max / 4.0).min(12.0);
                let n = (((cross - 2.0 * inset) / max) - EPS).ceil().max(1.0) as usize + 1;
                Some((n, b, max, inset))
            })
            .min_by_key(|(n, b, _, _)| (*n, b.plies, b.size));
        let Some((posts, beam, max, inset)) = best else {
            return Err(EstimateError::new(
                "beam",
                format!(
                    "{}: no tabulated beam carries a {} joist span.",
                    s.name,
                    fmt_ft_in(*carried)
                ),
                Some(idx),
            ));
        };
        let post_spacing = (cross - 2.0 * inset) / (posts - 1) as f64;
        let post_u: Vec<f64> = (0..posts)
            .map(|k| inset + k as f64 * post_spacing)
            .collect();

        for len in pieces_for(cross, &post_u, max_stock) {
            t.cut(
                Stock::Framing(beam.size),
                len,
                format!("Beam ply ({})", beam.label()),
                beam.plies as usize,
            );
        }
        // Laminate plies: two screws every 16" per extra ply.
        t.count(
            Item::FramingScrews,
            (beam.plies - 1) as f64 * (cross / 16.0).ceil() * 2.0,
        );

        let bottom_of_beam = top_of_joist - jl.depth() - beam.size.depth();
        let post_len = bottom_of_beam - POST_BASE_ALLOWANCE;
        if post_len >= 12.0 {
            if post_len > 14.0 * 12.0 {
                t.warn(format!(
                    "{}: posts over 14' exceed DCA6 6x6 limits; needs engineering.",
                    s.name
                ));
            }
            t.cut(Stock::Post6x6, post_len, "Post", posts);
            t.count(Item::PostBase6x6, posts as f64);
            t.count(Item::PostCap { plies: beam.plies }, posts as f64);
            t.count(Item::ConnectorScrews, posts as f64 * 24.0);
            post_len_out = Some(post_len_out.map_or(post_len, |x: f64| x.max(post_len)));
        }

        // Footing size from tributary load on an interior post.
        let outer_end =
            (*v - (run - cantilever)).abs() < EPS || (!attached && v.abs() < cantilever + EPS);
        let trib_depth = carried / 2.0 + if outer_end { cantilever } else { 0.0 };
        let trib_sqft = post_spacing.max(inset * 2.0) * trib_depth / 144.0;
        let load = trib_sqft * (p.site.live_load_psf + 10.0);
        let area_sqft = load / p.site.soil_bearing_psf;
        let need_d = (4.0 * area_sqft / PI).sqrt() * 12.0;
        let d = FOOTING_DIAMETERS
            .iter()
            .copied()
            .find(|d| *d + EPS >= need_d)
            .unwrap_or(*FOOTING_DIAMETERS.last().unwrap());
        footing_d = footing_d.max(d);

        beams.push(BeamDesign {
            size: beam.label(),
            plies: beam.plies,
            lumber: beam.size,
            carries_joist_span: *carried,
            max_span: max,
            posts,
            post_spacing,
            footing_diameter: d,
        });
        why.push(format!(
            "Beam {} at {} from the {}: posts every {} (max {} carrying {} joists).",
            beam.label(),
            fmt_ft_in(*v),
            if attached { "house" } else { "back" },
            fmt_ft_in(post_spacing),
            fmt_ft_in(max),
            fmt_ft_in(*carried)
        ));
        plan_beams.push(frame.seg(0.0, *v, cross, *v));
        plan_posts.extend(post_u.iter().map(|u| frame.to_plan(*u, *v)));
        total_posts += posts;
    }

    // Footings (one per post).
    let mut footing_kind = m.footing;
    if footing_kind == FootingType::PrecastBlocks && (attached || s.height > 30.0) {
        t.warn(format!(
            "{}: precast deck blocks are only suitable for low freestanding decks; using poured footings.",
            s.name
        ));
        footing_kind = FootingType::PouredBags;
    }
    match footing_kind {
        FootingType::PrecastBlocks => t.count(Item::DeckBlock, total_posts as f64),
        FootingType::PouredBags => {
            for b in &beams {
                let r_ft = b.footing_diameter / 24.0;
                let cu_ft = PI * r_ft * r_ft * (footing_depth + 2.0) / 12.0;
                t.count(Item::ConcreteBag80, cu_ft * b.posts as f64);
                if b.footing_diameter <= 24.0 {
                    let tubes = ((footing_depth + 2.0) / 48.0).ceil();
                    t.count(
                        Item::FormTube {
                            diameter_in: b.footing_diameter as u32,
                        },
                        tubes * b.posts as f64,
                    );
                }
            }
            why.push(format!(
                "{} footings, up to {}\" dia x {} deep ({} psf soil, {} frost depth).",
                total_posts,
                footing_d,
                fmt_ft_in(footing_depth),
                p.site.soil_bearing_psf,
                fmt_ft_in(p.site.frost_depth)
            ));
        }
    }
    if post_len_out.is_none() {
        why.push("Low deck: beams bear directly on footings/blocks (no posts).".into());
    }

    // Connectors and fasteners.
    if attached {
        t.count(Item::JoistHanger(jl), joist_count as f64);
        t.count(Item::ConnectorScrews, joist_count as f64 * 10.0);
        let oc = span_tables::ledger_fastener_spacing(m.ledger_fastener, bay).unwrap_or(10.0);
        let n = (cross / oc).ceil() + 1.0;
        t.count(
            Item::LedgerFastener {
                lags: m.ledger_fastener == crate::model::LedgerFastener::HalfInchLags,
            },
            n,
        );
        t.count(Item::LedgerFlashing, cross);
        why.push(format!(
            "Ledger fastened every {oc}\" OC, staggered; flashed."
        ));
    }
    let ties = joist_count * beam_lines.len();
    t.count(Item::HurricaneTie, ties as f64);
    t.count(Item::ConnectorScrews, ties as f64 * 6.0);
    // 3 screws per joist end into a rim, blocking 4 each, rim corners 3 each.
    let rim_ends = if attached { 1.0 } else { 2.0 };
    t.count(Item::FramingScrews, joist_count as f64 * 3.0 * rim_ends);
    t.count(
        Item::FramingScrews,
        block_rows as f64 * (joist_count - 1) as f64 * 4.0,
    );
    t.count(Item::FramingScrews, 2.0 * rim_ends * 3.0);
    if m.joist_tape {
        let inches = joist_count as f64 * joist_len + rim_ends * cross;
        t.count(Item::JoistTape, inches);
    }

    // Fascia over exposed rims and end joists.
    if m.fascia {
        let stock_max = Stock::Fascia
            .stock_lengths_ft(m, p.options.max_stock_ft)
            .last()
            .copied()
            .unwrap_or(12) as f64
            * 12.0;
        for e in edges {
            for span in &e.exposed {
                let supports: Vec<f64> = (1..)
                    .map(|k| k as f64 * 16.0)
                    .take_while(|x| *x < span.len())
                    .collect();
                for len in pieces_for(span.len(), &supports, stock_max) {
                    t.cut(Stock::Fascia, len, "Fascia", 1);
                }
                t.count(Item::FasciaScrews, (span.len() / 16.0).ceil() * 2.0);
            }
        }
    }

    let outline = vec![
        Point { x: s.x, y: s.y },
        Point {
            x: s.x + s.width,
            y: s.y,
        },
        Point {
            x: s.x + s.width,
            y: s.y + s.depth,
        },
        Point {
            x: s.x,
            y: s.y + s.depth,
        },
    ];
    let plan = SectionPlan {
        outline,
        ledger: attached.then(|| frame.seg(0.0, 0.0, cross, 0.0)),
        joists: joist_u
            .iter()
            .map(|u| frame.seg(*u, 0.0, *u, run))
            .collect(),
        beams: plan_beams,
        posts: plan_posts,
    };

    Ok(Framed {
        design: SectionDesign {
            name: s.name.clone(),
            joist_size: jl,
            joist_spacing: sp,
            joist_count,
            bays,
            bay_span: bay,
            joist_max_span: joist_max,
            cantilever,
            beams,
            post_length: post_len_out,
            footing: FootingDesign {
                kind: footing_kind,
                count: total_posts,
                diameter: footing_d,
                depth: footing_depth,
            },
            plan,
            rationale: why,
        },
        joist_u,
        joist_size: jl,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joist_faces_cover_width() {
        let f = joist_faces(192.0, 16.0);
        assert_eq!(f.first(), Some(&0.0));
        assert_eq!(f.last(), Some(&190.5));
        assert_eq!(f.len(), 13);
        assert!(f.windows(2).all(|w| w[1] - w[0] <= 16.0 + EPS));
    }

    #[test]
    fn snow_load_reduces_spans() {
        assert_eq!(load_factor(40.0), 1.0);
        assert!(load_factor(60.0) < 1.0);
    }
}
