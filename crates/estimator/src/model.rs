//! Input model: what the contractor describes about the deck.
//!
//! All lengths are in **inches** (f64). The UI is responsible for feet/inches entry.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Project {
    pub sections: Vec<Section>,
    #[serde(default)]
    pub stairs: Vec<Stair>,
    pub materials: MaterialChoices,
    #[serde(default)]
    pub site: SiteConditions,
    #[serde(default)]
    pub options: EstimateOptions,
}

/// One rectangular piece of the deck in plan view. Combine several for L/T or multi-level decks.
///
/// Plan coordinates: `x` grows east, `y` grows south. Edges are named by compass direction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Section {
    pub name: String,
    /// West edge position.
    #[serde(default)]
    pub x: f64,
    /// North edge position.
    #[serde(default)]
    pub y: f64,
    /// East-west extent.
    pub width: f64,
    /// North-south extent.
    pub depth: f64,
    /// Top of decking above finished grade.
    pub height: f64,
    /// Axis the joists run along. Decking runs across the joists, on the other axis.
    #[serde(default = "default_joist_axis")]
    pub joists_along: Axis,
    /// Edge fastened to the house with a ledger; `None` for a freestanding section.
    /// Must be perpendicular to `joists_along`.
    #[serde(default)]
    pub ledger_edge: Option<Edge>,
    /// Railing on open edges. `None` = automatic (required when height > 30").
    #[serde(default)]
    pub railing: Option<bool>,
}

fn default_joist_axis() -> Axis {
    Axis::NorthSouth
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    NorthSouth,
    EastWest,
}

impl Axis {
    pub fn perpendicular(self) -> Axis {
        match self {
            Axis::NorthSouth => Axis::EastWest,
            Axis::EastWest => Axis::NorthSouth,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Axis::NorthSouth => "north-south",
            Axis::EastWest => "east-west",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    North,
    South,
    East,
    West,
}

impl Edge {
    pub const ALL: [Edge; 4] = [Edge::North, Edge::South, Edge::East, Edge::West];

    pub fn opposite(self) -> Edge {
        match self {
            Edge::North => Edge::South,
            Edge::South => Edge::North,
            Edge::East => Edge::West,
            Edge::West => Edge::East,
        }
    }

    /// The axis this edge runs along.
    pub fn runs_along(self) -> Axis {
        match self {
            Edge::North | Edge::South => Axis::EastWest,
            Edge::East | Edge::West => Axis::NorthSouth,
        }
    }
}

/// Stairs leaving a section edge, down to grade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Stair {
    /// Index into `Project::sections`.
    pub section: usize,
    pub edge: Edge,
    /// Distance along the edge (from its west/north end) to the stair's near side.
    #[serde(default)]
    pub offset: f64,
    pub width: f64,
    /// Total rise; defaults to the section height.
    #[serde(default)]
    pub rise: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MaterialChoices {
    #[serde(default)]
    pub framing: FramingSpecies,
    pub decking: DeckingType,
    #[serde(default)]
    pub fastening: Fastening,
    #[serde(default)]
    pub railing: RailingType,
    #[serde(default)]
    pub footing: FootingType,
    /// Structural post size. 4x4 is used only where it's allowed (post height, beam bearing);
    /// other posts are upsized to 6x6 with a warning.
    #[serde(default)]
    pub post_size: PostSize,
    #[serde(default)]
    pub ledger_fastener: LedgerFastener,
    /// Joist spacing on center; `None` = widest the decking allows.
    #[serde(default)]
    pub joist_spacing: Option<JoistSpacing>,
    /// Smallest joist size to consider (many builders won't frame decks with 2x6).
    #[serde(default = "default_min_joist")]
    pub min_joist: LumberSize,
    #[serde(default)]
    pub picture_frame: bool,
    /// Butyl joist-protection tape on tops of joists, beams and rims.
    #[serde(default = "default_true")]
    pub joist_tape: bool,
    /// Fascia boards over exposed rim joists.
    #[serde(default)]
    pub fascia: bool,
}

fn default_min_joist() -> LumberSize {
    LumberSize::TwoBy8
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FramingSpecies {
    #[default]
    SouthernPine,
    DouglasFirLarch,
    HemFir,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DeckingType {
    /// Pressure-treated 5/4x6 radius-edge decking.
    Pt54x6,
    /// Pressure-treated 2x6.
    Pt2x6,
    /// Cedar 5/4x6.
    Cedar54x6,
    /// Generic grooved-edge composite (Trex/TimberTech/etc. class).
    Composite,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Fastening {
    #[default]
    FaceScrews,
    HiddenClips,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RailingType {
    #[default]
    WoodBalusters,
    MetalBalusters,
    PanelKits,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FootingType {
    /// Sonotube / hole poured with bagged concrete.
    #[default]
    PouredBags,
    /// Precast deck blocks (low, freestanding decks only).
    PrecastBlocks,
}

/// Structural post cross-sections.
#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum PostSize {
    #[serde(rename = "4x4")]
    FourBy4,
    #[default]
    #[serde(rename = "6x6")]
    SixBy6,
}

impl PostSize {
    /// Actual width in inches.
    pub fn width(self) -> f64 {
        match self {
            PostSize::FourBy4 => 3.5,
            PostSize::SixBy6 => 5.5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PostSize::FourBy4 => "4x4",
            PostSize::SixBy6 => "6x6",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum LedgerFastener {
    #[default]
    StructuralScrews,
    HalfInchLags,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum JoistSpacing {
    #[serde(rename = "12")]
    Oc12,
    #[serde(rename = "16")]
    Oc16,
    #[serde(rename = "24")]
    Oc24,
}

impl JoistSpacing {
    pub const ALL: [JoistSpacing; 3] = [JoistSpacing::Oc12, JoistSpacing::Oc16, JoistSpacing::Oc24];

    pub fn inches(self) -> f64 {
        match self {
            JoistSpacing::Oc12 => 12.0,
            JoistSpacing::Oc16 => 16.0,
            JoistSpacing::Oc24 => 24.0,
        }
    }
}

/// Dimensional lumber cross-sections used for framing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LumberSize {
    #[serde(rename = "2x4")]
    TwoBy4,
    #[serde(rename = "2x6")]
    TwoBy6,
    #[serde(rename = "2x8")]
    TwoBy8,
    #[serde(rename = "2x10")]
    TwoBy10,
    #[serde(rename = "2x12")]
    TwoBy12,
}

impl LumberSize {
    pub const JOISTS: [LumberSize; 4] = [
        LumberSize::TwoBy6,
        LumberSize::TwoBy8,
        LumberSize::TwoBy10,
        LumberSize::TwoBy12,
    ];

    /// Actual depth in inches.
    pub fn depth(self) -> f64 {
        match self {
            LumberSize::TwoBy4 => 3.5,
            LumberSize::TwoBy6 => 5.5,
            LumberSize::TwoBy8 => 7.25,
            LumberSize::TwoBy10 => 9.25,
            LumberSize::TwoBy12 => 11.25,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LumberSize::TwoBy4 => "2x4",
            LumberSize::TwoBy6 => "2x6",
            LumberSize::TwoBy8 => "2x8",
            LumberSize::TwoBy10 => "2x10",
            LumberSize::TwoBy12 => "2x12",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SiteConditions {
    /// Footing bottom depth below grade (local frost line).
    pub frost_depth: f64,
    /// Design live load, psf (40 typical, higher for snow country).
    pub live_load_psf: f64,
    /// Allowable soil bearing pressure, psf (1500 = IRC presumptive minimum).
    pub soil_bearing_psf: f64,
}

impl Default for SiteConditions {
    fn default() -> Self {
        SiteConditions {
            frost_depth: 36.0,
            live_load_psf: 40.0,
            soil_bearing_psf: 1500.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EstimateOptions {
    /// Extra decking boards, percent (covers defects and cupped boards).
    pub decking_overage_pct: f64,
    /// Extra framing lumber, percent. Cut optimization already accounts for offcuts.
    pub framing_overage_pct: f64,
    /// Saw kerf per cut.
    pub kerf: f64,
    /// Offcuts at least this long are reported as reusable.
    pub min_reusable_offcut: f64,
    /// Longest stock board to buy (feet). Yards often stock 20' but it can be hard to haul.
    pub max_stock_ft: u32,
}

impl Default for EstimateOptions {
    fn default() -> Self {
        EstimateOptions {
            decking_overage_pct: 5.0,
            framing_overage_pct: 0.0,
            kerf: 0.125,
            min_reusable_offcut: 24.0,
            max_stock_ft: 20,
        }
    }
}
