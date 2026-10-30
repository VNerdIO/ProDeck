//! Print an estimate for a project JSON file: `cargo run --example dump -- path.json`.

use estimator::{estimate, fmt_ft_in, Project};

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump <project.json>");
    let p: Project = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    match estimate(&p) {
        Err(e) => eprintln!("error: {e}"),
        Ok(e) => {
            for s in &e.sections {
                println!("== {}", s.name);
                for r in &s.rationale {
                    println!("   {r}");
                }
            }
            for s in &e.stairs {
                for r in &s.rationale {
                    println!("   stair: {r}");
                }
            }
            println!();
            for li in &e.line_items {
                println!(
                    "{:<10} {:>4} {:<5} {:<55} {:<5} {}",
                    li.category.label(),
                    li.quantity,
                    li.unit,
                    li.description,
                    li.size.clone().unwrap_or_default(),
                    li.notes.clone().unwrap_or_default()
                );
            }
            println!();
            for c in &e.cut_lists {
                println!(
                    "-- {} ({} boards, waste {})",
                    c.material,
                    c.boards.len(),
                    fmt_ft_in(c.total_waste)
                );
                for b in c.boards.iter().take(4) {
                    let cuts: Vec<String> = b
                        .cuts
                        .iter()
                        .map(|x| format!("{} {}", x.label, fmt_ft_in(x.length)))
                        .collect();
                    println!(
                        "   {} -> {} | waste {}",
                        fmt_ft_in(b.stock_length),
                        cuts.join(", "),
                        fmt_ft_in(b.waste)
                    );
                }
            }
            println!("\ntips: {:#?}", e.tips);
            println!("\nwarnings: {:#?}\nsummary: {:?}", e.warnings, e.summary);
        }
    }
}
