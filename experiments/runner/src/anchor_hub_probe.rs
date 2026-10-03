//! Anchor-hub probe — does the lift's hub position belong to the
//! **anchor** or to the **alphabet**?
//!
//! # The claim under test
//!
//! The Fold Law (`THEORY.md`, Law 11) says the lift concentrates degree
//! wherever it routes, and names d₀/d₁ as the resulting topological
//! hubs. `THEORY.md` already flags the *magnitude* as an encoding
//! property. What it still asserts as a law is the *shape*: that the
//! thing the fold anchors on becomes the hub. If instead the hub
//! position is held by the fold's **alphabet**, and the primordials
//! only rank highly because constructing an alphabet routes through
//! them, then the shape is a consequence of the encoding's choice of
//! alphabet and Law 11 is a corollary of Law 12 (degree = count of
//! novel participations) applied to the lift's routing — not an
//! independent law.
//!
//! # Why the shipped lift admits a crossed design
//!
//! The shipped sequence lift ([`koru_lambda_core::ByteFold::fold`]) has
//! exactly two free parameters, and they are structurally independent:
//!
//! - the **anchor** — the initial accumulator, one distinction;
//! - the **alphabet** — the map symbol → distinction, 256 entries.
//!
//! `fold` requires only that the alphabet entries be registered in the
//! engine and that the anchor not be one of them. It does not require
//! either to be derived from the other. The shipped code *happens* to
//! derive both from {d₀, d₁}, and
//! [`koru_lambda_core::domain_anchor`] additionally derives the anchor
//! *from an alphabet tip* — so in the shipped configuration the two
//! factors covary and no measurement on it alone can separate them.
//! This probe breaks the covariance by construction.
//!
//! # Design (stated before running)
//!
//! **Factor S (anchor / "seed"), `k` levels.** `anchor(s)` is a pure-d₀
//! chain of length `s + 1` hanging off `base = synthesize(d₀, d₁)`.
//! `anchor(0) == vectors::D_HEX`, the anchor the shipped test vectors
//! use. Depends on `s` only; no alphabet entry appears in its
//! derivation.
//!
//! **Factor A (alphabet), `k` levels.** `alphabet(a)[b]` is the shipped
//! 8-bit-wide bit chain for byte `b` (one `synthesize` per bit, folding
//! the accumulator with the primordial that bit selects) started from
//! `root(a)`, a pure-d₁ chain of length `a` off `base`. `alphabet(0)`
//! is *byte-identical to the shipped byte tips* — `root(0) == base` is
//! exactly [`koru_lambda_core::ByteMapping::map_byte_to_distinction`]'s
//! starting accumulator. Depends on `a` only; no anchor appears in its
//! derivation.
//!
//! The two namespaces are a pure-d₀ chain and a pure-d₁ chain off the
//! same base, so they diverge at the first step and `k ≤ 7` keeps both
//! shallower than the 8-step bit chains (so no anchor is ever an
//! alphabet entry). Disjointness is **asserted at runtime**
//! ([`CrossedDesign::audit`]), not assumed.
//!
//! **Cell.** Each of the `k × k` cells gets a fresh engine, builds its
//! own `(anchor(s), alphabet(a))`, and folds the *identical* byte
//! corpus. Degrees are snapshotted before and after the corpus fold, so
//! every quantity is reported twice: `total` (includes alphabet/anchor
//! setup) and `fold_delta` (the corpus fold alone). `fold_delta` is the
//! measurement that answers "what does folding data concentrate?"; the
//! `total` is what a consumer's engine actually contains.
//!
//! **Fidelity.** Cell `(0, 0)` is asserted to reproduce the shipped
//! encoding exactly: the alphabet equals `ByteFold`'s tips, the anchor
//! equals `vectors::D_HEX`, and [`Lift::fold`] agrees with
//! `ByteFold::fold` byte-for-byte on the published vector. Without that
//! the probe would be measuring a re-implementation.
//!
//! **Verdict statistic.** For each cell, record the identity of the
//! top-degree node. Then
//!
//! - `agree_across_seeds` = fraction of cell pairs that differ *only*
//!   in `s` whose top-degree node is the same distinction;
//! - `agree_across_alphabets` = the same for pairs differing only in `a`.
//!
//! Three outcomes are live, and the statistic distinguishes them
//! without privileging any:
//!
//! | `agree_across_seeds` | `agree_across_alphabets` | reading |
//! |---|---|---|
//! | 0 | 1 | hub tracks the **anchor** — Law 11 as stated |
//! | 1 | 0 | hub tracks the **alphabet** — Law 11 is a corollary |
//! | 1 | 1 | hub is **invariant to both** (the primordials) |
//! | 0 | 0 | hub tracks neither cleanly — inconclusive |
//!
//! The same statistic is also computed over the top node's *role*
//! rather than its identity ([`Attribution::role_verdict`]), because
//! identity is degenerate when nodes tie on degree: d₀ and d₁ sit
//! within a few counts of one another and which wins is settled by
//! setup bookkeeping. The role form is not a weaker test — it still
//! returns `TracksAnchor` whenever the anchor tops every cell — it is
//! blind to that one tie.
//!
//! **Corpus-size sweep.** Run the whole crossed design at several
//! corpus sizes. An anchor touched only at the fold's first step can
//! take at most one novel bump per distinct first step, so its degree
//! is bounded by the alphabet size however much data is folded, while
//! an alphabet entry takes a bump per distinct context it appears in.
//! If a crossover exists it is a function of corpus size, which is a
//! workload fact, not an axiom.
//!
//! # Can this probe fail?
//!
//! [`Routing::AnchorRouted`] is the negative control: the same design,
//! same corpus, same factors, but the fold routes *through the anchor
//! at every element step*. That encoding genuinely makes the anchor a
//! hub, so the probe must return the anchor-tracking verdict on it. If
//! both routings returned the same verdict, the statistic would be
//! measuring something other than what it claims.

use koru_lambda_core::{Distinction, DistinctionEngine};
use rand::distr::weighted::WeightedIndex;
use rand::distr::Distribution;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::{HashMap, HashSet};

use crate::coding_law_workload::spearman_rho;

/// Symbols in a byte alphabet.
pub const ALPHABET_SIZE: usize = 256;

/// Bit-steps per byte in the shipped byte lift.
const BITS_PER_BYTE: usize = 8;

/// Highest usable factor level: both factor namespaces are chains off
/// `base` and must stay shallower than the 8-step bit chains that build
/// an alphabet entry, or a factor value could land on one.
pub const MAX_FACTOR_LEVELS: usize = 7;

// ---------------------------------------------------------------------
// Factors
// ---------------------------------------------------------------------

/// `base` — the accumulator the shipped byte lift starts from.
fn base(engine: &DistinctionEngine) -> Distinction {
    engine.synthesize(engine.d0(), engine.d1())
}

/// The anchor for factor level `s`: a pure-d₀ chain of length `s + 1`
/// off `base`.
///
/// `anchor_for_seed(e, 0)` is `vectors::D_HEX`, the anchor the shipped
/// fold vectors publish.
#[must_use]
pub fn anchor_for_seed(engine: &DistinctionEngine, s: usize) -> Distinction {
    let mut acc = base(engine);
    for _ in 0..=s {
        acc = engine.synthesize(acc, engine.d0());
    }
    acc
}

/// One 8-bit-wide bit chain: the shipped byte lift's inner loop, with
/// the starting accumulator left free.
fn bit_chain(engine: &DistinctionEngine, start: Distinction, byte: u8) -> Distinction {
    let d0 = engine.d0();
    let d1 = engine.d1();
    let mut acc = start;
    for bit_index in 0..BITS_PER_BYTE {
        acc = engine.synthesize(acc, if (byte >> bit_index) & 1 == 0 { d0 } else { d1 });
    }
    acc
}

/// The alphabet for factor level `a`: shipped bit chains started from a
/// pure-d₁ chain of length `a` off `base`.
///
/// `alphabet_for_level(e, 0)` is byte-identical to the shipped byte
/// tips.
#[must_use]
pub fn alphabet_for_level(
    engine: &DistinctionEngine,
    a: usize,
) -> Box<[Distinction; ALPHABET_SIZE]> {
    let mut root = base(engine);
    for _ in 0..a {
        root = engine.synthesize(root, engine.d1());
    }
    Box::new(std::array::from_fn(|b| bit_chain(engine, root, u8::try_from(b).unwrap_or(0))))
}

// ---------------------------------------------------------------------
// The lift under test
// ---------------------------------------------------------------------

/// Which fold routing a cell uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Routing {
    /// The shipped routing: the anchor is the initial accumulator and
    /// is touched once, at the first step. Reproduces
    /// `ByteFold::fold` exactly when the alphabet is the shipped tips.
    Shipped,
    /// Negative control: every element step is followed by a fold
    /// through the anchor, so the anchor participates once per symbol.
    AnchorRouted,
}

impl Routing {
    /// Short label for reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Shipped => "shipped",
            Self::AnchorRouted => "anchor-routed",
        }
    }
}

/// An (anchor, alphabet, routing) triple — one point in the design.
pub struct Lift {
    anchor: Distinction,
    alphabet: Box<[Distinction; ALPHABET_SIZE]>,
    routing: Routing,
}

impl Lift {
    /// Build the lift for factor levels `(s, a)` against `engine`.
    #[must_use]
    pub fn build(engine: &DistinctionEngine, s: usize, a: usize, routing: Routing) -> Self {
        Self {
            anchor: anchor_for_seed(engine, s),
            alphabet: alphabet_for_level(engine, a),
            routing,
        }
    }

    /// The anchor this lift folds from.
    #[must_use]
    pub const fn anchor(&self) -> Distinction {
        self.anchor
    }

    /// The alphabet entry for one symbol.
    #[must_use]
    pub const fn symbol(&self, byte: u8) -> Distinction {
        self.alphabet[byte as usize]
    }

    /// Fold one record, anchored at this lift's anchor.
    ///
    /// Under [`Routing::Shipped`] this is `ByteFold::fold` with the
    /// alphabet left free: length first (so sequences of different
    /// lengths cannot coincide), then the elements, order carried by
    /// nesting depth.
    #[must_use]
    pub fn fold(&self, bytes: &[u8], engine: &DistinctionEngine) -> Distinction {
        let mut acc = self.anchor;
        let len_bytes = (bytes.len() as u64).to_le_bytes();
        for len_byte in len_bytes {
            acc = self.step(acc, len_byte, engine);
        }
        for &b in bytes {
            acc = self.step(acc, b, engine);
        }
        acc
    }

    fn step(&self, acc: Distinction, byte: u8, engine: &DistinctionEngine) -> Distinction {
        let next = engine.synthesize(acc, self.symbol(byte));
        match self.routing {
            Routing::Shipped => next,
            Routing::AnchorRouted => engine.synthesize(next, self.anchor),
        }
    }
}

// ---------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------

/// A byte corpus: records to fold, plus the ground-truth symbol
/// frequency the Coding Law comparison needs.
pub struct ByteCorpus {
    /// The records, in order.
    pub records: Vec<Vec<u8>>,
    /// `symbol_freq[b]` — occurrences of byte `b` across all records,
    /// counting the length prefixes, because those are folded too.
    pub symbol_freq: Vec<u64>,
    /// Total payload bytes (excludes length prefixes).
    pub payload_bytes: usize,
}

impl ByteCorpus {
    /// Zipf(`alpha`)-sampled bytes split into records of uniformly
    /// random length in `1..=255`.
    ///
    /// Record lengths span the whole low byte deliberately: the anchor
    /// is touched at the first step, whose symbol is the length's low
    /// byte, so spanning `1..=255` maximises the number of distinct
    /// first steps and is therefore the *most generous* record shape
    /// available to the anchor-as-hub reading.
    ///
    /// Reuses the Coding Law workload's sampler shape — `WeightedIndex`
    /// over `1/(k+1)^alpha` driven by a seeded `StdRng` — so the symbol
    /// distribution is the same family exp18 pins, moved from chain
    /// index to byte value.
    ///
    /// # Panics
    ///
    /// Panics if `WeightedIndex` rejects the weights, which cannot
    /// happen for finite positive `alpha`.
    #[must_use]
    pub fn zipf(alpha: f64, seed: u64, payload_bytes: usize) -> Self {
        let weights: Vec<f64> =
            (0..ALPHABET_SIZE).map(|k| 1.0 / ((k + 1) as f64).powf(alpha)).collect();
        let dist = WeightedIndex::new(&weights).expect("zipf weights valid (alpha > 0)");
        let mut rng = StdRng::seed_from_u64(seed);

        let mut records = Vec::new();
        let mut symbol_freq = vec![0u64; ALPHABET_SIZE];
        let mut emitted = 0usize;
        while emitted < payload_bytes {
            let want = rng.random_range(1usize..=255);
            let len = want.min(payload_bytes - emitted);
            let mut record = Vec::with_capacity(len);
            for _ in 0..len {
                let b = dist.sample(&mut rng);
                symbol_freq[b] += 1;
                record.push(u8::try_from(b).unwrap_or(0));
            }
            for len_byte in (len as u64).to_le_bytes() {
                symbol_freq[len_byte as usize] += 1;
            }
            emitted += len;
            records.push(record);
        }
        Self { records, symbol_freq, payload_bytes }
    }

    /// Number of fold calls this corpus drives.
    #[must_use]
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// Distinct low-bytes of record lengths — the structural ceiling on
    /// how many distinct first steps the anchor can take under
    /// [`Routing::Shipped`].
    #[must_use]
    pub fn distinct_first_steps(&self) -> usize {
        self.records
            .iter()
            .map(|r| (r.len() as u64).to_le_bytes()[0])
            .collect::<HashSet<u8>>()
            .len()
    }
}

// ---------------------------------------------------------------------
// Per-cell measurement
// ---------------------------------------------------------------------

/// What structural part of the lift a node belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Role {
    /// d₀ or d₁.
    Primordial,
    /// This cell's anchor.
    Anchor,
    /// An entry of this cell's alphabet.
    AlphabetElement,
    /// Anything else: accumulator chain nodes, alphabet-construction
    /// intermediates, fold tips.
    Interior,
}

impl Role {
    /// Short label for reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Primordial => "primordial",
            Self::Anchor => "anchor",
            Self::AlphabetElement => "alphabet",
            Self::Interior => "interior",
        }
    }
}

/// A degree reading taken twice: over the whole engine, and over the
/// corpus fold alone.
#[derive(Clone, Copy, Debug)]
pub struct Degrees {
    /// Degree in the finished engine, setup included.
    pub total: usize,
    /// Degree gained during the corpus fold only.
    pub fold_delta: usize,
}

/// One cell of the crossed design.
pub struct CellReport {
    /// Anchor factor level.
    pub seed_level: usize,
    /// Alphabet factor level.
    pub alphabet_level: usize,
    /// Routing used.
    pub routing: Routing,
    /// Distinctions in the finished engine.
    pub node_count: usize,
    /// d₀ degree.
    pub d0: Degrees,
    /// d₁ degree.
    pub d1: Degrees,
    /// Anchor degree.
    pub anchor: Degrees,
    /// Highest alphabet-entry degree.
    pub alphabet_max: Degrees,
    /// 1-based rank of the anchor among all nodes by total degree.
    pub anchor_rank: usize,
    /// 1-based rank of d₀ among all nodes by total degree.
    pub d0_rank: usize,
    /// Best (lowest) 1-based rank held by an alphabet entry.
    pub best_alphabet_rank: usize,
    /// The top-degree node by total degree; ties broken by identity
    /// bytes so the statistic is deterministic.
    pub top_identity: Distinction,
    /// Role of [`Self::top_identity`].
    pub top_role: Role,
    /// Top-degree node by fold-delta degree.
    pub top_identity_fold_delta: Distinction,
    /// Role of [`Self::top_identity_fold_delta`].
    pub top_role_fold_delta: Role,
    /// Spearman ρ between alphabet-entry total degree and corpus
    /// symbol frequency, over the 256 symbols — Law 12 inside the
    /// alphabet.
    pub rho_alphabet_vs_symbol_freq: f64,
    /// Spearman ρ between engine-reported degree and the novel-
    /// participation count recomputed from recorded parentage, over
    /// every node.
    pub rho_degree_vs_participations: f64,
    /// Whether `degree == participations + addend` held exactly for
    /// every node.
    pub degree_formula_exact: bool,
    /// Total-degree rank of each symbol's alphabet entry, for
    /// cross-cell rank comparison.
    pub symbol_degree: Vec<usize>,
    /// Top [`TOP_N`] nodes by total degree.
    pub top_nodes: Vec<RankedNode>,
}

/// One entry of a cell's top-degree table.
#[derive(Clone, Copy, Debug)]
pub struct RankedNode {
    /// The node.
    pub distinction: Distinction,
    /// Its total degree.
    pub degree: usize,
    /// Which part of the lift it belongs to.
    pub role: Role,
}

/// How many top nodes a cell report keeps.
pub const TOP_N: usize = 6;

/// Run one cell: fresh engine, build the lift, fold the corpus,
/// measure.
#[must_use]
pub fn run_cell(corpus: &ByteCorpus, s: usize, a: usize, routing: Routing) -> CellReport {
    let engine = DistinctionEngine::new();
    let lift = Lift::build(&engine, s, a, routing);

    // Snapshot every registered node's degree before the corpus fold,
    // so the fold-delta reading excludes anchor/alphabet construction.
    let before: HashMap<[u8; 16], usize> = engine
        .snapshot_distinctions()
        .into_iter()
        .map(|d| (*d.as_bytes(), engine.degree(d)))
        .collect();

    for record in &corpus.records {
        let _ = lift.fold(record, &engine);
    }

    let d0 = engine.d0();
    let d1 = engine.d1();
    let anchor = lift.anchor();
    let alphabet_set: HashSet<[u8; 16]> = (0..=255u8).map(|b| *lift.symbol(b).as_bytes()).collect();

    let nodes = engine.snapshot_distinctions();
    let delta_of = |d: Distinction| -> usize {
        engine.degree(d) - before.get(d.as_bytes()).copied().unwrap_or(0)
    };
    let read = |d: Distinction| Degrees { total: engine.degree(d), fold_delta: delta_of(d) };
    let role_of = |d: Distinction| -> Role {
        if d == d0 || d == d1 {
            Role::Primordial
        } else if d == anchor {
            Role::Anchor
        } else if alphabet_set.contains(d.as_bytes()) {
            Role::AlphabetElement
        } else {
            Role::Interior
        }
    };

    // Rank every node by total degree, descending; ties broken by
    // identity bytes so the ordering is deterministic.
    let mut by_total: Vec<(Distinction, usize)> =
        nodes.iter().map(|d| (*d, engine.degree(*d))).collect();
    by_total.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.as_bytes().cmp(y.0.as_bytes())));

    let rank_of = |target: Distinction| -> usize {
        by_total.iter().position(|(d, _)| *d == target).map_or(usize::MAX, |i| i + 1)
    };

    let mut by_delta: Vec<(Distinction, usize)> =
        nodes.iter().map(|d| (*d, delta_of(*d))).collect();
    by_delta.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.as_bytes().cmp(y.0.as_bytes())));

    let alphabet_total: Vec<usize> = (0..=255u8).map(|b| engine.degree(lift.symbol(b))).collect();
    let alphabet_delta_max = (0..=255u8).map(|b| delta_of(lift.symbol(b))).max().unwrap_or(0);
    let alphabet_total_max = alphabet_total.iter().copied().max().unwrap_or(0);

    let best_alphabet_rank = by_total
        .iter()
        .position(|(d, _)| alphabet_set.contains(d.as_bytes()))
        .map_or(usize::MAX, |i| i + 1);

    // Law 12's structural part, recomputed independently: a node's
    // novel-participation count is the number of registered children
    // that list it as a parent.
    let mut participations: HashMap<[u8; 16], usize> = HashMap::new();
    for (_, pair) in engine.snapshot_parentage() {
        *participations.entry(*pair.0.as_bytes()).or_default() += 1;
        *participations.entry(*pair.1.as_bytes()).or_default() += 1;
    }
    let mut degrees_vec = Vec::with_capacity(nodes.len());
    let mut parts_vec = Vec::with_capacity(nodes.len());
    let mut exact = true;
    for d in &nodes {
        let deg = engine.degree(*d);
        let part = participations.get(d.as_bytes()).copied().unwrap_or(0);
        let addend = if *d == d0 || *d == d1 { 1 } else { 2 };
        if deg != part + addend {
            exact = false;
        }
        degrees_vec.push(deg);
        parts_vec.push(part);
    }

    let (top_identity, _) = by_total[0];
    let (top_delta_identity, _) = by_delta[0];

    CellReport {
        seed_level: s,
        alphabet_level: a,
        routing,
        node_count: engine.distinction_count(),
        d0: read(d0),
        d1: read(d1),
        anchor: read(anchor),
        alphabet_max: Degrees { total: alphabet_total_max, fold_delta: alphabet_delta_max },
        anchor_rank: rank_of(anchor),
        d0_rank: rank_of(d0),
        best_alphabet_rank,
        top_identity,
        top_role: role_of(top_identity),
        top_identity_fold_delta: top_delta_identity,
        top_role_fold_delta: role_of(top_delta_identity),
        rho_alphabet_vs_symbol_freq: spearman_rho(&alphabet_total, &corpus.symbol_freq),
        rho_degree_vs_participations: spearman_rho(&degrees_vec, &parts_vec),
        degree_formula_exact: exact,
        symbol_degree: alphabet_total,
        top_nodes: by_total
            .iter()
            .take(TOP_N)
            .map(|(d, deg)| RankedNode { distinction: *d, degree: *deg, role: role_of(*d) })
            .collect(),
    }
}

// ---------------------------------------------------------------------
// Crossed design
// ---------------------------------------------------------------------

/// The `k × k` crossed design at one corpus size and routing.
pub struct CrossedDesign {
    /// Factor levels per factor.
    pub k: usize,
    /// Routing under test.
    pub routing: Routing,
    /// Cells, row-major in `(s, a)`.
    pub cells: Vec<CellReport>,
}

/// Which factor the hub position follows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// Changing the anchor moves the hub; changing the alphabet does
    /// not. Law 11 as stated.
    TracksAnchor,
    /// Changing the alphabet moves the hub; changing the anchor does
    /// not. Law 11 is a corollary of the encoding's alphabet choice.
    TracksAlphabet,
    /// Neither factor moves the hub.
    InvariantToBoth,
    /// Both factors move it, or the pattern is mixed.
    Inconclusive,
}

impl Verdict {
    /// Short label for reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::TracksAnchor => "hub tracks the ANCHOR",
            Self::TracksAlphabet => "hub tracks the ALPHABET",
            Self::InvariantToBoth => "hub is INVARIANT to both factors",
            Self::Inconclusive => "INCONCLUSIVE",
        }
    }
}

/// Agreement fractions and the verdict they imply.
#[derive(Clone, Copy, Debug)]
pub struct Attribution {
    /// Fraction of cell pairs differing only in the anchor level whose
    /// top-degree node is the same distinction.
    pub agree_across_seeds: f64,
    /// Fraction of cell pairs differing only in the alphabet level
    /// whose top-degree node is the same distinction.
    pub agree_across_alphabets: f64,
    /// Mean Spearman ρ between per-symbol degree-rank vectors of cells
    /// differing only in the anchor level.
    pub rho_symbol_rank_across_seeds: f64,
    /// Mean Spearman ρ between per-symbol degree-rank vectors of cells
    /// differing only in the alphabet level.
    pub rho_symbol_rank_across_alphabets: f64,
    /// Verdict implied by the two agreement fractions.
    pub verdict: Verdict,
    /// The same verdict computed over the top node's *role*, which is
    /// immune to the d₀/d₁ degree tie that makes the identity
    /// statistic degenerate at small corpus sizes.
    pub role_verdict: Verdict,
}

impl CrossedDesign {
    /// Run the design.
    #[must_use]
    pub fn run(corpus: &ByteCorpus, k: usize, routing: Routing) -> Self {
        assert!(
            (2..=MAX_FACTOR_LEVELS).contains(&k),
            "k must be in 2..={MAX_FACTOR_LEVELS} to keep the factor namespaces disjoint"
        );
        let mut cells = Vec::with_capacity(k * k);
        for s in 0..k {
            for a in 0..k {
                cells.push(run_cell(corpus, s, a, routing));
            }
        }
        Self { k, routing, cells }
    }

    /// Cell at factor levels `(s, a)`.
    #[must_use]
    pub fn cell(&self, s: usize, a: usize) -> &CellReport {
        &self.cells[s * self.k + a]
    }

    /// Compute the attribution statistic.
    #[must_use]
    pub fn attribution(&self) -> Attribution {
        let mut seed_same = 0usize;
        let mut seed_pairs = 0usize;
        let mut seed_rho = 0.0f64;
        for a in 0..self.k {
            for s1 in 0..self.k {
                for s2 in (s1 + 1)..self.k {
                    let c1 = self.cell(s1, a);
                    let c2 = self.cell(s2, a);
                    seed_pairs += 1;
                    if c1.top_identity == c2.top_identity {
                        seed_same += 1;
                    }
                    seed_rho += spearman_rho(&c1.symbol_degree, &c2.symbol_degree);
                }
            }
        }

        let mut alpha_same = 0usize;
        let mut alpha_pairs = 0usize;
        let mut alpha_rho = 0.0f64;
        for s in 0..self.k {
            for a1 in 0..self.k {
                for a2 in (a1 + 1)..self.k {
                    let c1 = self.cell(s, a1);
                    let c2 = self.cell(s, a2);
                    alpha_pairs += 1;
                    if c1.top_identity == c2.top_identity {
                        alpha_same += 1;
                    }
                    alpha_rho += spearman_rho(&c1.symbol_degree, &c2.symbol_degree);
                }
            }
        }

        let agree_across_seeds = seed_same as f64 / seed_pairs as f64;
        let agree_across_alphabets = alpha_same as f64 / alpha_pairs as f64;

        Attribution {
            agree_across_seeds,
            agree_across_alphabets,
            rho_symbol_rank_across_seeds: seed_rho / seed_pairs as f64,
            rho_symbol_rank_across_alphabets: alpha_rho / alpha_pairs as f64,
            verdict: Self::verdict_from(agree_across_seeds, agree_across_alphabets),
            role_verdict: self.role_verdict(),
        }
    }

    /// A factor "moves the hub" if it does so in every pair and "leaves
    /// it fixed" if it never does. Anything in between is reported as
    /// inconclusive rather than rounded toward a reading.
    fn verdict_from(agree_seeds: f64, agree_alphabets: f64) -> Verdict {
        let seed_moves = agree_seeds == 0.0;
        let seed_fixed = agree_seeds == 1.0;
        let alpha_moves = agree_alphabets == 0.0;
        let alpha_fixed = agree_alphabets == 1.0;
        match (seed_moves, seed_fixed, alpha_moves, alpha_fixed) {
            (true, _, _, true) => Verdict::TracksAnchor,
            (_, true, true, _) => Verdict::TracksAlphabet,
            (_, true, _, true) => Verdict::InvariantToBoth,
            _ => Verdict::Inconclusive,
        }
    }

    /// The same verdict computed over the top node's *role* rather than
    /// its identity.
    ///
    /// Needed because identity is degenerate when two nodes tie on
    /// degree: d₀ and d₁ sit within a few counts of each other and
    /// which one wins is decided by setup bookkeeping, so the
    /// identity statistic reads "inconclusive" in exactly the regime
    /// where the honest reading is "a primordial holds the top rank in
    /// every cell". This statistic is blind to that tie and reports
    /// `InvariantToBoth` when the role is constant across the grid.
    ///
    /// It is not a softer test: it reports `TracksAnchor` whenever the
    /// top node is the anchor in every cell, which is the
    /// Law-11-affirming outcome.
    fn role_verdict(&self) -> Verdict {
        let roles: HashSet<Role> = self.cells.iter().map(|c| c.top_role).collect();
        match roles.iter().copied().next() {
            Some(Role::Anchor) if roles.len() == 1 => Verdict::TracksAnchor,
            Some(Role::AlphabetElement) if roles.len() == 1 => Verdict::TracksAlphabet,
            Some(Role::Primordial | Role::Interior) if roles.len() == 1 => Verdict::InvariantToBoth,
            _ => Verdict::Inconclusive,
        }
    }

    /// Under [`Routing::Shipped`] the anchor is touched only at the
    /// fold's first step, so it can take at most one novel bump per
    /// distinct first step — a ceiling of `ALPHABET_SIZE`, independent
    /// of how much data is folded.
    ///
    /// Returns `true` iff every cell's anchor fold-delta equals the
    /// corpus's distinct-first-step count exactly.
    #[must_use]
    pub fn anchor_delta_equals_distinct_first_steps(&self, corpus: &ByteCorpus) -> bool {
        self.routing == Routing::Shipped
            && self.cells.iter().all(|c| c.anchor.fold_delta == corpus.distinct_first_steps())
    }

    /// Whether the primordials gained *no* degree from folding the
    /// corpus in any cell — i.e. their standing is entirely
    /// alphabet-construction cost.
    #[must_use]
    pub fn primordials_gained_nothing_from_the_fold(&self) -> bool {
        self.cells.iter().all(|c| c.d0.fold_delta == 0 && c.d1.fold_delta == 0)
    }

    /// Structural audit of the design itself, independent of any
    /// outcome: the factors must be genuinely crossed and the shipped
    /// cell must reproduce the shipped encoding.
    ///
    /// Returns the list of failures; empty means the design is sound.
    #[must_use]
    pub fn audit(k: usize) -> Vec<String> {
        let mut failures = Vec::new();
        let engine = DistinctionEngine::new();

        // All factor values distinct, and no anchor is an alphabet
        // entry of any level.
        let anchors: Vec<Distinction> = (0..k).map(|s| anchor_for_seed(&engine, s)).collect();
        let alphabets: Vec<Box<[Distinction; ALPHABET_SIZE]>> =
            (0..k).map(|a| alphabet_for_level(&engine, a)).collect();

        let mut anchor_bytes = HashSet::new();
        for (s, anc) in anchors.iter().enumerate() {
            if !anchor_bytes.insert(*anc.as_bytes()) {
                failures.push(format!("anchor level {s} duplicates an earlier anchor"));
            }
        }
        let mut all_alphabet = HashSet::new();
        for (a, alpha) in alphabets.iter().enumerate() {
            for (b, d) in alpha.iter().enumerate() {
                if !all_alphabet.insert(*d.as_bytes()) {
                    failures.push(format!("alphabet {a} symbol {b} collides with another entry"));
                }
            }
        }
        for (s, anc) in anchors.iter().enumerate() {
            if all_alphabet.contains(anc.as_bytes()) {
                failures.push(format!("anchor level {s} is an alphabet entry — factors covary"));
            }
        }

        // The shipped cell must be the shipped encoding.
        let shipped_fold = koru_lambda_core::ByteFold::new(&engine);
        for b in 0..=255u8 {
            if alphabets[0][b as usize] != shipped_fold.tip(b) {
                failures.push(format!("alphabet level 0 symbol {b} is not the shipped byte tip"));
            }
        }
        if anchors[0].to_hex() != koru_lambda_core::vectors::D_HEX {
            failures.push(format!(
                "anchor level 0 is {} not the published anchor vector D",
                anchors[0].to_hex()
            ));
        }
        let probe_lift = Lift::build(&engine, 0, 0, Routing::Shipped);
        if probe_lift.fold(b"hi", &engine).to_hex()
            != koru_lambda_core::vectors::FOLD_HI_ANCHORED_AT_D_HEX
        {
            failures.push(
                "the probe's shipped routing does not reproduce the published fold vector"
                    .to_string(),
            );
        }
        if probe_lift.fold(b"a longer record", &engine)
            != shipped_fold.fold(anchors[0], b"a longer record", &engine)
        {
            failures.push("the probe's shipped routing disagrees with ByteFold::fold".to_string());
        }

        failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn design_audit_passes() {
        let failures = CrossedDesign::audit(6);
        assert!(failures.is_empty(), "design audit failed: {failures:?}");
    }

    #[test]
    fn probe_reports_anchor_tracking_on_the_anchor_routed_control() {
        // The probe must be able to return the Law-11-affirming
        // verdict. The anchor-routed encoding genuinely makes the
        // anchor a hub; if the statistic cannot see that, it is not
        // measuring what it claims.
        let corpus = ByteCorpus::zipf(1.0, 0xC0DE, 4096);
        let design = CrossedDesign::run(&corpus, 3, Routing::AnchorRouted);
        let attr = design.attribution();
        assert_eq!(
            attr.verdict,
            Verdict::TracksAnchor,
            "anchor-routed control did not produce the anchor verdict: {attr:?}"
        );
    }

    #[test]
    fn the_two_routings_do_not_produce_the_same_verdict() {
        // The discrimination claim, stated as a test: a probe whose
        // verdict is constant across encodings is not evidence.
        let corpus = ByteCorpus::zipf(1.0, 0xC0DE, 4096);
        let shipped = CrossedDesign::run(&corpus, 3, Routing::Shipped).attribution();
        let control = CrossedDesign::run(&corpus, 3, Routing::AnchorRouted).attribution();
        assert_ne!(
            shipped.verdict, control.verdict,
            "the probe returned the same verdict for two encodings that differ in \
             exactly the property under test"
        );
    }

    #[test]
    fn the_role_verdict_also_discriminates_the_two_routings() {
        // The role statistic is the one that survives the d0/d1 degree
        // tie. It must still be able to return the Law-11-affirming
        // answer, or it is not a test either.
        let corpus = ByteCorpus::zipf(1.0, 0xC0DE, 4096);
        let shipped = CrossedDesign::run(&corpus, 3, Routing::Shipped).attribution();
        let control = CrossedDesign::run(&corpus, 3, Routing::AnchorRouted).attribution();
        assert_eq!(control.role_verdict, Verdict::TracksAnchor);
        assert_eq!(shipped.role_verdict, Verdict::TracksAlphabet);
    }

    #[test]
    fn the_anchor_ceiling_is_the_distinct_first_step_count() {
        // Under the shipped routing the anchor is touched once per
        // fold, at a step whose symbol is the length's low byte, so its
        // novel participations equal the number of distinct first
        // steps — bounded by the alphabet size however large the
        // corpus. Asserted at two sizes an order of magnitude apart so
        // a size-dependent bound would show up.
        for size in [2048usize, 65_536] {
            let corpus = ByteCorpus::zipf(1.0, 0xC0DE, size);
            let design = CrossedDesign::run(&corpus, 3, Routing::Shipped);
            assert!(
                design.anchor_delta_equals_distinct_first_steps(&corpus),
                "anchor fold-delta is not the distinct-first-step count at {size} B"
            );
            assert!(corpus.distinct_first_steps() <= ALPHABET_SIZE);
        }
    }

    #[test]
    fn the_anchor_ceiling_does_not_hold_under_the_anchor_routed_control() {
        // The companion negative: the ceiling is a property of the
        // shipped routing, not of anchors in general.
        let corpus = ByteCorpus::zipf(1.0, 0xC0DE, 2048);
        let design = CrossedDesign::run(&corpus, 3, Routing::AnchorRouted);
        assert!(
            design.cell(0, 0).anchor.fold_delta > ALPHABET_SIZE,
            "anchor-routed control did not break the ceiling"
        );
    }

    #[test]
    fn primordials_gain_no_degree_from_folding_data() {
        let corpus = ByteCorpus::zipf(1.0, 0xC0DE, 16_384);
        for routing in [Routing::Shipped, Routing::AnchorRouted] {
            let design = CrossedDesign::run(&corpus, 3, routing);
            assert!(
                design.primordials_gained_nothing_from_the_fold(),
                "a primordial gained fold degree under {}",
                routing.label()
            );
        }
    }

    #[test]
    fn degree_is_the_novel_participation_count_exactly() {
        let corpus = ByteCorpus::zipf(1.0, 0xC0DE, 1024);
        let cell = run_cell(&corpus, 0, 0, Routing::Shipped);
        assert!(
            cell.degree_formula_exact,
            "degree != participations + addend somewhere — Law 12's structural part broke"
        );
    }

    #[test]
    fn corpus_is_deterministic() {
        let a = ByteCorpus::zipf(1.0, 0xC0DE, 2048);
        let b = ByteCorpus::zipf(1.0, 0xC0DE, 2048);
        assert_eq!(a.records, b.records);
        assert_eq!(a.symbol_freq, b.symbol_freq);
    }
}
