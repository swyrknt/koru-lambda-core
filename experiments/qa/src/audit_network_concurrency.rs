// Audit: NetworkAgent has no internal synchronization.
// Static observation -- this file documents the locking pattern by
// demonstrating that you must externally wrap NetworkAgent in a Mutex.
// It also stresses that the engine itself is concurrent, but the agent
// is the bottleneck.

use koru_lambda_core::{DistinctionEngine, NetworkAgent, PeerIdentity};
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    println!("=== Audit: NetworkAgent concurrency contract ===\n");

    let engine = Arc::new(DistinctionEngine::new());
    // NetworkAgent owns &mut self APIs only. Embedder must externally lock.
    let agent = Arc::new(Mutex::new(NetworkAgent::new(&engine)));

    let mut handles = Vec::new();
    for tid in 0..8 {
        let engine = Arc::clone(&engine);
        let agent = Arc::clone(&agent);
        handles.push(thread::spawn(move || {
            for i in 0..50 {
                let peer = PeerIdentity::new(format!("t{}_p{}", tid, i), &engine);
                let mut g = agent.lock().expect("agent mutex poisoned");
                g.join_peer(peer, &engine);
            }
        }));
    }
    for h in handles {
        h.join().expect("worker panicked");
    }

    let g = agent.lock().expect("agent mutex poisoned");
    let count = g.validator_count();
    println!(
        "  validator_count after 8x50 concurrent joins (Mutex-serialised) = {}",
        count
    );
    println!("  engine.distinction_count = {}", engine.distinction_count());
    println!(
        "  engine.relationship_count = {}",
        engine.relationship_count()
    );
    println!(
        "  r=2d-3 numerically? {}",
        engine.relationship_count() as i64 == 2 * engine.distinction_count() as i64 - 3
    );
    println!();
    println!("  OBSERVATION: NetworkAgent is &mut self everywhere. There is NO");
    println!("  built-in Mutex/RwLock. The above test only works because we wrap");
    println!("  the agent ourselves. Embedders MUST do this. Nothing in network.rs");
    println!("  enforces or documents the requirement.");
    println!("  VERDICT: agent is single-writer by construction, advertised as");
    println!("           network consensus. SEVERITY: MEDIUM (footgun / doc gap).");
}
