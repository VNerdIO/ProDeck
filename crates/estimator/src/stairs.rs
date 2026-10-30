//! Exterior stairs from a section edge down to grade.

use std::f64::consts::PI;

use crate::bom::StairDesign;
use crate::framing::Framed;
use crate::materials::{decking_spec, Item, Stock};
use crate::model::{LumberSize, Project, Stair};
use crate::{fmt_ft_in, EstimateError, Takeoff};

/// IRC R311.7.5 maximum riser height.
pub const MAX_RISER: f64 = 7.75;
/// Unit run: two 5-1/2" boards with a small nosing.
pub const UNIT_RUN: f64 = 10.5;
/// Extra stringer length for the top bearing and bottom cut.
const STRINGER_EXTRA: f64 = 12.0;
/// Concrete landing pad at the bottom: depth x thickness.
const PAD_DEPTH: f64 = 36.0;
const PAD_THICKNESS: f64 = 4.0;

pub(crate) fn build_stair(
    idx: usize,
    st: &Stair,
    p: &Project,
    framed: &[Framed],
    t: &mut Takeoff,
) -> Result<StairDesign, EstimateError> {
    let m = &p.materials;
    let spec = decking_spec(m.decking);
    let section = &p.sections[st.section];
    let rise = st.rise.unwrap_or(section.height);
    let risers = (rise / MAX_RISER).ceil().max(1.0) as usize;
    let riser_height = rise / risers as f64;
    let treads = risers - 1;
    let total_run = treads as f64 * UNIT_RUN;
    let stringer_length = (rise * rise + total_run * total_run).sqrt() + STRINGER_EXTRA;
    let max_stock = p.options.max_stock_ft as f64 * 12.0;
    if stringer_length > max_stock {
        return Err(EstimateError::new(
            "stair_too_long",
            format!(
                "Stair {}: stringers would be {} long; add a landing (split into two stairs).",
                idx + 1,
                fmt_ft_in(stringer_length)
            ),
            Some(st.section),
        ));
    }
    let stringers = (st.width / spec.stair_stringer_spacing).ceil() as usize + 1;
    t.cut(
        Stock::Framing(LumberSize::TwoBy12),
        stringer_length,
        "Stair stringer",
        stringers,
    );
    t.count(Item::StringerConnector, stringers as f64);
    t.count(Item::ConnectorScrews, stringers as f64 * 10.0);

    // Two boards per tread, overhanging the outer stringers.
    let tread_len = st.width + 2.0 * 0.75;
    t.cut(Stock::Decking, tread_len, "Stair tread", treads * 2);
    t.count(
        Item::DeckScrews {
            composite: spec.composite,
        },
        (treads * 2 * stringers * 2) as f64,
    );

    let riser_stock = if spec.composite {
        Stock::Fascia
    } else {
        Stock::Riser1x8
    };
    t.cut(riser_stock, st.width, "Stair riser", risers);
    if spec.composite {
        t.count(Item::FasciaScrews, (risers * stringers * 2) as f64);
    } else {
        t.count(
            Item::DeckScrews { composite: false },
            (risers * stringers * 2) as f64,
        );
    }

    let pad_cu_ft = PAD_DEPTH * (st.width + 12.0) * PAD_THICKNESS / 1728.0;
    t.count(Item::ConcreteBag80, pad_cu_ft);

    let joist = framed
        .get(st.section)
        .map(|f| f.joist_size)
        .unwrap_or(LumberSize::TwoBy8);
    let angle = (rise / total_run.max(1.0)).atan() * 180.0 / PI;
    Ok(StairDesign {
        section: st.section,
        edge: st.edge,
        width: st.width,
        rise,
        risers,
        riser_height,
        treads,
        unit_run: UNIT_RUN,
        total_run,
        stringers,
        stringer_length,
        handrails: risers >= 4,
        rationale: vec![
            format!(
                "{risers} risers at {} and {treads} treads at {UNIT_RUN}\" run (angle {angle:.0} degrees).",
                fmt_ft_in(riser_height)
            ),
            format!(
                "{stringers} 2x12 stringers at {}\" OC max, hung from the {} rim.",
                spec.stair_stringer_spacing,
                joist.label()
            ),
            format!("Concrete landing pad {}\" x {} x {}\".", PAD_DEPTH, fmt_ft_in(st.width + 12.0), PAD_THICKNESS),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riser_math() {
        let rise: f64 = 36.0;
        let n = (rise / MAX_RISER).ceil();
        assert_eq!(n, 5.0);
        assert!(rise / n <= MAX_RISER);
    }
}
