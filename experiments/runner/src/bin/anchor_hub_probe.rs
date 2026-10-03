//! Anchor-hub probe runner.
//!
//! Prints the design audit, then the crossed design at a sweep of
//! corpus sizes under both the shipped routing and the anchor-routed
//! negative control. See `anchor_hub_probe` for the design statement.

use koru_experiments_runner::anchor_hub_probe::{
    ByteCorpus, CrossedDesign, Routing, MAX_FACTOR_LEVELS,
};

/// Factor levels per factor. 6 × 6 = 36 cells per (size, routing).
const K: usize = 6;

/// Zipf exponent — the same family exp18 pins, moved from chain index
/// to byte value.
const ALPHA: f64 = 1.0;

/// Corpus RNG seed. Distinct from exp18's `0xC0DE` so the two corpora
/// cannot be confused for one another.
const CORPUS_SEED: u64 = 0xA17C_0DE5;

/// Payload sizes swept, in bytes. Intermediate steps between 1 KiB and
/// 4 KiB are there to locate the crossover the first run showed sitting
/// somewhere in that interval.
const SIZES: [usize; 9] = [64, 256, 1024, 1536, 2048, 3072, 4096, 16_384, 65_536];

fn main() {
    println!("anchor-hub probe — does the lift's hub belong to the anchor or the alphabet?");
    println!("k = {K} (max {MAX_FACTOR_LEVELS}), alpha = {ALPHA}, corpus seed = {CORPUS_SEED:#x}");
    println!();

    println!("== design audit ==");
    let failures = CrossedDesign::audit(K);
    if failures.is_empty() {
        println!("  factors crossed, factor values pairwise distinct, no anchor is an");
        println!("  alphabet entry, and cell (0,0) reproduces the shipped encoding");
        println!("  (byte tips, anchor vector D, published fold vector, ByteFold::fold).");
    } else {
        for f in &failures {
            println!("  FAIL: {f}");
        }
        println!("design is unsound; measurements below are not interpretable");
    }
    println!();

    for routing in [Routing::Shipped, Routing::AnchorRouted] {
        println!("================================================================");
        println!("routing: {}", routing.label());
        println!("================================================================");
        for size in SIZES {
            let corpus = ByteCorpus::zipf(ALPHA, CORPUS_SEED, size);
            let design = CrossedDesign::run(&corpus, K, routing);
            let attr = design.attribution();
            let shipped_cell = design.cell(0, 0);

            println!();
            println!(
                "-- payload {} B, {} records, {} distinct first steps --",
                corpus.payload_bytes,
                corpus.record_count(),
                corpus.distinct_first_steps()
            );
            println!("   verdict (top-node identity): {}", attr.verdict.label());
            println!("   verdict (top-node role):     {}", attr.role_verdict.label());
            println!(
                "   agree(top-degree identity | vary anchor, alphabet fixed) = {:.3}",
                attr.agree_across_seeds
            );
            println!(
                "   agree(top-degree identity | vary alphabet, anchor fixed) = {:.3}",
                attr.agree_across_alphabets
            );
            println!(
                "   mean rho(per-symbol degree rank): vary anchor = {:.4}, vary alphabet = {:.4}",
                attr.rho_symbol_rank_across_seeds, attr.rho_symbol_rank_across_alphabets
            );

            // Role census over the whole grid, both readings.
            let mut total_roles = std::collections::BTreeMap::new();
            let mut delta_roles = std::collections::BTreeMap::new();
            for cell in &design.cells {
                *total_roles.entry(cell.top_role.label()).or_insert(0usize) += 1;
                *delta_roles.entry(cell.top_role_fold_delta.label()).or_insert(0usize) += 1;
            }
            println!("   top-degree role census (total degree):   {total_roles:?}");
            println!("   top-degree role census (fold delta only): {delta_roles:?}");
            println!(
                "   primordials gained nothing from the fold (all cells): {}",
                design.primordials_gained_nothing_from_the_fold()
            );
            if routing == Routing::Shipped {
                println!(
                    "   anchor fold-delta == distinct first steps (all cells): {}",
                    design.anchor_delta_equals_distinct_first_steps(&corpus)
                );
            }

            println!("   cell (0,0) = shipped encoding: {} nodes", shipped_cell.node_count);
            println!(
                "     d0  total {:>7}  fold-delta {:>7}  rank {}",
                shipped_cell.d0.total, shipped_cell.d0.fold_delta, shipped_cell.d0_rank
            );
            println!(
                "     d1  total {:>7}  fold-delta {:>7}",
                shipped_cell.d1.total, shipped_cell.d1.fold_delta
            );
            println!(
                "     anchor          total {:>7}  fold-delta {:>7}  rank {}",
                shipped_cell.anchor.total, shipped_cell.anchor.fold_delta, shipped_cell.anchor_rank
            );
            println!(
                "     max alphabet    total {:>7}  fold-delta {:>7}  best rank {}",
                shipped_cell.alphabet_max.total,
                shipped_cell.alphabet_max.fold_delta,
                shipped_cell.best_alphabet_rank
            );
            println!(
                "     rho(alphabet degree, symbol freq) = {:.4}   \
                 rho(degree, participations) = {:.4}   formula exact: {}",
                shipped_cell.rho_alphabet_vs_symbol_freq,
                shipped_cell.rho_degree_vs_participations,
                shipped_cell.degree_formula_exact
            );
            println!("     top {} by total degree:", shipped_cell.top_nodes.len());
            for node in &shipped_cell.top_nodes {
                println!(
                    "       {}  degree {:>7}  {}",
                    node.distinction.to_hex(),
                    node.degree,
                    node.role.label()
                );
            }
        }
        println!();
    }
}
