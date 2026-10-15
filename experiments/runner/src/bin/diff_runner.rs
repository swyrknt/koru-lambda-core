//! Rust half of the Python/Rust differential harness.
//!
//! Reads a *plan* — one command per line — from stdin and writes one JSON
//! record per command to stdout (NDJSON). `experiments/python/diff_harness.py`
//! generates the plan, runs this binary, executes the identical plan against
//! `experiments/python/engine.py`, and compares the two record streams.
//!
//! The plan is the whole of the shared state: every command names its
//! operands by index into a pool that both sides build the same way, so
//! neither side needs to be told anything the other computed. That is what
//! makes a record-for-record comparison meaningful — a divergence is a
//! divergence in the implementation, not in the driver.
//!
//! # Why JSON is hand-rolled here
//!
//! This crate carries no `serde`, and the harness is not worth a dependency
//! for a format whose every value is an integer, a bool, or a hex string.
//! [`esc`] handles the one case that is not trivially safe: a caught panic
//! message, which is arbitrary text.
//!
//! # Not shipped
//!
//! `experiments/*` is excluded in the root `Cargo.toml`, and this crate is
//! `publish = false`.
//!
//! ```text
//! cargo run -q -p koru-experiments-runner --bin diff_runner < plan.txt
//! ```
//!
//! ## Plan grammar
//!
//! ```text
//! synth <i> <j>              synthesize pool[i], pool[j]; push the result
//! degree <i>                 degree of pool[i]
//! counts                     distinction_count / relationship_count
//! parents <i>                canonical parent pair of pool[i], or null
//! verify_raw <hex>           trust-boundary acceptance of 16 raw bytes
//! cross <i>                  pool[i] read against a *fresh* second engine
//! cross_synth <i>            synthesize(pool[i], d0) on a fresh engine
//! cross_project <i> <dir> <bnd> <sig>   project pool[i] on a fresh engine
//! dump                       canonical-sorted parentage dump
//! digest                     SHA-256 over the canonical-sorted dump
//! audit                      check_structural_invariant
//! project <i> <dir> <bnd> <sig>   canonical_bytes + projection_id
//! restore <sig> <hex>        restore acceptance of wire bytes
//! replay <i>,<i>,...         replay the dump in the given entry order
//! lift_init                  fresh engine; build all 256 byte tips
//! lift_d                     the §10.1 `d`, inside the lift engine
//! lift_dom <hexlabel>        domain_anchor(label)
//! lift_acc <ref> <hexbytes>  raw chain: synthesize(acc, tip(b)) per byte
//! lift_fold <ref> <hexbytes> ByteFold::fold (§4.2)
//! lift_comp <ref> <ref,...>  ByteFold::fold_distinctions (§4.4)
//! ```
//!
//! `<dir>` is `up` / `down` / `undir`; `<bnd>` is `h<N>` / `sat`; `<sig>` is
//! `adj` / `deg` / `hop`. A `<ref>` into the lift engine is one of `d0`,
//! `d1`, `d`, `dom:<hexlabel>`, `t:<hexbyte>`, `v:<n>`.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::panic::AssertUnwindSafe;

use koru_lambda_core::projection::{Adjacency, Boundary, Degree, Direction, HopDistance};
use koru_lambda_core::replay::{replay_topological, ReplayError};
use koru_lambda_core::{
    domain_anchor, ByteFold, Distinction, DistinctionEngine, DomainLabel, ParentPair,
    RawDistinctionId,
};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------- JSON output

/// Escape a string for a JSON string literal.
///
/// Only one caller feeds this anything but hex and fixed identifiers: the
/// panic-message path, where the text is whatever an `assert!` in the
/// substrate wrote.
fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

fn jstr(s: &str) -> String {
    format!("\"{}\"", esc(s))
}

/// An output record under construction: ordered `(key, raw-JSON-value)` pairs.
struct Rec(Vec<(&'static str, String)>);

impl Rec {
    fn new(op: &'static str) -> Self {
        Self(vec![("op", jstr(op))])
    }

    fn s(mut self, k: &'static str, v: &str) -> Self {
        self.0.push((k, jstr(v)));
        self
    }

    fn n(mut self, k: &'static str, v: u64) -> Self {
        self.0.push((k, v.to_string()));
        self
    }

    fn b(mut self, k: &'static str, v: bool) -> Self {
        self.0.push((k, v.to_string()));
        self
    }

    /// Push an already-encoded JSON value (array, `null`, nested object).
    fn raw(mut self, k: &'static str, v: String) -> Self {
        self.0.push((k, v));
        self
    }

    fn build(&self) -> String {
        let body: Vec<String> = self.0.iter().map(|(k, v)| format!("{}:{}", jstr(k), v)).collect();
        format!("{{{}}}", body.join(","))
    }
}

fn err_rec(op: &'static str, msg: &str) -> String {
    Rec::new(op).b("ok", false).s("error", msg).build()
}

/// Run `f`, converting a panic into an `Err(message)` without printing it.
///
/// The substrate refuses an illegal fold with `assert!`, and the Python
/// prototype refuses the same fold with an exception. Comparing the two
/// requires catching the panic rather than aborting the plan.
fn quiet<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(AssertUnwindSafe(f));
    std::panic::set_hook(prev);
    r.map_err(|e| {
        if let Some(s) = e.downcast_ref::<&str>() {
            (*s).to_owned()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "panic".to_owned()
        }
    })
}

// ---------------------------------------------------------------- lift state

/// The second engine, used only for the lift.
///
/// Separate because `docs/SPEC.md` §4.3 makes a lift grow the engine by 513
/// distinctions, which would move every later `Degree` and `Saturated` read
/// in the main plan.
struct LiftState {
    engine: DistinctionEngine,
    fold: ByteFold,
    d: Option<Distinction>,
    anchors: BTreeMap<u8, Distinction>,
    vals: Vec<Distinction>,
}

impl LiftState {
    fn resolve(&self, r: &str) -> Result<Distinction, String> {
        match r {
            "d0" => Ok(self.engine.d0()),
            "d1" => Ok(self.engine.d1()),
            "d" => self.d.ok_or_else(|| "lift ref `d`: run lift_d first".to_owned()),
            _ => {
                if let Some(rest) = r.strip_prefix("t:") {
                    let b = u8::from_str_radix(rest, 16)
                        .map_err(|_| format!("lift ref `t:`: bad byte {rest}"))?;
                    Ok(self.fold.tip(b))
                } else if let Some(rest) = r.strip_prefix("dom:") {
                    let b = u8::from_str_radix(rest, 16)
                        .map_err(|_| format!("lift ref `dom:`: bad label {rest}"))?;
                    self.anchors
                        .get(&b)
                        .copied()
                        .ok_or_else(|| format!("lift ref `dom:{rest}`: run lift_dom first"))
                } else if let Some(rest) = r.strip_prefix("v:") {
                    let i: usize =
                        rest.parse().map_err(|_| format!("lift ref `v:`: bad index {rest}"))?;
                    self.vals
                        .get(i)
                        .copied()
                        .ok_or_else(|| format!("lift ref `v:{i}`: out of range"))
                } else {
                    Err(format!("unknown lift ref `{r}`"))
                }
            },
        }
    }
}

// ---------------------------------------------------------------- the runner

/// What one materialized projection contributes to a record: the §7 wire
/// bytes, the §8 identity, and the entry count. A named struct rather than a
/// tuple because the two hex strings are not interchangeable and a caller
/// reading `.0` and `.1` has no way to tell which is which.
struct ProjectionResult {
    bytes: String,
    pid: String,
    entries: usize,
}

struct Runner {
    engine: DistinctionEngine,
    pool: Vec<Distinction>,
    lift: Option<LiftState>,
}

/// Canonical-sorted parentage dump: sorted by `(child, p0, p1)`, which is
/// bytewise over `Distinction`'s `Ord`. `docs/SPEC.md` leaves the dump's own
/// order unspecified, so both sides sort before comparing.
fn sorted_dump(engine: &DistinctionEngine) -> Vec<(Distinction, ParentPair)> {
    let mut v = engine.snapshot_parentage();
    v.sort_unstable_by_key(|(c, (a, b))| (*c.as_bytes(), *a.as_bytes(), *b.as_bytes()));
    v
}

/// SHA-256 over the canonical-sorted dump, `child ‖ p0 ‖ p1` per entry.
fn dump_digest(engine: &DistinctionEngine) -> String {
    let mut h = Sha256::new();
    for (c, (a, b)) in sorted_dump(engine) {
        h.update(c.as_bytes());
        h.update(a.as_bytes());
        h.update(b.as_bytes());
    }
    hex::encode(h.finalize())
}

fn parse_dir(s: &str) -> Result<Direction, String> {
    match s {
        "up" => Ok(Direction::Upstream),
        "down" => Ok(Direction::Downstream),
        "undir" => Ok(Direction::Undirected),
        _ => Err(format!("unknown direction `{s}`")),
    }
}

fn parse_boundary(s: &str) -> Result<Boundary, String> {
    if s == "sat" {
        return Ok(Boundary::Saturated);
    }
    s.strip_prefix('h')
        .and_then(|n| n.parse::<usize>().ok())
        .map(Boundary::Hops)
        .ok_or_else(|| format!("unknown boundary `{s}`"))
}

/// Decode a plan payload. `.` is the empty sequence — a plan is whitespace
/// split, so an empty token cannot survive the trip, and a zero-length fold
/// is a case worth running (`docs/SPEC.md` §4.2 folds the length first, so
/// the empty sequence still produces eight synthesize calls).
fn decode_payload(s: &str) -> Result<Vec<u8>, String> {
    if s == "." {
        return Ok(Vec::new());
    }
    hex::decode(s).map_err(|e| format!("bad hex: {e}"))
}

/// Split a comma-separated plan list; `.` is the empty list.
fn split_list(s: &str) -> Vec<&str> {
    if s == "." {
        return Vec::new();
    }
    s.split(',').filter(|t| !t.is_empty()).collect()
}

fn parse_hex16(s: &str) -> Result<[u8; 16], String> {
    let v = hex::decode(s).map_err(|e| format!("bad hex: {e}"))?;
    <[u8; 16]>::try_from(v.as_slice())
        .map_err(|_| format!("expected 16 bytes, got {}", s.len() / 2))
}

impl Runner {
    fn new() -> Self {
        let engine = DistinctionEngine::new();
        let pool = vec![engine.d0(), engine.d1()];
        Self { engine, pool, lift: None }
    }

    fn at(&self, s: &str) -> Result<Distinction, String> {
        let i: usize = s.parse().map_err(|_| format!("bad pool index `{s}`"))?;
        self.pool.get(i).copied().ok_or_else(|| format!("pool index {i} out of range"))
    }

    fn lift_mut(&mut self) -> Result<&mut LiftState, String> {
        self.lift.as_mut().ok_or_else(|| "run lift_init first".to_owned())
    }

    fn exec(&mut self, line: &str) -> String {
        let f: Vec<&str> = line.split_whitespace().collect();
        let (op, args) = match f.split_first() {
            Some((op, rest)) => (*op, rest),
            None => return err_rec("empty", "blank command"),
        };
        match self.dispatch(op, args) {
            Ok(rec) => rec,
            Err(msg) => err_rec("error", &format!("{op}: {msg}")),
        }
    }

    fn dispatch(&mut self, op: &str, args: &[&str]) -> Result<String, String> {
        match op {
            "synth" => {
                let [a, b] = <[&str; 2]>::try_from(args).map_err(|_| "want 2 args")?;
                let (x, y) = (self.at(a)?, self.at(b)?);
                let outcome = self.engine.synthesize_novel(x, y);
                let d = outcome.distinction();
                let novel = outcome.is_novel();
                self.pool.push(d);
                Ok(Rec::new("synth")
                    .n("slot", (self.pool.len() - 1) as u64)
                    .s("id", &hex::encode(d.as_bytes()))
                    .b("novel", novel)
                    .build())
            },
            "degree" => {
                let [a] = <[&str; 1]>::try_from(args).map_err(|_| "want 1 arg")?;
                let d = self.at(a)?;
                Ok(Rec::new("degree")
                    .s("id", &hex::encode(d.as_bytes()))
                    .n("value", self.engine.degree(d) as u64)
                    .build())
            },
            "counts" => Ok(Rec::new("counts")
                .n("distinctions", self.engine.distinction_count() as u64)
                .n("relationships", self.engine.relationship_count() as u64)
                .build()),
            "parents" => {
                let [a] = <[&str; 1]>::try_from(args).map_err(|_| "want 1 arg")?;
                let d = self.at(a)?;
                let p = match self.engine.parents_of(d) {
                    None => "null".to_owned(),
                    Some((x, y)) => format!(
                        "[{},{}]",
                        jstr(&hex::encode(x.as_bytes())),
                        jstr(&hex::encode(y.as_bytes()))
                    ),
                };
                Ok(Rec::new("parents")
                    .s("id", &hex::encode(d.as_bytes()))
                    .raw("parents", p)
                    .build())
            },
            "verify_raw" => {
                let [h] = <[&str; 1]>::try_from(args).map_err(|_| "want 1 arg")?;
                let bytes = parse_hex16(h)?;
                let ok = self.engine.verify(RawDistinctionId::from_bytes(bytes)).is_ok();
                Ok(Rec::new("verify_raw").s("id", h).b("witnessed", ok).build())
            },
            // A `Distinction` carries no engine tag, so a value from this
            // engine can be read against another. `docs/SPEC.md` §6.1 says an
            // unwitnessed distinction has degree 0; this is the only way to
            // reach that arm from outside, since `verify` is the other door.
            "cross" => {
                let [a] = <[&str; 1]>::try_from(args).map_err(|_| "want 1 arg")?;
                let d = self.at(a)?;
                let other = DistinctionEngine::new();
                Ok(Rec::new("cross")
                    .s("id", &hex::encode(d.as_bytes()))
                    .b("has", other.has(d))
                    .n("degree", other.degree(d) as u64)
                    .b("parents_some", other.parents_of(d).is_some())
                    .build())
            },
            // `synthesize` with a parent the receiving engine never witnessed.
            // §3 calls it a programmer error rather than a runtime condition,
            // so both implementations refuse — one by panic, one by
            // exception. What is compared is that they refuse, and that the
            // refused call left the engine untouched.
            "cross_synth" => {
                let [a] = <[&str; 1]>::try_from(args).map_err(|_| "want 1 arg")?;
                let d = self.at(a)?;
                let other = DistinctionEngine::new();
                let got = quiet(|| other.synthesize(d, other.d0()));
                let rec = Rec::new("cross_synth").s("id", &hex::encode(d.as_bytes()));
                Ok(match got {
                    Ok(x) => rec
                        .b("ok", true)
                        .s("result", &hex::encode(x.as_bytes()))
                        .n("distinctions", other.distinction_count() as u64)
                        .build(),
                    Err(msg) => rec
                        .b("ok", false)
                        .s("detail", &msg)
                        .n("distinctions", other.distinction_count() as u64)
                        .build(),
                })
            },
            // A projection rooted at a distinction the engine never
            // witnessed. §5's root axis admits "any witnessed distinction"
            // and says nothing about an unwitnessed one, so this is the one
            // probe here whose expected answer the specification does not fix.
            "cross_project" => {
                let [r, dir, bnd, sig] = <[&str; 4]>::try_from(args).map_err(|_| "want 4 args")?;
                let d = self.at(r)?;
                let (direction, boundary) = (parse_dir(dir)?, parse_boundary(bnd)?);
                let other = DistinctionEngine::new();
                let got = quiet(|| Self::project(&other, d, direction, boundary, sig));
                let rec = Rec::new("cross_project")
                    .s("root", &hex::encode(d.as_bytes()))
                    .s("direction", dir)
                    .s("boundary", bnd)
                    .s("signal", sig);
                Ok(match got {
                    Ok(Ok(got)) => rec
                        .b("ok", true)
                        .n("entries", got.entries as u64)
                        .s("bytes", &got.bytes)
                        .s("pid", &got.pid)
                        .build(),
                    Ok(Err(msg)) | Err(msg) => rec.b("ok", false).s("detail", &msg).build(),
                })
            },
            "dump" => {
                let entries: Vec<String> = sorted_dump(&self.engine)
                    .into_iter()
                    .map(|(c, (a, b))| {
                        format!(
                            "[{},{},{}]",
                            jstr(&hex::encode(c.as_bytes())),
                            jstr(&hex::encode(a.as_bytes())),
                            jstr(&hex::encode(b.as_bytes()))
                        )
                    })
                    .collect();
                Ok(Rec::new("dump")
                    .n("len", entries.len() as u64)
                    .raw("entries", format!("[{}]", entries.join(",")))
                    .build())
            },
            "digest" => Ok(Rec::new("digest")
                .s("value", &dump_digest(&self.engine))
                .n("distinctions", self.engine.distinction_count() as u64)
                .build()),
            "audit" => Ok(match self.engine.check_structural_invariant() {
                Ok(()) => Rec::new("audit").b("ok", true).build(),
                Err(e) => Rec::new("audit").b("ok", false).s("kind", kind_of_invariant(&e)).build(),
            }),
            "project" => {
                let [r, dir, bnd, sig] = <[&str; 4]>::try_from(args).map_err(|_| "want 4 args")?;
                let root = self.at(r)?;
                let (direction, boundary) = (parse_dir(dir)?, parse_boundary(bnd)?);
                let got = Self::project(&self.engine, root, direction, boundary, sig)?;
                Ok(Rec::new("project")
                    .s("root", &hex::encode(root.as_bytes()))
                    .s("direction", dir)
                    .s("boundary", bnd)
                    .s("signal", sig)
                    .n("entries", got.entries as u64)
                    .s("bytes", &got.bytes)
                    .s("pid", &got.pid)
                    .build())
            },
            "restore" => {
                let [sig, h] = <[&str; 2]>::try_from(args).map_err(|_| "want 2 args")?;
                let bytes = decode_payload(h)?;
                let accepted = match sig {
                    "adj" => self.engine.restore_projection(&bytes, Adjacency).is_ok(),
                    "deg" => self.engine.restore_projection(&bytes, Degree).is_ok(),
                    "hop" => self.engine.restore_projection(&bytes, HopDistance).is_ok(),
                    _ => return Err(format!("unknown signal `{sig}`")),
                };
                Ok(Rec::new("restore").s("signal", sig).b("accepted", accepted).build())
            },
            // The entry order is supplied rather than chosen so both sides
            // replay the identical input; the dump itself is compared by the
            // `dump` command.
            "replay" => {
                let [list] = <[&str; 1]>::try_from(args).map_err(|_| "want 1 arg")?;
                let dump = sorted_dump(&self.engine);
                let mut entries: Vec<(Distinction, ParentPair)> = Vec::new();
                for tok in split_list(list) {
                    let i: usize = tok.parse().map_err(|_| format!("bad replay index `{tok}`"))?;
                    entries.push(
                        *dump.get(i).ok_or_else(|| format!("replay index {i} out of range"))?,
                    );
                }
                let fed = entries.len();
                Ok(match replay_topological(entries) {
                    Ok(e) => Rec::new("replay")
                        .n("fed", fed as u64)
                        .s("outcome", "ok")
                        .n("distinctions", e.distinction_count() as u64)
                        .n("relationships", e.relationship_count() as u64)
                        .s("digest", &dump_digest(&e))
                        .build(),
                    Err(err) => {
                        let (kind, remaining) = match err {
                            ReplayError::MissingPrimordial => ("missing_primordial", None),
                            ReplayError::Unreachable(n) => ("unreachable", Some(n)),
                            ReplayError::Mismatch { .. } => ("mismatch", None),
                            _ => ("other", None),
                        };
                        let r = Rec::new("replay").n("fed", fed as u64).s("outcome", kind);
                        match remaining {
                            Some(n) => r.n("remaining", n as u64).build(),
                            None => r.build(),
                        }
                    },
                })
            },
            "lift_init" => {
                let engine = DistinctionEngine::new();
                let fold = ByteFold::new(&engine);
                let tips: Vec<String> =
                    (0..=255u8).map(|b| jstr(&hex::encode(fold.tip(b).as_bytes()))).collect();
                let rec = Rec::new("lift_init")
                    .raw("tips", format!("[{}]", tips.join(",")))
                    .n("distinctions", engine.distinction_count() as u64)
                    .n("relationships", engine.relationship_count() as u64)
                    .n("degree_d0", engine.degree(engine.d0()) as u64)
                    .n("degree_d1", engine.degree(engine.d1()) as u64)
                    .build();
                self.lift = Some(LiftState {
                    engine,
                    fold,
                    d: None,
                    anchors: BTreeMap::new(),
                    vals: vec![],
                });
                Ok(rec)
            },
            // `docs/SPEC.md` §10.1's `d`, which §10.2 uses as the one anchor
            // that is provably not a byte tip.
            "lift_d" => {
                let st = self.lift_mut()?;
                let c = st.engine.synthesize(st.engine.d0(), st.engine.d1());
                let d = st.engine.synthesize(c, st.engine.d0());
                st.d = Some(d);
                Ok(Rec::new("lift_d")
                    .s("id", &hex::encode(d.as_bytes()))
                    .n("distinctions", st.engine.distinction_count() as u64)
                    .build())
            },
            "lift_dom" => {
                let [l] = <[&str; 1]>::try_from(args).map_err(|_| "want 1 arg")?;
                let label = u8::from_str_radix(l, 16).map_err(|_| format!("bad label `{l}`"))?;
                let st = self.lift_mut()?;
                let a = domain_anchor(&st.engine, &st.fold, DomainLabel::new(label));
                st.anchors.insert(label, a);
                Ok(Rec::new("lift_dom")
                    .s("label", l)
                    .s("id", &hex::encode(a.as_bytes()))
                    .n("distinctions", st.engine.distinction_count() as u64)
                    .build())
            },
            // The bare fold chain with *no* length prefix, which is how a
            // caller can name the accumulator a composite fold would reach —
            // the one way to drive §4.4's "no part may equal the accumulator"
            // refusal on purpose.
            "lift_acc" => {
                let [r, h] = <[&str; 2]>::try_from(args).map_err(|_| "want 2 args")?;
                let bytes = decode_payload(h)?;
                let st = self.lift_mut()?;
                let mut acc = st.resolve(r)?;
                for b in bytes {
                    acc = st.engine.synthesize(acc, st.fold.tip(b));
                }
                st.vals.push(acc);
                Ok(Rec::new("lift_acc")
                    .n("slot", (st.vals.len() - 1) as u64)
                    .s("id", &hex::encode(acc.as_bytes()))
                    .n("distinctions", st.engine.distinction_count() as u64)
                    .build())
            },
            "lift_fold" => {
                let [r, h] = <[&str; 2]>::try_from(args).map_err(|_| "want 2 args")?;
                let bytes = decode_payload(h)?;
                let st = self.lift_mut()?;
                let anchor = st.resolve(r)?;
                let (engine, fold) = (&st.engine, &st.fold);
                let got = quiet(|| fold.fold(anchor, &bytes, engine));
                Ok(Self::fold_rec("lift_fold", st, got))
            },
            "lift_comp" => {
                let [r, parts] = <[&str; 2]>::try_from(args).map_err(|_| "want 2 args")?;
                let st = self.lift_mut()?;
                let anchor = st.resolve(r)?;
                let elements: Vec<Distinction> = split_list(parts)
                    .into_iter()
                    .map(|t| st.resolve(t))
                    .collect::<Result<_, _>>()?;
                let (engine, fold) = (&st.engine, &st.fold);
                let got = quiet(|| fold.fold_distinctions(anchor, &elements, engine));
                Ok(Self::fold_rec("lift_comp", st, got))
            },
            _ => Err("unknown command".to_owned()),
        }
    }

    /// Shared tail of `lift_fold` / `lift_comp`: record the result, or the
    /// refusal, together with the engine state either way.
    ///
    /// The state is reported on the refusal path too, because a refusal that
    /// happens *after* the length prefix has been folded has already grown the
    /// engine — and the two implementations have to grow it by the same amount.
    fn fold_rec(op: &'static str, st: &mut LiftState, got: Result<Distinction, String>) -> String {
        match got {
            Ok(d) => {
                st.vals.push(d);
                Rec::new(op)
                    .b("ok", true)
                    .n("slot", (st.vals.len() - 1) as u64)
                    .s("id", &hex::encode(d.as_bytes()))
                    .n("distinctions", st.engine.distinction_count() as u64)
                    .build()
            },
            // `detail` is free text — the substrate's `assert!` message here,
            // an exception's `str` on the Python side — and the harness
            // excludes it from comparison. What must agree is `ok` and the
            // engine size the refusal left behind.
            Err(msg) => Rec::new(op)
                .b("ok", false)
                .s("detail", &msg)
                .n("distinctions", st.engine.distinction_count() as u64)
                .build(),
        }
    }

    /// Materialize one projection. The signal is a type parameter in the
    /// substrate, so the three arms cannot be collapsed into a value.
    fn project(
        engine: &DistinctionEngine,
        root: Distinction,
        direction: Direction,
        boundary: Boundary,
        sig: &str,
    ) -> Result<ProjectionResult, String> {
        macro_rules! go {
            ($s:expr) => {{
                let p = engine
                    .project(root)
                    .direction(direction)
                    .boundary(boundary)
                    .signal($s)
                    .materialize();
                ProjectionResult {
                    bytes: hex::encode(p.canonical_bytes()),
                    pid: hex::encode(p.projection_id().as_bytes()),
                    entries: p.output().len(),
                }
            }};
        }
        Ok(match sig {
            "adj" => go!(Adjacency),
            "deg" => go!(Degree),
            "hop" => go!(HopDistance),
            _ => return Err(format!("unknown signal `{sig}`")),
        })
    }
}

/// Name the invariant that broke, so the two auditors can be compared by
/// which law failed rather than by message text.
fn kind_of_invariant(e: &koru_lambda_core::InvariantError) -> &'static str {
    use koru_lambda_core::InvariantError as E;
    match e {
        E::BinaryParentageMismatch { .. } => "binary_parentage",
        E::UnregisteredParent { .. } => "unregistered_parent",
        E::ContentAddressMismatch { .. } => "content_address",
        E::DegreeMismatch { .. } => "degree",
        E::SelfParentage { .. } => "self_parentage",
        _ => "other",
    }
}

fn main() -> io::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut runner = Runner::new();
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        writeln!(out, "{}", runner.exec(line))?;
    }
    out.flush()
}
