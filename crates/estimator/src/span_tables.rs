//! Prescriptive span tables (see `data/span_tables.toml` for source and caveats).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::model::{FramingSpecies, JoistSpacing, LedgerFastener, LumberSize};

const RAW: &str = include_str!("../data/span_tables.toml");

static TABLES: LazyLock<Tables> =
    LazyLock::new(|| Tables::parse(RAW).expect("span_tables.toml is valid"));

#[derive(Deserialize)]
struct RawTables {
    joists: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    beams: RawBeams,
    ledger: RawLedger,
}

#[derive(Deserialize)]
struct RawBeams {
    joist_spans_ft: Vec<u32>,
    #[serde(flatten)]
    species: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

#[derive(Deserialize)]
struct RawLedger {
    joist_spans_ft: Vec<u32>,
    half_inch_lags: Vec<f64>,
    structural_screws: Vec<f64>,
}

/// A built-up beam: number of plies of a dimensional size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BeamSize {
    pub plies: u8,
    pub size: LumberSize,
}

impl BeamSize {
    pub fn label(self) -> String {
        format!("{}-{}", self.plies, self.size.label())
    }
}

struct Tables {
    /// species -> size -> [12, 16, 24] max span inches
    joists: BTreeMap<String, BTreeMap<LumberSize, [f64; 3]>>,
    beam_cols: Vec<f64>,
    /// species -> beam -> max span inches per joist-span column
    beams: BTreeMap<String, BTreeMap<BeamSize, Vec<f64>>>,
    ledger_cols: Vec<f64>,
    lags: Vec<f64>,
    screws: Vec<f64>,
}

/// Parse "13-1" (feet-inches) into inches.
pub fn parse_ft_in(s: &str) -> Result<f64, String> {
    let (ft, inch) = s.split_once('-').ok_or_else(|| format!("bad span {s:?}"))?;
    let ft: f64 = ft
        .trim()
        .parse()
        .map_err(|_| format!("bad feet in {s:?}"))?;
    let inch: f64 = inch
        .trim()
        .parse()
        .map_err(|_| format!("bad inches in {s:?}"))?;
    Ok(ft * 12.0 + inch)
}

fn parse_size(s: &str) -> Result<LumberSize, String> {
    Ok(match s {
        "2x4" => LumberSize::TwoBy4,
        "2x6" => LumberSize::TwoBy6,
        "2x8" => LumberSize::TwoBy8,
        "2x10" => LumberSize::TwoBy10,
        "2x12" => LumberSize::TwoBy12,
        _ => return Err(format!("unknown lumber size {s:?}")),
    })
}

fn parse_beam(s: &str) -> Result<BeamSize, String> {
    let (plies, size) = s.split_once('-').ok_or_else(|| format!("bad beam {s:?}"))?;
    Ok(BeamSize {
        plies: plies.parse().map_err(|_| format!("bad plies in {s:?}"))?,
        size: parse_size(size)?,
    })
}

impl Tables {
    fn parse(raw: &str) -> Result<Tables, String> {
        let raw: RawTables = toml::from_str(raw).map_err(|e| e.to_string())?;
        let mut joists = BTreeMap::new();
        for (species, sizes) in raw.joists {
            let mut m = BTreeMap::new();
            for (size, spans) in sizes {
                let v: Vec<f64> = spans
                    .iter()
                    .map(|s| parse_ft_in(s))
                    .collect::<Result<_, _>>()?;
                let arr: [f64; 3] = v
                    .try_into()
                    .map_err(|_| format!("{species}/{size}: need 3 spans"))?;
                m.insert(parse_size(&size)?, arr);
            }
            joists.insert(species, m);
        }
        let ncols = raw.beams.joist_spans_ft.len();
        let mut beams = BTreeMap::new();
        for (species, sizes) in raw.beams.species {
            let mut m = BTreeMap::new();
            for (beam, spans) in sizes {
                let v: Vec<f64> = spans
                    .iter()
                    .map(|s| parse_ft_in(s))
                    .collect::<Result<_, _>>()?;
                if v.len() != ncols {
                    return Err(format!("beam {species}/{beam}: expected {ncols} spans"));
                }
                m.insert(parse_beam(&beam)?, v);
            }
            beams.insert(species, m);
        }
        let ledger_cols: Vec<f64> = raw
            .ledger
            .joist_spans_ft
            .iter()
            .map(|f| *f as f64 * 12.0)
            .collect();
        if raw.ledger.half_inch_lags.len() != ledger_cols.len()
            || raw.ledger.structural_screws.len() != ledger_cols.len()
        {
            return Err("ledger table column mismatch".into());
        }
        Ok(Tables {
            joists,
            beam_cols: raw
                .beams
                .joist_spans_ft
                .iter()
                .map(|f| *f as f64 * 12.0)
                .collect(),
            beams,
            ledger_cols,
            lags: raw.ledger.half_inch_lags,
            screws: raw.ledger.structural_screws,
        })
    }
}

fn species_key(s: FramingSpecies) -> &'static str {
    match s {
        FramingSpecies::SouthernPine => "southern_pine",
        // DCA6 groups DF-L, Hem-Fir and SPF in one column.
        FramingSpecies::DouglasFirLarch | FramingSpecies::HemFir => "douglas_fir_larch",
    }
}

fn spacing_idx(s: JoistSpacing) -> usize {
    match s {
        JoistSpacing::Oc12 => 0,
        JoistSpacing::Oc16 => 1,
        JoistSpacing::Oc24 => 2,
    }
}

/// Index of the first column >= `value`; `None` if beyond the table.
fn column_for(cols: &[f64], value: f64) -> Option<usize> {
    cols.iter().position(|c| value <= *c + 1e-6)
}

/// Max joist span in inches, or `None` if the size isn't tabulated.
pub fn joist_max_span(
    species: FramingSpecies,
    size: LumberSize,
    spacing: JoistSpacing,
) -> Option<f64> {
    TABLES
        .joists
        .get(species_key(species))?
        .get(&size)
        .map(|spans| spans[spacing_idx(spacing)])
}

/// Max beam span (post spacing) in inches for a beam carrying joists of `joist_span` inches.
/// `None` if the joist span exceeds the table.
pub fn beam_max_span(species: FramingSpecies, beam: BeamSize, joist_span: f64) -> Option<f64> {
    let col = column_for(&TABLES.beam_cols, joist_span)?;
    TABLES
        .beams
        .get(species_key(species))?
        .get(&beam)
        .map(|v| v[col])
}

/// All tabulated beam sizes for a species.
pub fn beam_sizes(species: FramingSpecies) -> Vec<BeamSize> {
    TABLES
        .beams
        .get(species_key(species))
        .map(|m| m.keys().copied().collect())
        .unwrap_or_default()
}

/// Ledger fastener on-center spacing (inches) for the given joist span.
pub fn ledger_fastener_spacing(fastener: LedgerFastener, joist_span: f64) -> Option<f64> {
    let col = column_for(&TABLES.ledger_cols, joist_span)?;
    Some(match fastener {
        LedgerFastener::HalfInchLags => TABLES.lags[col],
        LedgerFastener::StructuralScrews => TABLES.screws[col],
    })
}

/// Largest joist span covered by the beam/ledger tables, inches.
pub fn max_tabulated_joist_span() -> f64 {
    *TABLES.beam_cols.last().expect("non-empty")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_parse() {
        assert!(!TABLES.joists.is_empty());
    }

    #[test]
    fn parses_feet_inches() {
        assert_eq!(parse_ft_in("13-1").unwrap(), 157.0);
        assert_eq!(parse_ft_in("9-0").unwrap(), 108.0);
    }

    #[test]
    fn joist_lookup() {
        let s = joist_max_span(
            FramingSpecies::SouthernPine,
            LumberSize::TwoBy10,
            JoistSpacing::Oc16,
        )
        .unwrap();
        assert_eq!(s, 14.0 * 12.0);
        // Hem-Fir uses the DF-L column.
        assert_eq!(
            joist_max_span(
                FramingSpecies::HemFir,
                LumberSize::TwoBy8,
                JoistSpacing::Oc16
            ),
            joist_max_span(
                FramingSpecies::DouglasFirLarch,
                LumberSize::TwoBy8,
                JoistSpacing::Oc16
            )
        );
    }

    #[test]
    fn beam_lookup_rounds_joist_span_up() {
        let b = BeamSize {
            plies: 2,
            size: LumberSize::TwoBy10,
        };
        // 11' joist span uses the 12' column.
        assert_eq!(
            beam_max_span(FramingSpecies::SouthernPine, b, 132.0),
            Some(7.0 * 12.0 + 4.0)
        );
        assert_eq!(
            beam_max_span(FramingSpecies::SouthernPine, b, 19.0 * 12.0),
            None
        );
    }

    #[test]
    fn spans_decrease_with_spacing_and_load() {
        for species in [
            FramingSpecies::SouthernPine,
            FramingSpecies::DouglasFirLarch,
        ] {
            for size in LumberSize::JOISTS {
                let v: Vec<f64> = JoistSpacing::ALL
                    .iter()
                    .map(|s| joist_max_span(species, size, *s).unwrap())
                    .collect();
                assert!(v.windows(2).all(|w| w[0] >= w[1]), "{species:?} {size:?}");
            }
            for beam in beam_sizes(species) {
                let v: Vec<f64> = TABLES
                    .beam_cols
                    .iter()
                    .map(|c| beam_max_span(species, beam, *c).unwrap())
                    .collect();
                assert!(v.windows(2).all(|w| w[0] >= w[1]), "{species:?} {beam:?}");
            }
        }
    }
}
