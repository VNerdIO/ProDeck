//! Plan-view geometry: section frames, adjacency, and which edges are open (need railing,
//! fascia) versus shared with another section or the house.

use serde::{Deserialize, Serialize};

use crate::model::{Axis, Edge, Section, Stair};

const EPS: f64 = 1e-6;
/// Drop at an edge above which a guard is required (IRC R312).
pub const GUARD_HEIGHT: f64 = 30.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Segment {
    pub a: Point,
    pub b: Point,
}

impl Segment {
    pub fn length(&self) -> f64 {
        ((self.b.x - self.a.x).powi(2) + (self.b.y - self.a.y).powi(2)).sqrt()
    }
}

/// A section's framing frame: `u` runs across the joists (along beams/ledger), `v` runs along
/// the joists starting at the ledger side (or the north/west side when freestanding).
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    x0: f64,
    y0: f64,
    /// Length along the joists.
    pub run: f64,
    /// Length across the joists.
    pub cross: f64,
    along: Axis,
    /// `v` measured from the south/east side.
    flip: bool,
}

impl Frame {
    pub fn new(s: &Section) -> Frame {
        let (run, cross) = match s.joists_along {
            Axis::NorthSouth => (s.depth, s.width),
            Axis::EastWest => (s.width, s.depth),
        };
        let flip = matches!(s.ledger_edge, Some(Edge::South) | Some(Edge::East));
        Frame {
            x0: s.x,
            y0: s.y,
            run,
            cross,
            along: s.joists_along,
            flip,
        }
    }

    pub fn to_plan(&self, u: f64, v: f64) -> Point {
        let v = if self.flip { self.run - v } else { v };
        match self.along {
            Axis::NorthSouth => Point {
                x: self.x0 + u,
                y: self.y0 + v,
            },
            Axis::EastWest => Point {
                x: self.x0 + v,
                y: self.y0 + u,
            },
        }
    }

    pub fn seg(&self, u0: f64, v0: f64, u1: f64, v1: f64) -> Segment {
        Segment {
            a: self.to_plan(u0, v0),
            b: self.to_plan(u1, v1),
        }
    }
}

/// An interval `[a, b]` along a section edge, measured from the edge's west/north end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub a: f64,
    pub b: f64,
}

impl Span {
    pub fn len(&self) -> f64 {
        self.b - self.a
    }
}

/// Open and guarded portions of one section edge.
#[derive(Debug, Clone)]
pub struct EdgeInfo {
    pub edge: Edge,
    pub length: f64,
    /// Not against the house or another section: rim is visible (fascia) and boards overhang.
    pub exposed: Vec<Span>,
    /// Exposed or a drop to a lower section of more than 30": needs a guard (before stair openings).
    pub drop_off: Vec<Span>,
}

pub struct Layout {
    pub frames: Vec<Frame>,
    /// Per section, per edge.
    pub edges: Vec<Vec<EdgeInfo>>,
}

/// The fixed coordinate of an edge's line and the extent along it, in plan coordinates.
fn edge_line(s: &Section, e: Edge) -> (f64, f64, f64) {
    match e {
        Edge::North => (s.y, s.x, s.x + s.width),
        Edge::South => (s.y + s.depth, s.x, s.x + s.width),
        Edge::West => (s.x, s.y, s.y + s.depth),
        Edge::East => (s.x + s.width, s.y, s.y + s.depth),
    }
}

pub fn edge_length(s: &Section, e: Edge) -> f64 {
    let (_, a, b) = edge_line(s, e);
    b - a
}

fn subtract(spans: &[Span], cut: Span) -> Vec<Span> {
    let mut out = Vec::new();
    for s in spans {
        if cut.b <= s.a + EPS || cut.a >= s.b - EPS {
            out.push(*s);
            continue;
        }
        if cut.a > s.a + EPS {
            out.push(Span { a: s.a, b: cut.a });
        }
        if cut.b < s.b - EPS {
            out.push(Span { a: cut.b, b: s.b });
        }
    }
    out
}

/// Remove stair openings from guard spans.
pub fn subtract_stairs(spans: &[Span], section: usize, edge: Edge, stairs: &[Stair]) -> Vec<Span> {
    stairs
        .iter()
        .filter(|st| st.section == section && st.edge == edge)
        .fold(spans.to_vec(), |acc, st| {
            subtract(
                &acc,
                Span {
                    a: st.offset,
                    b: st.offset + st.width,
                },
            )
        })
}

pub fn overlaps(a: &Section, b: &Section) -> bool {
    a.x < b.x + b.width - EPS
        && b.x < a.x + a.width - EPS
        && a.y < b.y + b.depth - EPS
        && b.y < a.y + a.depth - EPS
}

pub fn layout(sections: &[Section]) -> Layout {
    let frames = sections.iter().map(Frame::new).collect();
    let mut edges = Vec::with_capacity(sections.len());
    for (i, s) in sections.iter().enumerate() {
        let mut per = Vec::new();
        for e in Edge::ALL {
            let (line, a0, b0) = edge_line(s, e);
            let full = vec![Span { a: 0.0, b: b0 - a0 }];
            if s.ledger_edge == Some(e) {
                per.push(EdgeInfo {
                    edge: e,
                    length: b0 - a0,
                    exposed: vec![],
                    drop_off: vec![],
                });
                continue;
            }
            let mut exposed = full.clone();
            let mut drop_off = full;
            for (j, o) in sections.iter().enumerate() {
                if i == j {
                    continue;
                }
                let (oline, oa, ob) = edge_line(o, e.opposite());
                if (oline - line).abs() > EPS {
                    continue;
                }
                let a = oa.max(a0);
                let b = ob.min(b0);
                if b - a <= EPS {
                    continue;
                }
                let cut = Span {
                    a: a - a0,
                    b: b - a0,
                };
                exposed = subtract(&exposed, cut);
                if s.height - o.height <= GUARD_HEIGHT {
                    drop_off = subtract(&drop_off, cut);
                }
            }
            per.push(EdgeInfo {
                edge: e,
                length: b0 - a0,
                exposed,
                drop_off,
            });
        }
        edges.push(per);
    }
    Layout { frames, edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sec(x: f64, y: f64, w: f64, d: f64, h: f64) -> Section {
        Section {
            name: "s".into(),
            x,
            y,
            width: w,
            depth: d,
            height: h,
            joists_along: Axis::NorthSouth,
            ledger_edge: Some(Edge::North),
            railing: None,
        }
    }

    #[test]
    fn frame_flips_for_south_ledger() {
        let mut s = sec(0.0, 0.0, 120.0, 96.0, 36.0);
        s.ledger_edge = Some(Edge::South);
        let f = Frame::new(&s);
        assert_eq!(f.to_plan(0.0, 0.0), Point { x: 0.0, y: 96.0 });
    }

    #[test]
    fn l_shape_shared_edge_is_not_exposed() {
        // A: 16' x 12' at origin. B: 8' x 8' hanging off A's south edge, west-aligned.
        let a = sec(0.0, 0.0, 192.0, 144.0, 36.0);
        let mut b = sec(0.0, 144.0, 96.0, 96.0, 36.0);
        b.ledger_edge = None;
        let l = layout(&[a, b]);
        let a_south = l.edges[0].iter().find(|e| e.edge == Edge::South).unwrap();
        assert_eq!(a_south.exposed, vec![Span { a: 96.0, b: 192.0 }]);
        let b_north = l.edges[1].iter().find(|e| e.edge == Edge::North).unwrap();
        assert!(b_north.exposed.is_empty());
        assert!(b_north.drop_off.is_empty());
        // Ledger edge contributes nothing.
        assert!(l.edges[0]
            .iter()
            .find(|e| e.edge == Edge::North)
            .unwrap()
            .exposed
            .is_empty());
    }

    #[test]
    fn big_step_down_needs_guard_on_upper() {
        let a = sec(0.0, 0.0, 120.0, 120.0, 72.0);
        let mut b = sec(0.0, 120.0, 120.0, 120.0, 30.0);
        b.ledger_edge = None;
        let l = layout(&[a, b]);
        let a_south = l.edges[0].iter().find(|e| e.edge == Edge::South).unwrap();
        assert!(a_south.exposed.is_empty());
        assert_eq!(a_south.drop_off.len(), 1);
    }

    #[test]
    fn detects_overlap() {
        assert!(overlaps(
            &sec(0.0, 0.0, 100.0, 100.0, 0.0),
            &sec(50.0, 50.0, 100.0, 100.0, 0.0)
        ));
        assert!(!overlaps(
            &sec(0.0, 0.0, 100.0, 100.0, 0.0),
            &sec(100.0, 0.0, 100.0, 100.0, 0.0)
        ));
    }
}
