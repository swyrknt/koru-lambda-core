/// Falsification Test: Structural Coherence Dynamics
///
/// Tests whether high-coherence subprocesses exhibit greater evolutionary
/// instability than low-coherence subprocesses, indicating feedback between
/// structural properties and subsequent evolution.
///
/// Falsification Target:
/// Passive labeling - high-coherence designation has no causal influence
/// on subsequent structural evolution, proving structural properties lack
/// dynamical consequences.
use koru_lambda_core::DistinctionEngine;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::IntoNodeIdentifiers;
use rand::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

/// Helper module for graph analysis and subprocess dynamics
mod coherence_helpers {
    use super::*;

    /// Converts engine state to petgraph for analysis
    pub fn build_graph(engine: &DistinctionEngine) -> Graph<String, (), petgraph::Undirected> {
        let (distinctions, relationships) = engine.get_state_snapshot_unsynchronized();

        let mut graph = Graph::new_undirected();
        let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

        for distinction in &distinctions {
            let node_idx = graph.add_node(distinction.to_hex());
            node_map.insert(distinction.to_hex(), node_idx);
        }

        for (id_a, id_b) in &relationships {
            if let (Some(&idx_a), Some(&idx_b)) = (node_map.get(id_a), node_map.get(id_b)) {
                graph.add_edge(idx_a, idx_b, ());
            }
        }

        graph
    }

    /// Extract ego graph: all nodes within specified radius of start node
    pub fn ego_graph(
        graph: &Graph<String, (), petgraph::Undirected>,
        start: NodeIndex,
        radius: usize,
    ) -> HashSet<NodeIndex> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();
        let mut distances: HashMap<NodeIndex, usize> = HashMap::new();

        queue.push_back(start);
        distances.insert(start, 0);
        result.insert(start);

        while let Some(current) = queue.pop_front() {
            let current_distance = distances[&current];

            if current_distance < radius {
                for neighbor in graph.neighbors(current) {
                    if let std::collections::hash_map::Entry::Vacant(e) = distances.entry(neighbor)
                    {
                        e.insert(current_distance + 1);
                        result.insert(neighbor);
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        result
    }

    /// Calculate clustering coefficient for a single node
    fn node_clustering_coefficient(
        graph: &Graph<String, (), petgraph::Undirected>,
        node: NodeIndex,
    ) -> f64 {
        let neighbors: Vec<NodeIndex> = graph.neighbors(node).collect();
        let degree = neighbors.len();

        if degree < 2 {
            return 0.0;
        }

        // Count triangles: edges between neighbors
        let mut triangles = 0;

        for i in 0..neighbors.len() {
            for j in (i + 1)..neighbors.len() {
                if graph.contains_edge(neighbors[i], neighbors[j]) {
                    triangles += 1;
                }
            }
        }

        let possible_triangles = degree * (degree - 1) / 2;
        triangles as f64 / possible_triangles as f64
    }

    /// Calculate clustering coefficient for all nodes
    pub fn clustering_coefficients(
        graph: &Graph<String, (), petgraph::Undirected>,
    ) -> HashMap<NodeIndex, f64> {
        graph.node_identifiers().map(|n| (n, node_clustering_coefficient(graph, n))).collect()
    }

    /// Calculate average clustering coefficient for a subprocess
    pub fn subprocess_coherence(
        clustering: &HashMap<NodeIndex, f64>,
        subprocess_nodes: &HashSet<NodeIndex>,
    ) -> f64 {
        if subprocess_nodes.is_empty() {
            return 0.0;
        }

        let sum: f64 = subprocess_nodes.iter().filter_map(|n| clustering.get(n)).sum();

        sum / subprocess_nodes.len() as f64
    }
}

/// Evolve universe with local selection bias
fn evolve_locally(engine: &mut DistinctionEngine, steps: usize, rng: &mut StdRng) {
    for _step in 0..steps {
        let distinctions = engine.get_state_snapshot_unsynchronized().0;

        if distinctions.len() < 2 {
            continue;
        }

        let graph = coherence_helpers::build_graph(engine);
        let id_to_idx: HashMap<String, NodeIndex> =
            graph.node_identifiers().map(|idx| (graph[idx].clone(), idx)).collect();

        let a = distinctions.choose(rng).unwrap().clone();
        let a_idx = id_to_idx.get(&a.to_hex());

        let b = if let Some(&idx_a) = a_idx {
            let mut neighborhood = HashSet::new();
            for neighbor_1 in graph.neighbors(idx_a) {
                neighborhood.insert(neighbor_1);
                for neighbor_2 in graph.neighbors(neighbor_1) {
                    if neighbor_2 != idx_a {
                        neighborhood.insert(neighbor_2);
                    }
                }
            }

            let neighborhood_distinctions: Vec<_> = distinctions
                .iter()
                .filter(|d| {
                    if let Some(&idx) = id_to_idx.get(&d.to_hex()) {
                        neighborhood.contains(&idx) && d.to_hex() != a.to_hex()
                    } else {
                        false
                    }
                })
                .cloned()
                .collect();

            if !neighborhood_distinctions.is_empty() {
                neighborhood_distinctions.choose(rng).unwrap().clone()
            } else {
                let others: Vec<_> =
                    distinctions.iter().filter(|d| d.to_hex() != a.to_hex()).cloned().collect();
                if others.is_empty() {
                    continue;
                }
                others.choose(rng).unwrap().clone()
            }
        } else {
            let others: Vec<_> =
                distinctions.iter().filter(|d| d.to_hex() != a.to_hex()).cloned().collect();
            if others.is_empty() {
                continue;
            }
            others.choose(rng).unwrap().clone()
        };

        engine.synthesize(&a, &b);
    }
}

/// Evolve a specific subprocess internally
///
/// Confines synthesis operations to distinctions within the subprocess,
/// allowing measurement of localized evolutionary dynamics.
fn evolve_subprocess(
    engine: &mut DistinctionEngine,
    subprocess_node_ids: &HashSet<String>,
    steps: usize,
    rng: &mut StdRng,
) {
    for _step in 0..steps {
        let all_distinctions = engine.get_state_snapshot_unsynchronized().0;

        let subprocess_distinctions: Vec<_> = all_distinctions
            .iter()
            .filter(|d| subprocess_node_ids.contains(&d.to_hex()))
            .cloned()
            .collect();

        if subprocess_distinctions.len() < 2 {
            continue;
        }

        let a = subprocess_distinctions.choose(rng).unwrap().clone();
        let b_options: Vec<_> =
            subprocess_distinctions.iter().filter(|d| d.to_hex() != a.to_hex()).cloned().collect();

        if b_options.is_empty() {
            continue;
        }

        let b = b_options.choose(rng).unwrap().clone();
        engine.synthesize(&a, &b);
    }
}

/// Falsification Test: Structural Feedback
///
/// Hypothesis: High-coherence subprocesses exhibit greater coherence
/// change magnitude during local evolution than low-coherence subprocesses,
/// demonstrating feedback between structure and dynamics.
///
/// Falsifies if: High-coherence and low-coherence subprocesses show
/// equivalent coherence change (|Δ_high| ≤ |Δ_low|), proving coherence has
/// no dynamical effect and acts only as passive labeling.
///
/// Measurement:
/// 1. Evolve substrate with local bias (5000 steps)
/// 2. Identify highest and lowest coherence subprocesses
/// 3. Evolve each subprocess independently (100 steps)
/// 4. Measure coherence change magnitude for each
#[test]
fn test_structural_feedback() {
    // ============================================================
    // SETUP
    // ============================================================
    println!("\nTest: Structural Feedback Falsification");
    println!("  Testing for structural influence on dynamics...");

    let mut engine = DistinctionEngine::new();
    let mut rng = StdRng::seed_from_u64(123);

    // ============================================================
    // EVOLUTION: Build Substrate
    // ============================================================
    println!("  Executing 5000 synthesis operations...");
    evolve_locally(&mut engine, 5000, &mut rng);

    let initial_graph = coherence_helpers::build_graph(&engine);
    let node_count = initial_graph.node_count();

    println!("  Graph size: {} nodes, {} edges", node_count, initial_graph.edge_count());

    assert!(node_count >= 100, "Graph too small for robust testing: {} nodes", node_count);

    // ============================================================
    // SUBPROCESS IDENTIFICATION: Find High/Low Coherence
    // ============================================================
    println!("  Identifying high and low coherence subprocesses...");

    let all_clustering = coherence_helpers::clustering_coefficients(&initial_graph);

    // Calculate average coherence for all ego graphs
    let mut subprocess_coherences: Vec<(NodeIndex, f64)> = Vec::new();

    for node_idx in initial_graph.node_identifiers() {
        let ego = coherence_helpers::ego_graph(&initial_graph, node_idx, 2);
        let coherence = coherence_helpers::subprocess_coherence(&all_clustering, &ego);
        subprocess_coherences.push((node_idx, coherence));
    }

    // Sort by coherence
    subprocess_coherences.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let (high_coherence_center, high_coherence_initial) = subprocess_coherences[0];
    let (low_coherence_center, low_coherence_initial) = *subprocess_coherences.last().unwrap();

    println!("    High-coherence subprocess: {:.4}", high_coherence_initial);
    println!("    Low-coherence subprocess: {:.4}", low_coherence_initial);

    assert!(
        high_coherence_initial - low_coherence_initial > 0.01,
        "Could not find sufficiently different subprocesses to compare"
    );

    // Get node IDs for each subprocess
    let high_subprocess_nodes =
        coherence_helpers::ego_graph(&initial_graph, high_coherence_center, 2);
    let low_subprocess_nodes =
        coherence_helpers::ego_graph(&initial_graph, low_coherence_center, 2);

    let high_subprocess_ids: HashSet<String> =
        high_subprocess_nodes.iter().map(|&idx| initial_graph[idx].clone()).collect();

    let low_subprocess_ids: HashSet<String> =
        low_subprocess_nodes.iter().map(|&idx| initial_graph[idx].clone()).collect();

    println!("    High-coherence subprocess: {} nodes", high_subprocess_ids.len());
    println!("    Low-coherence subprocess: {} nodes", low_subprocess_ids.len());

    // ============================================================
    // LOCAL EVOLUTION: Perturb Each Subprocess
    // ============================================================
    println!("  Evolving high-coherence subprocess internally (100 steps)...");
    evolve_subprocess(&mut engine, &high_subprocess_ids, 100, &mut rng);

    println!("  Evolving low-coherence subprocess internally (100 steps)...");
    evolve_subprocess(&mut engine, &low_subprocess_ids, 100, &mut rng);

    // ============================================================
    // MEASUREMENT: Coherence Change
    // ============================================================
    let final_graph = coherence_helpers::build_graph(&engine);
    let final_clustering = coherence_helpers::clustering_coefficients(&final_graph);

    let final_high_subprocess =
        coherence_helpers::ego_graph(&final_graph, high_coherence_center, 2);
    let final_low_subprocess = coherence_helpers::ego_graph(&final_graph, low_coherence_center, 2);

    let high_coherence_final =
        coherence_helpers::subprocess_coherence(&final_clustering, &final_high_subprocess);
    let low_coherence_final =
        coherence_helpers::subprocess_coherence(&final_clustering, &final_low_subprocess);

    let delta_high = high_coherence_final - high_coherence_initial;
    let delta_low = low_coherence_final - low_coherence_initial;

    println!("\n  Results:");
    println!("    High-coherence change: {:+.4}", delta_high);
    println!("    Low-coherence change: {:+.4}", delta_low);

    // ============================================================
    // ASSERTION
    // ============================================================
    assert!(
        delta_high.abs() > delta_low.abs(),
        "FALSIFIED: High-coherence subprocesses are not more dynamically unstable.\n  \
         |Δ_high| = {:.4}, |Δ_low| = {:.4}\n  \
         Coherence has no causal influence on dynamics.",
        delta_high.abs(),
        delta_low.abs()
    );

    // ============================================================
    // REPORTING
    // ============================================================
    println!("\nHypothesis sustained.");
    println!("  High-coherence subprocesses exhibit greater evolutionary instability.");
    println!("  Magnitude ratio: {:.2}x", delta_high.abs() / delta_low.abs().max(0.0001));
    println!("  Structure causally influences dynamics.");
}
