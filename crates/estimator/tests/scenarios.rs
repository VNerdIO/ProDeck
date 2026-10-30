//! End-to-end scenarios. Quantities asserted here were checked by hand; when the engine
//! changes on purpose, re-verify before updating them.

use estimator::materials::Category;
use estimator::{estimate, Axis, Edge, Estimate, PostSize, Project};

fn load(name: &str) -> Project {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn qty(e: &Estimate, desc_contains: &str) -> u32 {
    e.line_items
        .iter()
        .filter(|l| l.description.contains(desc_contains))
        .map(|l| l.quantity)
        .sum()
}

fn pieces(e: &Estimate, label: &str) -> usize {
    e.cut_lists
        .iter()
        .flat_map(|c| &c.boards)
        .flat_map(|b| &b.cuts)
        .filter(|c| c.label == label)
        .count()
}

#[test]
fn attached_12x16_pt() {
    let e = estimate(&load("attached_12x16_pt")).unwrap();
    let s = &e.sections[0];
    // 12' run attached: 11' bay with a 1' cantilever. SYP 2x8 @ 16" spans 11'-10".
    assert_eq!(s.joist_size.label(), "2x8");
    assert_eq!(s.joist_spacing, 16.0);
    assert_eq!(s.joist_count, 13); // 16' / 16" OC + 1
    assert_eq!(s.beams.len(), 1);
    assert_eq!(s.beams[0].size, "2-2x10");
    assert_eq!(s.beams[0].posts, 3);
    assert_eq!(s.footing.diameter, 18.0);

    assert_eq!(pieces(&e, "Joist"), 13);
    assert_eq!(pieces(&e, "Ledger"), 1);
    assert_eq!(pieces(&e, "Rim joist"), 1);
    assert_eq!(pieces(&e, "Post"), 3);
    assert_eq!(qty(&e, "Joist hanger"), 13);
    assert_eq!(qty(&e, "Hurricane tie"), 13);
    assert_eq!(qty(&e, "ledger screw"), 13); // 192" / 16" + 1

    // 144.75" coverage / 5.625" pitch = 26 rows, + 12 tread boards, + 5% spares.
    assert_eq!(pieces(&e, "Deck board"), 26);
    assert_eq!(pieces(&e, "Stair tread"), 12);

    let st = &e.stairs[0];
    assert_eq!(st.risers, 7);
    assert_eq!(st.treads, 6);
    assert_eq!(st.stringers, 4);
    assert!(st.handrails);

    // Guards: west 12', east 12', south 16' minus the 4' stair; plus two stair rails.
    // Endpoints: 6 corners/ends + 2 intermediates on the 12' sides + 2 per stair rail.
    assert_eq!(pieces(&e, "Railing post"), 12);

    assert!(
        e.tips.iter().any(|t| t.contains("16' boards")),
        "{:?}",
        e.tips
    );
}

#[test]
fn freestanding_composite_with_panels() {
    let e = estimate(&load("freestanding_20x14_composite")).unwrap();
    let s = &e.sections[0];
    assert_eq!(s.joist_spacing, 16.0);
    assert_eq!(s.beams.len(), 2);
    assert!(e.sections[0].plan.ledger.is_none());
    assert_eq!(qty(&e, "Joist hanger"), 0);
    assert!(qty(&e, "Hidden deck fastener clips") >= 4);
    assert!(qty(&e, "Railing kit") > 0);
    assert_eq!(
        qty(&e, "Post sleeve for 4x4"),
        pieces(&e, "Railing post") as u32
    );
    assert!(pieces(&e, "Picture-frame border") > 0);
    assert!(pieces(&e, "Fascia") > 0);
    // Composite stairs use 12" OC stringers.
    assert_eq!(e.stairs[0].stringers, 5);
}

#[test]
fn l_shape_two_levels() {
    let e = estimate(&load("l_shape_two_level")).unwrap();
    assert_eq!(e.sections.len(), 2);
    // 24" step between levels: no guard needed where they meet, so the upper south edge
    // only needs a guard on the 6' not over the lower deck.
    let upper = &e.sections[0];
    let lower = &e.sections[1];
    assert!(upper.plan.ledger.is_some());
    assert!(lower.plan.ledger.is_none());
    assert_eq!(lower.beams.len(), 2);
    assert!(e
        .line_items
        .iter()
        .any(|l| l.category == Category::Concrete));
}

#[test]
fn deterministic_and_serializable() {
    for f in [
        "attached_12x16_pt",
        "freestanding_20x14_composite",
        "l_shape_two_level",
    ] {
        let p = load(f);
        let a = estimate(&p).unwrap();
        let b = estimate(&p).unwrap();
        assert_eq!(a, b, "{f}");
        let json = serde_json::to_string(&a).unwrap();
        let back: Estimate = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }
}

#[test]
fn every_cut_list_is_physically_valid() {
    for f in [
        "attached_12x16_pt",
        "freestanding_20x14_composite",
        "l_shape_two_level",
    ] {
        let e = estimate(&load(f)).unwrap();
        for c in &e.cut_lists {
            for b in &c.boards {
                let used: f64 = b.cuts.iter().map(|x| x.length).sum();
                assert!(
                    used <= b.stock_length + 1e-6,
                    "{f}: {} overfilled",
                    c.material
                );
                assert!(b.waste >= 0.0);
            }
        }
    }
}

#[test]
fn validation_errors_are_reported() {
    let mut p = load("attached_12x16_pt");
    p.sections[0].ledger_edge = Some(estimator::Edge::East);
    let err = estimate(&p).unwrap_err();
    assert_eq!(err.code, "ledger_parallel_to_joists");

    let mut p = load("l_shape_two_level");
    p.sections[1].y = 100.0;
    assert_eq!(estimate(&p).unwrap_err().code, "overlap");

    let mut p = load("attached_12x16_pt");
    p.stairs[0].offset = 170.0;
    assert_eq!(estimate(&p).unwrap_err().code, "stair_fit");
}

#[test]
fn snow_load_upsizes_framing() {
    let base = estimate(&load("attached_12x16_pt")).unwrap();
    let mut p = load("attached_12x16_pt");
    p.site.live_load_psf = 70.0;
    let snow = estimate(&p).unwrap();
    assert!(snow.sections[0].joist_size > base.sections[0].joist_size);
    assert!(snow.warnings.iter().any(|w| w.contains("psf")));
}

#[test]
fn decking_runs_across_the_joists() {
    let e = estimate(&load("attached_12x16_pt")).unwrap();
    assert_eq!(e.sections[0].decking_along, Axis::EastWest);

    // Boards toward the house need a freestanding section.
    let mut p = load("attached_12x16_pt");
    p.sections[0].joists_along = Axis::EastWest;
    let err = estimate(&p).unwrap_err();
    assert_eq!(err.code, "ledger_parallel_to_joists");
    assert!(err.message.contains("freestanding"), "{}", err.message);
    p.sections[0].ledger_edge = None;
    p.stairs[0].edge = Edge::South;
    let e = estimate(&p).unwrap();
    assert_eq!(e.sections[0].decking_along, Axis::NorthSouth);
}

#[test]
fn four_by_four_posts_where_allowed() {
    let mut p = load("attached_12x16_pt");
    p.materials.post_size = PostSize::FourBy4;
    let e = estimate(&p).unwrap();
    let s = &e.sections[0];
    // Same 2-2x10 beam: 2 plies (3") sit on a 3-1/2" post. 48 - 1 - 7.25 - 9.25 - 3 = 27.5".
    assert_eq!(s.beams[0].size, "2-2x10");
    assert_eq!(s.beams[0].post_size, Some(PostSize::FourBy4));
    assert_eq!(s.post_length, Some(27.5));
    assert_eq!(qty(&e, "4x4 PT post (ground contact)"), 1); // three 27.5" posts from one 8'
    assert_eq!(qty(&e, "Post base, 4x4"), 3);
    assert_eq!(qty(&e, "Post-beam cap, 4x4 to 2-ply"), 3);
    assert_eq!(qty(&e, "6x6"), 0);
    assert!(
        !e.warnings.iter().any(|w| w.contains("4x4")),
        "{:?}",
        e.warnings
    );
}

#[test]
fn tall_four_by_four_posts_are_upsized() {
    let mut p = load("attached_12x16_pt");
    p.materials.post_size = PostSize::FourBy4;
    p.sections[0].height = 120.0;
    let e = estimate(&p).unwrap();
    // 120 - 1 - 7.25 - 9.25 - 3 = 99.5" posts, over the 96" 4x4 limit.
    assert_eq!(e.sections[0].post_length, Some(99.5));
    assert_eq!(e.sections[0].beams[0].post_size, Some(PostSize::SixBy6));
    assert_eq!(qty(&e, "Post base, 6x6"), 3);
    assert_eq!(qty(&e, "4x4 PT post (ground contact)"), 0);
    assert!(
        e.warnings
            .iter()
            .any(|w| w.contains("4x4 posts are limited")),
        "{:?}",
        e.warnings
    );
}

#[test]
fn four_by_four_posts_take_narrower_beams() {
    let mut p = load("freestanding_20x14_composite");
    p.sections[0].height = 60.0;
    let six = estimate(&p).unwrap();
    assert_eq!(six.sections[0].beams[0].size, "3-2x10");
    assert_eq!(six.sections[0].beams[0].posts, 3);

    p.materials.post_size = PostSize::FourBy4;
    let four = estimate(&p).unwrap();
    let b = &four.sections[0].beams[0];
    assert_eq!(b.plies, 2);
    assert_eq!(b.posts, 4);
    assert_eq!(b.post_size, Some(PostSize::FourBy4));
    assert_eq!(qty(&four, "Post base, 4x4"), 8);

    // A low deck has no posts, so the post size doesn't change the beams.
    let mut low = load("freestanding_20x14_composite");
    low.materials.post_size = PostSize::FourBy4;
    let e = estimate(&low).unwrap();
    assert_eq!(e.sections[0].beams[0].size, "3-2x10");
    assert_eq!(e.sections[0].beams[0].post_size, None);
}
