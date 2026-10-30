//! ProDeck estimator: turns a deck description into a store pick list and cut list.
//!
//! Pure and deterministic: no I/O, no randomness. The pipeline is
//! geometry -> framing -> decking -> stairs -> railing -> cut optimization -> BOM.
//!
//! Spans follow the AWC DCA6 prescriptive tables (`data/span_tables.toml`). Output is an
//! estimate; local code and the building inspector have final say.

pub mod bom;
pub mod cutopt;
mod decking;
mod framing;
pub mod geometry;
pub mod materials;
pub mod model;
mod railing;
pub mod span_tables;
mod stairs;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use bom::{CutList, Estimate, LineItem, SectionDesign, StairDesign};
use cutopt::Piece;
use materials::{Item, Stock};
pub use model::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EstimateError {
    /// Machine-readable error code.
    pub code: String,
    pub message: String,
    /// Index of the offending section/stair, when applicable.
    pub section: Option<usize>,
}

impl EstimateError {
    pub(crate) fn new(
        code: &str,
        message: impl Into<String>,
        section: Option<usize>,
    ) -> EstimateError {
        EstimateError {
            code: code.into(),
            message: message.into(),
            section,
        }
    }
}

impl std::fmt::Display for EstimateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EstimateError {}

/// Everything collected while walking the structure, before cut optimization and packaging.
#[derive(Default)]
pub(crate) struct Takeoff {
    pub pieces: BTreeMap<Stock, Vec<Piece>>,
    /// Raw counts in the item's base unit (see `Item::packaging`).
    pub items: BTreeMap<Item, f64>,
    pub warnings: Vec<String>,
    pub tips: Vec<String>,
}

impl Takeoff {
    pub fn cut(&mut self, stock: Stock, length: f64, label: impl Into<String>, qty: usize) {
        let label = label.into();
        let v = self.pieces.entry(stock).or_default();
        for _ in 0..qty {
            v.push(Piece {
                length: round_len(length),
                label: label.clone(),
            });
        }
    }

    pub fn count(&mut self, item: Item, n: f64) {
        if n > 0.0 {
            *self.items.entry(item).or_default() += n;
        }
    }

    pub fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    /// A money-saving suggestion (not a problem with the design).
    pub fn tip(&mut self, msg: impl Into<String>) {
        self.tips.push(msg.into());
    }
}

/// Round to 1/16" so cut lists read cleanly and float noise doesn't leak into the output.
pub(crate) fn round_len(x: f64) -> f64 {
    (x * 16.0).round() / 16.0
}

/// Format inches as feet-inches, e.g. 157.5 -> 13' 1-1/2".
pub fn fmt_ft_in(inches: f64) -> String {
    let sixteenths = (inches * 16.0).round() as i64;
    let ft = sixteenths / (12 * 16);
    let rem = sixteenths % (12 * 16);
    let whole = rem / 16;
    let frac = rem % 16;
    let frac_s = if frac == 0 {
        String::new()
    } else {
        let mut n = frac;
        let mut d = 16;
        while n % 2 == 0 {
            n /= 2;
            d /= 2;
        }
        format!("-{n}/{d}")
    };
    if ft == 0 {
        format!("{whole}{frac_s}\"")
    } else {
        format!("{ft}' {whole}{frac_s}\"")
    }
}

/// Run the full estimate.
pub fn estimate(project: &Project) -> Result<Estimate, EstimateError> {
    validate(project)?;
    let layout = geometry::layout(&project.sections);
    let mut t = Takeoff::default();

    let mut designs = Vec::new();
    for (i, s) in project.sections.iter().enumerate() {
        let f = framing::frame_section(i, s, &layout.frames[i], &layout.edges[i], project, &mut t)?;
        decking::deck_section(
            i,
            s,
            &layout.frames[i],
            &layout.edges[i],
            &f,
            project,
            &mut t,
        );
        designs.push(f);
    }
    let stair_designs = project
        .stairs
        .iter()
        .enumerate()
        .map(|(i, st)| stairs::build_stair(i, st, project, &designs, &mut t))
        .collect::<Result<Vec<_>, _>>()?;
    railing::build_railing(project, &layout, &designs, &stair_designs, &mut t);

    bom::finish(
        project,
        t,
        designs.into_iter().map(|f| f.design).collect(),
        stair_designs,
    )
}

fn validate(p: &Project) -> Result<(), EstimateError> {
    if p.sections.is_empty() {
        return Err(EstimateError::new(
            "no_sections",
            "Add at least one deck section.",
            None,
        ));
    }
    for (i, s) in p.sections.iter().enumerate() {
        let name = &s.name;
        if ![s.x, s.y, s.width, s.depth, s.height]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(EstimateError::new(
                "not_a_number",
                format!("{name}: dimensions must be numbers."),
                Some(i),
            ));
        }
        if s.width < 24.0 || s.depth < 24.0 {
            return Err(EstimateError::new(
                "too_small",
                format!("{name}: width and depth must be at least 2'."),
                Some(i),
            ));
        }
        if s.width > 1200.0 || s.depth > 1200.0 {
            return Err(EstimateError::new(
                "too_large",
                format!("{name}: sections are limited to 100' per side."),
                Some(i),
            ));
        }
        if s.height < 6.0 {
            return Err(EstimateError::new(
                "too_low",
                format!("{name}: height must be at least 6\" for framing clearance."),
                Some(i),
            ));
        }
        if s.height > 14.0 * 12.0 + 24.0 {
            return Err(EstimateError::new(
                "too_high",
                format!("{name}: decks over ~16' high are outside prescriptive (DCA6) limits; get an engineered design."),
                Some(i),
            ));
        }
        if let Some(e) = s.ledger_edge {
            if e.runs_along() == s.joists_along {
                return Err(EstimateError::new(
                    "ledger_parallel_to_joists",
                    format!(
                        "{name}: joists hang from the house ledger, so decking on an attached section runs parallel to the house ({}). To run boards toward the house, make the section freestanding (a beam and posts along the house).",
                        e.runs_along().label()
                    ),
                    Some(i),
                ));
            }
        }
        for (j, o) in p.sections.iter().enumerate().skip(i + 1) {
            if geometry::overlaps(s, o) {
                return Err(EstimateError::new(
                    "overlap",
                    format!("{name} overlaps {}.", o.name),
                    Some(j),
                ));
            }
        }
    }
    for (i, st) in p.stairs.iter().enumerate() {
        let Some(s) = p.sections.get(st.section) else {
            return Err(EstimateError::new(
                "stair_section",
                format!("Stair {}: unknown section.", i + 1),
                None,
            ));
        };
        if s.ledger_edge == Some(st.edge) {
            return Err(EstimateError::new(
                "stair_on_ledger",
                format!("Stair {}: can't leave from the house side.", i + 1),
                Some(st.section),
            ));
        }
        let len = geometry::edge_length(s, st.edge);
        if st.width < 36.0 {
            return Err(EstimateError::new(
                "stair_width",
                format!("Stair {}: minimum width is 36\".", i + 1),
                Some(st.section),
            ));
        }
        if st.offset < 0.0 || st.offset + st.width > len + 1e-6 {
            return Err(EstimateError::new(
                "stair_fit",
                format!(
                    "Stair {}: doesn't fit on the {:?} edge of {}.",
                    i + 1,
                    st.edge,
                    s.name
                ),
                Some(st.section),
            ));
        }
    }
    let o = &p.options;
    if !(0.0..=50.0).contains(&o.decking_overage_pct)
        || !(0.0..=50.0).contains(&o.framing_overage_pct)
    {
        return Err(EstimateError::new(
            "overage",
            "Overage must be between 0% and 50%.",
            None,
        ));
    }
    if !(8..=24).contains(&o.max_stock_ft) {
        return Err(EstimateError::new(
            "max_stock",
            "Longest stock length must be 8'-24'.",
            None,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_feet_inches() {
        assert_eq!(fmt_ft_in(157.5), "13' 1-1/2\"");
        assert_eq!(fmt_ft_in(192.0), "16' 0\"");
        assert_eq!(fmt_ft_in(5.25), "5-1/4\"");
    }
}
