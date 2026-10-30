//! One-dimensional cutting-stock optimizer.
//!
//! Given the pieces a material must be cut into and the stock lengths the store sells, choose
//! which boards to buy and how to cut them. Objective, in order: least total linear length
//! bought, then fewest boards.
//!
//! Strategy: run several greedy packings (first-fit decreasing seeded with each stock length,
//! and best-fit decreasing), then improve each by emptying the least-full boards into other
//! boards' free space and shrinking every board to the shortest stock length that still holds
//! its cuts. Keep the best result. Deterministic for a given input.

use serde::{Deserialize, Serialize};

const EPS: f64 = 1e-6;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Piece {
    /// Inches.
    pub length: f64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StockBoard {
    /// Inches.
    pub stock_length: f64,
    pub cuts: Vec<Piece>,
    /// Leftover after cuts and kerf, inches.
    pub waste: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CutError {
    PieceTooLong {
        label: String,
        length: f64,
        max: f64,
    },
    NoStock,
}

impl std::fmt::Display for CutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CutError::PieceTooLong { label, length, max } => write!(
                f,
                "{label} is {:.1}\" long but the longest stock board is {:.0}\"",
                length, max
            ),
            CutError::NoStock => write!(f, "no stock lengths available"),
        }
    }
}

#[derive(Clone)]
struct Bin {
    capacity: f64,
    pieces: Vec<usize>,
    used: f64,
}

impl Bin {
    fn new(capacity: f64) -> Bin {
        Bin {
            capacity,
            pieces: Vec::new(),
            used: 0.0,
        }
    }

    /// Length consumed if `len` is added (kerf between adjacent cuts).
    fn used_with(&self, len: f64, kerf: f64) -> f64 {
        if self.pieces.is_empty() {
            len
        } else {
            self.used + kerf + len
        }
    }

    fn fits(&self, len: f64, kerf: f64) -> bool {
        self.used_with(len, kerf) <= self.capacity + EPS
    }

    fn add(&mut self, idx: usize, len: f64, kerf: f64) {
        self.used = self.used_with(len, kerf);
        self.pieces.push(idx);
    }
}

/// Total kerf-inclusive length of a set of pieces.
#[cfg(test)]
fn packed_len(lengths: &[f64], kerf: f64) -> f64 {
    if lengths.is_empty() {
        0.0
    } else {
        lengths.iter().sum::<f64>() + kerf * (lengths.len() - 1) as f64
    }
}

fn shortest_fitting(stock: &[f64], used: f64) -> Option<f64> {
    stock.iter().copied().find(|s| used <= s + EPS)
}

/// Cut `pieces` from boards of the given stock lengths (inches, any order).
pub fn optimize(pieces: &[Piece], stock: &[f64], kerf: f64) -> Result<Vec<StockBoard>, CutError> {
    let mut stock: Vec<f64> = stock.to_vec();
    stock.sort_by(f64::total_cmp);
    stock.dedup();
    let max = *stock.last().ok_or(CutError::NoStock)?;
    if let Some(p) = pieces.iter().find(|p| p.length > max + EPS) {
        return Err(CutError::PieceTooLong {
            label: p.label.clone(),
            length: p.length,
            max,
        });
    }
    if pieces.is_empty() {
        return Ok(Vec::new());
    }

    // Longest first; ties broken by label so output is stable.
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    order.sort_by(|a, b| {
        pieces[*b]
            .length
            .total_cmp(&pieces[*a].length)
            .then_with(|| pieces[*a].label.cmp(&pieces[*b].label))
    });
    let lens: Vec<f64> = pieces.iter().map(|p| p.length).collect();

    let mut candidates = Vec::new();
    for &open in &stock {
        candidates.push(pack(&order, &lens, &stock, kerf, open, false));
    }
    candidates.push(pack(&order, &lens, &stock, kerf, max, true));

    let best = candidates
        .into_iter()
        .map(|bins| improve(bins, &lens, &stock, kerf))
        .min_by(|a, b| score(a).partial_cmp(&score(b)).expect("finite"))
        .expect("at least one candidate");

    let mut boards: Vec<StockBoard> = best
        .into_iter()
        .map(|b| {
            let mut cuts: Vec<Piece> = b.pieces.iter().map(|i| pieces[*i].clone()).collect();
            cuts.sort_by(|a, b| {
                b.length
                    .total_cmp(&a.length)
                    .then_with(|| a.label.cmp(&b.label))
            });
            StockBoard {
                stock_length: b.capacity,
                waste: (b.capacity - b.used).max(0.0),
                cuts,
            }
        })
        .collect();
    boards.sort_by(|a, b| {
        b.stock_length
            .total_cmp(&a.stock_length)
            .then_with(|| a.waste.total_cmp(&b.waste))
    });
    Ok(boards)
}

fn score(bins: &[Bin]) -> (f64, usize) {
    (bins.iter().map(|b| b.capacity).sum(), bins.len())
}

/// Greedy packing. New boards open at `open` length (or the shortest that fits a piece longer
/// than `open`). `best_fit` places into the fullest board that fits; otherwise the first.
fn pack(
    order: &[usize],
    lens: &[f64],
    stock: &[f64],
    kerf: f64,
    open: f64,
    best_fit: bool,
) -> Vec<Bin> {
    let mut bins: Vec<Bin> = Vec::new();
    for &i in order {
        let len = lens[i];
        let target = if best_fit {
            bins.iter_mut()
                .filter(|b| b.fits(len, kerf))
                .min_by(|a, b| (a.capacity - a.used).total_cmp(&(b.capacity - b.used)))
        } else {
            bins.iter_mut().find(|b| b.fits(len, kerf))
        };
        match target {
            Some(b) => b.add(i, len, kerf),
            None => {
                let cap = if len <= open + EPS {
                    open
                } else {
                    shortest_fitting(stock, len).expect("checked against max")
                };
                let mut b = Bin::new(cap);
                b.add(i, len, kerf);
                bins.push(b);
            }
        }
    }
    bins
}

/// Shrink each board to the shortest stock that holds it, then repeatedly try to empty the
/// least-full board into the others (growing a receiving board to a longer stock length if
/// that is cheaper than keeping the emptied board).
fn improve(mut bins: Vec<Bin>, lens: &[f64], stock: &[f64], kerf: f64) -> Vec<Bin> {
    shrink(&mut bins, stock);
    loop {
        let mut improved = false;
        let mut idx: Vec<usize> = (0..bins.len()).collect();
        idx.sort_by(|a, b| bins[*a].used.total_cmp(&bins[*b].used));
        for &victim in &idx {
            if let Some(next) = try_empty(&bins, victim, lens, stock, kerf) {
                bins = next;
                improved = true;
                break;
            }
        }
        if !improved {
            break;
        }
    }
    bins
}

fn shrink(bins: &mut [Bin], stock: &[f64]) {
    for b in bins.iter_mut() {
        if let Some(s) = shortest_fitting(stock, b.used) {
            b.capacity = s;
        }
    }
}

/// Redistribute every piece of `victim` into other boards. Succeeds only if total stock
/// length bought goes down.
fn try_empty(
    bins: &[Bin],
    victim: usize,
    lens: &[f64],
    stock: &[f64],
    kerf: f64,
) -> Option<Vec<Bin>> {
    let max = *stock.last()?;
    let before = score(bins).0;
    let mut rest: Vec<Bin> = bins
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != victim)
        .map(|(_, b)| b.clone())
        .collect();
    if rest.is_empty() {
        return None;
    }
    let mut moving = bins[victim].pieces.clone();
    moving.sort_by(|a, b| lens[*b].total_cmp(&lens[*a]));
    for p in moving {
        let len = lens[p];
        // Prefer a board with room as-is; otherwise grow the board that needs the least growth.
        let pick = rest
            .iter()
            .enumerate()
            .filter(|(_, b)| b.fits(len, kerf))
            .min_by(|a, b| (a.1.capacity - a.1.used).total_cmp(&(b.1.capacity - b.1.used)))
            .map(|(i, _)| i)
            .or_else(|| {
                rest.iter()
                    .enumerate()
                    .filter(|(_, b)| b.used_with(len, kerf) <= max + EPS)
                    .min_by(|a, b| {
                        let ga = shortest_fitting(stock, a.1.used_with(len, kerf))
                            .unwrap_or(f64::MAX)
                            - a.1.capacity;
                        let gb = shortest_fitting(stock, b.1.used_with(len, kerf))
                            .unwrap_or(f64::MAX)
                            - b.1.capacity;
                        ga.total_cmp(&gb)
                    })
                    .map(|(i, _)| i)
            })?;
        let b = &mut rest[pick];
        b.add(p, len, kerf);
        b.capacity = shortest_fitting(stock, b.used)?.max(b.capacity);
    }
    shrink(&mut rest, stock);
    (score(&rest).0 + EPS < before).then_some(rest)
}

/// Split a run longer than the longest stock into segments that end on supports (joists,
/// posts). `supports` are positions along the run, ascending. `first_max` lets callers stagger
/// splices between adjacent rows by shortening the first segment.
pub fn split_at_supports(total: f64, supports: &[f64], max_len: f64, first_max: f64) -> Vec<f64> {
    if total <= max_len + EPS {
        return vec![total];
    }
    let mut out = Vec::new();
    let mut start = 0.0;
    let mut limit = first_max.min(max_len);
    while total - start > max_len + EPS {
        // Furthest support we can reach that makes progress.
        let cut = supports
            .iter()
            .copied()
            .filter(|s| *s > start + EPS && *s - start <= limit + EPS && *s < total - EPS)
            .fold(None, |acc: Option<f64>, s| {
                Some(acc.map_or(s, |a| a.max(s)))
            });
        // No support in reach: cut at the limit (caller should not let this happen).
        let cut = cut.unwrap_or(start + limit);
        out.push(cut - start);
        start = cut;
        limit = max_len;
    }
    out.push(total - start);
    out
}

/// Pieces that would need splicing anyway; total length of naive one-piece-per-board buying.
pub fn naive_total(pieces: &[Piece], stock: &[f64]) -> f64 {
    let mut stock = stock.to_vec();
    stock.sort_by(f64::total_cmp);
    pieces
        .iter()
        .map(|p| shortest_fitting(&stock, p.length).unwrap_or(f64::INFINITY))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn p(len: f64) -> Piece {
        Piece {
            length: len,
            label: format!("{len}"),
        }
    }

    const FT: f64 = 12.0;

    #[test]
    fn combines_short_pieces() {
        // Four 4' pieces fit one 16' board exactly (no kerf).
        let boards = optimize(
            &[p(48.0), p(48.0), p(48.0), p(48.0)],
            &[96.0, 144.0, 192.0],
            0.0,
        )
        .unwrap();
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].stock_length, 192.0);
    }

    #[test]
    fn kerf_prevents_exact_fit() {
        let boards = optimize(&[p(48.0), p(48.0)], &[96.0, 120.0], 0.125).unwrap();
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].stock_length, 120.0);
    }

    #[test]
    fn picks_shortest_stock_for_single_piece() {
        let boards = optimize(&[p(13.0 * FT)], &[96.0, 144.0, 168.0, 192.0], 0.125).unwrap();
        assert_eq!(boards[0].stock_length, 14.0 * FT);
    }

    #[test]
    fn mixes_lengths_to_reduce_waste() {
        // 10' + 6' pieces: a 16' board is better than 10' + 8'.
        let boards = optimize(&[p(120.0), p(72.0)], &[96.0, 120.0, 144.0, 192.0], 0.0).unwrap();
        let total: f64 = boards.iter().map(|b| b.stock_length).sum();
        assert_eq!(total, 192.0);
    }

    #[test]
    fn rejects_overlong_piece() {
        assert!(matches!(
            optimize(&[p(300.0)], &[240.0], 0.0),
            Err(CutError::PieceTooLong { .. })
        ));
    }

    #[test]
    fn split_lands_on_supports() {
        let joists: Vec<f64> = (0..=24).map(|i| i as f64 * 16.0).collect(); // 0..384
        let segs = split_at_supports(380.0, &joists, 240.0, 240.0);
        assert_eq!(segs.iter().sum::<f64>(), 380.0);
        assert!(segs.iter().all(|s| *s <= 240.0));
        assert_eq!(segs[0], 240.0);
        // Staggered row shortens the first segment.
        let staggered = split_at_supports(380.0, &joists, 240.0, 240.0 - 32.0);
        assert_eq!(staggered[0], 208.0);
    }

    proptest! {
        #[test]
        fn packing_is_valid_and_never_worse_than_naive(
            lens in proptest::collection::vec(6.0f64..240.0, 1..40),
            kerf in 0.0f64..0.25,
        ) {
            let stock: Vec<f64> = (4..=10).map(|f| f as f64 * 24.0).collect(); // 8'..20'
            let pieces: Vec<Piece> = lens.iter().map(|l| p(*l)).collect();
            let boards = optimize(&pieces, &stock, kerf).unwrap();
            // Every piece placed exactly once.
            let placed: usize = boards.iter().map(|b| b.cuts.len()).sum();
            prop_assert_eq!(placed, pieces.len());
            for b in &boards {
                let ls: Vec<f64> = b.cuts.iter().map(|c| c.length).collect();
                prop_assert!(packed_len(&ls, kerf) <= b.stock_length + EPS);
                prop_assert!(stock.contains(&b.stock_length));
            }
            let total: f64 = boards.iter().map(|b| b.stock_length).sum();
            prop_assert!(total <= naive_total(&pieces, &stock) + EPS);
        }
    }
}
