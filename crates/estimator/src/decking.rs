//! Decking boards and their fasteners for one section.

use crate::cutopt::split_at_supports;
use crate::framing::Framed;
use crate::geometry::{EdgeInfo, Frame};
use crate::materials::{decking_spec, Item, Stock};
use crate::model::{Fastening, Project, Section};
use crate::{fmt_ft_in, Takeoff};

/// Board overhang past an exposed rim or fascia.
const OVERHANG: f64 = 0.75;

pub(crate) fn deck_section(
    _idx: usize,
    s: &Section,
    frame: &Frame,
    edges: &[EdgeInfo],
    framed: &Framed,
    p: &Project,
    t: &mut Takeoff,
) {
    let m = &p.materials;
    let spec = decking_spec(m.decking);
    let pitch = spec.width + spec.gap;
    let max_stock = Stock::Decking
        .stock_lengths_ft(m, p.options.max_stock_ft)
        .last()
        .copied()
        .unwrap_or(16) as f64
        * 12.0;

    // Edges parallel to the boards are the ledger side and the outer end; edges at the board
    // ends are the sides.
    let is_row_parallel = |e: &EdgeInfo| e.edge.runs_along() != s.joists_along;
    let exposed = |e: &EdgeInfo| !e.exposed.is_empty();
    let parallel_exposed = edges
        .iter()
        .filter(|e| is_row_parallel(e) && exposed(e))
        .count() as f64;
    let side_exposed = edges
        .iter()
        .filter(|e| !is_row_parallel(e) && exposed(e))
        .count() as f64;

    let mut coverage = frame.run + parallel_exposed * OVERHANG;
    let mut row_len = frame.cross + side_exposed * OVERHANG;
    // Offset from the start of a row to joist u = 0 (approximate: assumes the overhang and any
    // border are on the u = 0 side, which only shifts splice points by under one board width).
    let mut row_offset = if side_exposed > 0.0 { OVERHANG } else { 0.0 };

    if m.picture_frame {
        for e in edges {
            for span in &e.exposed {
                // Mitered corners: add a board width for each end.
                let len = span.len() + 2.0 * (spec.width + OVERHANG);
                let supports: Vec<f64> = framed.joist_u.clone();
                for piece in split_at_supports(len, &supports, max_stock, max_stock) {
                    t.cut(Stock::Decking, piece, "Picture-frame border", 1);
                }
                t.count(
                    Item::DeckScrews {
                        composite: spec.composite,
                    },
                    (len / 16.0).ceil() * 2.0,
                );
            }
        }
        coverage -= parallel_exposed * pitch;
        row_len -= side_exposed * pitch;
        if side_exposed > 0.0 {
            row_offset -= pitch;
        }
        t.warn(format!(
            "{}: picture-frame borders need extra blocking under the border boards; add it on site.",
            s.name
        ));
    }

    // Rows just over a stock length force the next size up; flag it so the builder can tighten
    // the framing by an inch or two instead.
    let sold: Vec<f64> = Stock::Decking
        .stock_lengths_ft(m, p.options.max_stock_ft)
        .iter()
        .map(|f| *f as f64 * 12.0)
        .collect();
    if let Some(under) = sold
        .iter()
        .copied()
        .find(|l| row_len > *l && row_len - *l <= 3.0)
    {
        let next = sold.iter().copied().find(|l| *l > row_len);
        if let Some(next) = next {
            t.tip(format!(
                "{}: deck rows are {} with overhang, just over {}. Framing {} narrower lets you buy {}' boards instead of {}'.",
                s.name,
                fmt_ft_in(row_len),
                fmt_ft_in(under),
                fmt_ft_in(row_len - under),
                under / 12.0,
                next / 12.0
            ));
        }
    }

    let rows = (coverage / pitch).ceil().max(1.0) as usize;
    let supports: Vec<f64> = framed.joist_u.iter().map(|u| u + row_offset).collect();
    let spacing = framed.design.joist_spacing;
    for row in 0..rows {
        // Stagger butt joints across three rows so seams don't line up.
        let shift = (row % 3) as f64 * 2.0 * spacing;
        let first_max = (max_stock - shift).max(spacing);
        for piece in split_at_supports(row_len, &supports, max_stock, first_max) {
            t.cut(Stock::Decking, piece, "Deck board", 1);
        }
    }
    if row_len > max_stock {
        t.warn(format!(
            "{}: deck rows are {:.0}' long, so boards butt on joists; double the joist (or add a sleeper) at each seam.",
            s.name,
            row_len / 12.0
        ));
    }

    let crossings = rows as f64 * framed.design.joist_count as f64;
    match m.fastening {
        Fastening::FaceScrews => t.count(
            Item::DeckScrews {
                composite: spec.composite,
            },
            crossings * 2.0,
        ),
        Fastening::HiddenClips => {
            t.count(Item::HiddenClips, crossings);
            if !spec.composite {
                t.warn("Hidden clips require grooved-edge boards; confirm the wood decking is grooved.");
            }
        }
    }
}
