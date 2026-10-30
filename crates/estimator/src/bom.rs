//! Bill of materials: run cut optimization per material, package loose items into store
//! units, and assemble the final `Estimate`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cutopt::{self, StockBoard};
use crate::geometry::{Point, Segment};
use crate::materials::{Category, Stock};
use crate::model::{Axis, Edge, FootingType, LumberSize, PostSize, Project};
use crate::{EstimateError, Takeoff};

pub const DISCLAIMER: &str =
    "Material estimate only. Framing follows AWC DCA6 prescriptive tables; \
local code, permit drawings, and the building inspector have final say.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Estimate {
    /// Store pick list, grouped and sorted by department.
    pub line_items: Vec<LineItem>,
    pub cut_lists: Vec<CutList>,
    pub sections: Vec<SectionDesign>,
    pub stairs: Vec<StairDesign>,
    pub warnings: Vec<String>,
    /// Money-saving suggestions, e.g. a small dimension change that avoids longer boards.
    pub tips: Vec<String>,
    pub summary: Summary,
    pub disclaimer: String,
    pub engine_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LineItem {
    pub category: Category,
    pub description: String,
    /// Stock length for lumber (e.g. "16'"), absent for counted items.
    pub size: Option<String>,
    /// Stock length in inches, for sorting/rendering.
    pub length_in: Option<f64>,
    pub quantity: u32,
    pub unit: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CutList {
    pub material: String,
    pub category: Category,
    pub boards: Vec<StockBoard>,
    /// Inches of stock bought.
    pub total_stock: f64,
    /// Inches left over after all cuts.
    pub total_waste: f64,
    /// Offcuts long enough to be worth keeping.
    pub reusable_offcuts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Summary {
    pub deck_area_sqft: f64,
    pub lumber_pieces: u32,
    pub linear_feet: f64,
    pub waste_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SectionDesign {
    pub name: String,
    /// Axis the decking boards run along (across the joists).
    pub decking_along: Axis,
    pub joist_size: LumberSize,
    pub joist_spacing: f64,
    pub joist_count: usize,
    pub bays: usize,
    pub bay_span: f64,
    pub joist_max_span: f64,
    pub cantilever: f64,
    pub beams: Vec<BeamDesign>,
    /// Longest post, inches; `None` when beams bear directly on footings.
    pub post_length: Option<f64>,
    pub footing: FootingDesign,
    pub plan: SectionPlan,
    /// Plain-language reasons for each sizing decision.
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BeamDesign {
    /// e.g. "2-2x10".
    pub size: String,
    pub plies: u8,
    pub lumber: LumberSize,
    pub carries_joist_span: f64,
    pub max_span: f64,
    pub posts: usize,
    pub post_spacing: f64,
    /// `None` when the beam bears directly on footings.
    pub post_size: Option<PostSize>,
    pub footing_diameter: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FootingDesign {
    pub kind: FootingType,
    pub count: usize,
    pub diameter: f64,
    pub depth: f64,
}

/// Plan-view drawing primitives (inches, plan coordinates) for the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SectionPlan {
    pub outline: Vec<Point>,
    pub ledger: Option<Segment>,
    pub joists: Vec<Segment>,
    pub beams: Vec<Segment>,
    pub posts: Vec<Point>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StairDesign {
    pub section: usize,
    pub edge: Edge,
    pub width: f64,
    pub rise: f64,
    pub risers: usize,
    pub riser_height: f64,
    pub treads: usize,
    pub unit_run: f64,
    pub total_run: f64,
    pub stringers: usize,
    pub stringer_length: f64,
    pub handrails: bool,
    pub rationale: Vec<String>,
}

fn overage_pct(stock: Stock, p: &Project) -> f64 {
    match stock.category() {
        Category::Decking => p.options.decking_overage_pct,
        _ => p.options.framing_overage_pct,
    }
}

pub(crate) fn finish(
    p: &Project,
    t: Takeoff,
    sections: Vec<SectionDesign>,
    stairs: Vec<StairDesign>,
) -> Result<Estimate, EstimateError> {
    let m = &p.materials;
    let mut line_items = Vec::new();
    let mut cut_lists = Vec::new();
    let mut total_stock = 0.0;
    let mut total_waste = 0.0;

    for (stock, pieces) in &t.pieces {
        if pieces.is_empty() {
            continue;
        }
        let lengths: Vec<f64> = stock
            .stock_lengths_ft(m, p.options.max_stock_ft)
            .iter()
            .map(|f| *f as f64 * 12.0)
            .collect();
        let boards = cutopt::optimize(pieces, &lengths, p.options.kerf)
            .map_err(|e| EstimateError::new("cut", e.to_string(), None))?;
        let description = stock.description(m);

        let mut by_len: BTreeMap<u32, u32> = BTreeMap::new();
        for b in &boards {
            *by_len
                .entry((b.stock_length / 12.0).round() as u32)
                .or_default() += 1;
        }
        let total: u32 = by_len.values().sum();
        let extra = (total as f64 * overage_pct(*stock, p) / 100.0 - 1e-9)
            .ceil()
            .max(0.0) as u32;
        // Spares go on the length bought most often.
        let common = by_len
            .iter()
            .max_by_key(|(len, n)| (**n, **len))
            .map(|(l, _)| *l);
        if let (Some(c), true) = (common, extra > 0) {
            *by_len.get_mut(&c).expect("present") += extra;
        }
        for (ft, qty) in by_len.iter().rev() {
            line_items.push(LineItem {
                category: stock.category(),
                description: description.clone(),
                size: Some(format!("{ft}'")),
                length_in: Some(*ft as f64 * 12.0),
                quantity: *qty,
                unit: "pc".into(),
                notes: (extra > 0 && Some(*ft) == common)
                    .then(|| format!("includes {extra} spare")),
            });
        }

        let stock_sum: f64 = boards.iter().map(|b| b.stock_length).sum();
        let waste_sum: f64 = boards.iter().map(|b| b.waste).sum();
        total_stock += stock_sum;
        total_waste += waste_sum;
        cut_lists.push(CutList {
            material: description,
            category: stock.category(),
            reusable_offcuts: boards
                .iter()
                .filter(|b| b.waste >= p.options.min_reusable_offcut)
                .count(),
            total_stock: stock_sum,
            total_waste: waste_sum,
            boards,
        });
    }

    for (item, raw) in &t.items {
        let pack = item.packaging();
        let qty = (raw / pack.per - 1e-9).ceil().max(1.0) as u32;
        line_items.push(LineItem {
            category: item.category(),
            description: item.description(),
            size: None,
            length_in: None,
            quantity: qty,
            unit: pack.unit.into(),
            notes: None,
        });
    }

    line_items.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.length_in.is_none().cmp(&b.length_in.is_none()))
            .then_with(|| a.description.cmp(&b.description))
            .then_with(|| {
                b.length_in
                    .unwrap_or(0.0)
                    .total_cmp(&a.length_in.unwrap_or(0.0))
            })
    });

    let summary = Summary {
        deck_area_sqft: round2(p.sections.iter().map(|s| s.width * s.depth).sum::<f64>() / 144.0),
        lumber_pieces: line_items
            .iter()
            .filter(|l| l.length_in.is_some())
            .map(|l| l.quantity)
            .sum(),
        linear_feet: round2(total_stock / 12.0),
        waste_pct: if total_stock > 0.0 {
            round2(total_waste / total_stock * 100.0)
        } else {
            0.0
        },
    };

    let mut warnings = t.warnings.clone();
    warnings.dedup();

    Ok(Estimate {
        line_items,
        cut_lists,
        sections,
        stairs,
        warnings,
        tips: t.tips,
        summary,
        disclaimer: DISCLAIMER.into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
    })
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}
