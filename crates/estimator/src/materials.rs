//! Material catalog: generic big-box items, the stock lengths they're sold in, and how
//! loose counts are packaged (boxes, tubs, bags). Quantities only — no SKUs or prices.

use serde::{Deserialize, Serialize};

use crate::model::{DeckingType, FramingSpecies, LumberSize, MaterialChoices, PostSize};

/// Store department, used to group the pick list the way the store is laid out.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Lumber,
    Decking,
    Railing,
    Concrete,
    Hardware,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Lumber => "Lumber",
            Category::Decking => "Decking",
            Category::Railing => "Railing",
            Category::Concrete => "Concrete",
            Category::Hardware => "Hardware & Fasteners",
        }
    }
}

/// Linear stock that gets cut: each has a set of lengths sold at the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stock {
    Framing(LumberSize),
    Post(PostSize),
    RailPost4x4,
    Decking,
    Fascia,
    /// 1x8 riser board for wood stairs (composite stairs use fascia).
    Riser1x8,
}

/// Physical properties of the chosen decking.
#[derive(Debug, Clone, Copy)]
pub struct DeckingSpec {
    pub width: f64,
    pub thickness: f64,
    pub gap: f64,
    /// Widest joist spacing the manufacturer allows for perpendicular installation.
    pub max_joist_spacing: f64,
    pub stair_stringer_spacing: f64,
    pub composite: bool,
}

pub fn decking_spec(d: DeckingType) -> DeckingSpec {
    match d {
        DeckingType::Pt54x6 => DeckingSpec {
            width: 5.5,
            thickness: 1.0,
            gap: 0.125,
            max_joist_spacing: 16.0,
            stair_stringer_spacing: 16.0,
            composite: false,
        },
        DeckingType::Pt2x6 => DeckingSpec {
            width: 5.5,
            thickness: 1.5,
            gap: 0.125,
            max_joist_spacing: 24.0,
            stair_stringer_spacing: 16.0,
            composite: false,
        },
        DeckingType::Cedar54x6 => DeckingSpec {
            width: 5.5,
            thickness: 1.0,
            gap: 0.125,
            max_joist_spacing: 16.0,
            stair_stringer_spacing: 16.0,
            composite: false,
        },
        DeckingType::Composite => DeckingSpec {
            width: 5.5,
            thickness: 0.94,
            gap: 0.1875,
            max_joist_spacing: 16.0,
            stair_stringer_spacing: 12.0,
            composite: true,
        },
    }
}

fn species_label(s: FramingSpecies) -> &'static str {
    match s {
        FramingSpecies::SouthernPine => "PT Southern Pine #2",
        FramingSpecies::DouglasFirLarch => "PT Douglas Fir-Larch #2",
        FramingSpecies::HemFir => "PT Hem-Fir #2",
    }
}

fn even_feet(from: u32, to: u32) -> Vec<u32> {
    (from..=to).step_by(2).collect()
}

impl Stock {
    pub fn category(self) -> Category {
        match self {
            Stock::Framing(_) | Stock::Post(_) => Category::Lumber,
            Stock::Decking | Stock::Fascia | Stock::Riser1x8 => Category::Decking,
            Stock::RailPost4x4 => Category::Railing,
        }
    }

    pub fn description(self, m: &MaterialChoices) -> String {
        match self {
            Stock::Framing(size) => format!(
                "{} {} (ground contact)",
                size.label(),
                species_label(m.framing)
            ),
            Stock::Post(size) => format!("{} PT post (ground contact)", size.label()),
            Stock::RailPost4x4 => "4x4 PT post".into(),
            Stock::Decking => match m.decking {
                DeckingType::Pt54x6 => "5/4x6 PT radius-edge decking".into(),
                DeckingType::Pt2x6 => "2x6 PT decking".into(),
                DeckingType::Cedar54x6 => "5/4x6 cedar decking".into(),
                DeckingType::Composite => "Composite decking, grooved edge".into(),
            },
            Stock::Fascia => {
                if decking_spec(m.decking).composite {
                    "Composite fascia 1x12".into()
                } else {
                    "1x8 PT fascia".into()
                }
            }
            Stock::Riser1x8 => "1x8 PT riser board".into(),
        }
    }

    /// Lengths sold, in feet, ascending, capped at `max_ft`.
    pub fn stock_lengths_ft(self, m: &MaterialChoices, max_ft: u32) -> Vec<u32> {
        let all = match self {
            Stock::Framing(_) => even_feet(8, 20),
            Stock::Post(_) => vec![8, 10, 12, 16],
            Stock::RailPost4x4 => vec![8, 10, 12],
            Stock::Decking => match m.decking {
                DeckingType::Pt54x6 => vec![8, 10, 12, 16, 20],
                DeckingType::Pt2x6 => even_feet(8, 20),
                DeckingType::Cedar54x6 => vec![8, 10, 12, 16],
                DeckingType::Composite => vec![12, 16, 20],
            },
            Stock::Fascia => {
                if decking_spec(m.decking).composite {
                    vec![12]
                } else {
                    vec![8, 10, 12, 16]
                }
            }
            Stock::Riser1x8 => vec![8, 10, 12, 16],
        };
        let capped: Vec<u32> = all.iter().copied().filter(|f| *f <= max_ft).collect();
        // Never return nothing: fall back to the shortest length sold.
        if capped.is_empty() {
            vec![all[0]]
        } else {
            capped
        }
    }
}

/// Counted (non-cut) items. Each knows its packaging so raw counts become store units.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Item {
    JoistHanger(LumberSize),
    HurricaneTie,
    PostBase(PostSize),
    PostCap { post: PostSize, plies: u8 },
    ConnectorScrews,
    FramingScrews,
    LedgerFastener { lags: bool },
    LedgerFlashing,
    JoistTape,
    DeckScrews { composite: bool },
    HiddenClips,
    FasciaScrews,
    ConcreteBag80,
    FormTube { diameter_in: u32 },
    DeckBlock,
    StringerConnector,
    CarriageBolt,
    Baluster2x2 { length_in: u32 },
    MetalBaluster { length_in: u32 },
    RailingPanelKit { length_ft: u32, stair: bool },
    PostSleeve,
    PostSleeveCap,
}

/// How an item is sold: `per` raw units in one store unit.
pub struct Packaging {
    pub per: f64,
    pub unit: &'static str,
}

impl Item {
    pub fn category(&self) -> Category {
        match self {
            Item::ConcreteBag80 | Item::FormTube { .. } | Item::DeckBlock => Category::Concrete,
            Item::Baluster2x2 { .. }
            | Item::MetalBaluster { .. }
            | Item::RailingPanelKit { .. }
            | Item::PostSleeve
            | Item::PostSleeveCap => Category::Railing,
            Item::HiddenClips | Item::DeckScrews { .. } | Item::FasciaScrews => Category::Decking,
            _ => Category::Hardware,
        }
    }

    pub fn description(&self) -> String {
        match self {
            Item::JoistHanger(s) => format!("Joist hanger, {} (galvanized)", s.label()),
            Item::HurricaneTie => "Hurricane tie, joist-to-beam".into(),
            Item::PostBase(post) => format!("Post base, {} adjustable", post.label()),
            Item::PostCap { post, plies } => {
                format!("Post-beam cap, {} to {plies}-ply beam", post.label())
            }
            Item::ConnectorScrews => "Structural connector screws #9 x 1-1/2\" (100 ct)".into(),
            Item::FramingScrews => "Exterior framing screws 3\" (5 lb box, ~350 ct)".into(),
            Item::LedgerFastener { lags: true } => {
                "1/2\" x 4\" galvanized lag screw + washer".into()
            }
            Item::LedgerFastener { lags: false } => {
                "Structural ledger screw 5\" (LedgerLOK class)".into()
            }
            Item::LedgerFlashing => "Ledger flashing, Z-style 10'".into(),
            Item::JoistTape => "Joist/butyl tape, 50' roll".into(),
            Item::DeckScrews { composite: true } => {
                "Composite deck screws, color-matched (350 ct)".into()
            }
            Item::DeckScrews { composite: false } => {
                "Exterior deck screws 2-1/2\" (5 lb, ~350 ct)".into()
            }
            Item::HiddenClips => "Hidden deck fastener clips (tub of 90 w/ screws)".into(),
            Item::FasciaScrews => "Fascia screws, color-matched (100 ct)".into(),
            Item::ConcreteBag80 => "Concrete mix, 80 lb bag".into(),
            Item::FormTube { diameter_in } => format!("Concrete form tube {diameter_in}\" x 4'"),
            Item::DeckBlock => "Precast concrete deck block".into(),
            Item::StringerConnector => "Stair stringer connector".into(),
            Item::CarriageBolt => "1/2\" x 8\" galvanized carriage bolt + nut + washer".into(),
            Item::Baluster2x2 { length_in } => format!("2x2 PT baluster {length_in}\""),
            Item::MetalBaluster { length_in } => {
                format!("Aluminum baluster {length_in}\" w/ connectors (10 pk)")
            }
            Item::RailingPanelKit {
                length_ft,
                stair: false,
            } => format!("Railing kit, {length_ft}' level"),
            Item::RailingPanelKit {
                length_ft,
                stair: true,
            } => format!("Railing kit, {length_ft}' stair"),
            Item::PostSleeve => "Post sleeve for 4x4".into(),
            Item::PostSleeveCap => "Post sleeve cap + skirt".into(),
        }
    }

    pub fn packaging(&self) -> Packaging {
        let (per, unit) = match self {
            Item::ConnectorScrews => (100.0, "box"),
            Item::FramingScrews => (350.0, "box"),
            Item::DeckScrews { .. } => (350.0, "box"),
            Item::HiddenClips => (90.0, "tub"),
            Item::FasciaScrews => (100.0, "pack"),
            Item::MetalBaluster { .. } => (10.0, "pack"),
            Item::JoistTape => (600.0, "roll"), // counted in inches
            Item::ConcreteBag80 => (0.6, "bag"), // counted in cubic feet
            Item::LedgerFlashing => (120.0, "pc"), // counted in inches
            _ => (1.0, "ea"),
        };
        Packaging { per, unit }
    }
}
