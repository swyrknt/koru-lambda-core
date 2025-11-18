// Falsification test suite
mod falsification;

// Import falsification tests
#[path = "falsification/commutativity.rs"]
mod commutativity;

#[path = "falsification/determinism.rs"]
mod determinism;

#[path = "falsification/non_associativity.rs"]
mod non_associativity;

#[path = "falsification/robustness.rs"]
mod robustness;

#[path = "falsification/information_dynamics.rs"]
mod information_dynamics;

#[path = "falsification/structural_coherence.rs"]
mod structural_coherence;

#[path = "falsification/compaction.rs"]
mod compaction;

#[path = "falsification/network_consensus.rs"]
mod network_consensus;

#[path = "falsification/wasm_consistency.rs"]
mod wasm_consistency;
