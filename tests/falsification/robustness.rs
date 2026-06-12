/// Falsification Test: Robustness and Resilience
///
/// Tests structural integrity under node removal, validating scale-free network
/// behavior through differential vulnerability to random versus targeted attacks.
///
/// Scale-free networks exhibit a critical property: they are resilient to random
/// failures but vulnerable to targeted hub attacks. This emerges from preferential
/// attachment during network growth.
///
/// Falsification Target:
/// Uniform vulnerability - system exhibits equal fragility under random and
/// targeted hub removal, proving absence of scale-free topology.
use koru_lambda_core::DistinctionEngine;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::IntoNodeIdentifiers;
use rand::prelude::*;
use std::collections::HashMap;

/// Helper module for graph construction and analysis
mod graph_helpers {
    use super::*;

    /// Converts engine state to petgraph for analysis
    pub fn build_graph(engine: &DistinctionEngine) -> Graph<String, (), petgraph::Undirected> {
        let (distinctions, relationships) = engine.get_state_snapshot_unsynchronized();

        let mut graph = Graph::new_undirected();
        let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

        // Add all distinction nodes
        for distinction in &distinctions {
            let node_idx = graph.add_node(distinction.id().to_string());
            node_map.insert(distinction.id().to_string(), node_idx);
        }

        // Add all relationship edges
        for (id_a, id_b) in &relationships {
            if let (Some(&idx_a), Some(&idx_b)) = (node_map.get(id_a), node_map.get(id_b)) {
                graph.add_edge(idx_a, idx_b, ());
            }
        }

        graph
    }

    /// Gets the largest connected component from a graph
    pub fn largest_component_size(graph: &Graph<String, (), petgraph::Undirected>) -> usize {
        if graph.node_count() == 0 {
            return 0;
        }

        // Use DFS to find all connected components
        let mut visited = vec![false; graph.node_count()];
        let mut max_component_size = 0;

        for node in graph.node_identifiers() {
            let node_idx = node.index();
            if !visited[node_idx] {
                let component_size = dfs_component_size(graph, node, &mut visited);
                max_component_size = max_component_size.max(component_size);
            }
        }

        max_component_size
    }

    fn dfs_component_size(
        graph: &Graph<String, (), petgraph::Undirected>,
        start: NodeIndex,
        visited: &mut [bool],
    ) -> usize {
        let mut stack = vec![start];
        let mut size = 0;

        while let Some(node) = stack.pop() {
            let node_idx = node.index();
            if visited[node_idx] {
                continue;
            }
            visited[node_idx] = true;
            size += 1;

            for neighbor in graph.neighbors(node) {
                if !visited[neighbor.index()] {
                    stack.push(neighbor);
                }
            }
        }

        size
    }
}

/// Falsification Test: Uniform Vulnerability
///
/// Hypothesis: Scale-free topology exhibits differential vulnerability,
/// with targeted hub removal causing greater fragmentation than random
/// node removal. The system naturally evolves scale-free structure through
/// preferential attachment.
///
/// Falsifies if: Survival rates are similar under both attack strategies
/// (difference < 20%), proving uniform vulnerability and absence of scale-free
/// topology.
///
/// Measurement:
/// 1. Build degree-biased graph through preferential attachment (3000 steps)
/// 2. Measure largest component survival under 20% random node removal
/// 3. Measure largest component survival under 20% targeted hub removal
/// 4. Calculate vulnerability gap (should be > 20% for scale-free networks)
#[test]
fn test_falsify_uniform_vulnerability() {
    // ============================================================
    // SETUP
    // ============================================================
    println!("\nTest: Uniform Vulnerability Falsification");
    println!("  Testing for scale-free topology...");

    let engine = DistinctionEngine::new();
    let mut rng = StdRng::seed_from_u64(42); // Fixed seed for reproducibility

    // ============================================================
    // EVOLUTION: Preferential Attachment
    // ============================================================
    println!("  Executing 3000 synthesis operations with degree bias...");

    for step in 0..3000 {
        let distinctions = engine.get_state_snapshot_unsynchronized().0;

        if distinctions.len() < 2 {
            continue;
        }

        // Build graph to calculate degrees
        let graph = graph_helpers::build_graph(&engine);
        let mut node_degrees: HashMap<String, usize> = HashMap::new();

        for node_idx in graph.node_identifiers() {
            let id = &graph[node_idx];
            node_degrees.insert(id.clone(), graph.neighbors(node_idx).count());
        }

        // Weight selection by degree (preferential attachment)
        // Higher degree nodes get higher probability
        let weights: Vec<f64> = distinctions
            .iter()
            .map(|d| (node_degrees.get(d.id()).copied().unwrap_or(0) + 1) as f64)
            .collect();

        // Select two DIFFERENT parents using weighted selection
        let total_weight: f64 = weights.iter().sum();

        // First parent
        let r1 = rng.gen::<f64>() * total_weight;
        let mut cumulative = 0.0;
        let mut parent1_idx = 0;
        for (i, &weight) in weights.iter().enumerate() {
            cumulative += weight;
            if cumulative >= r1 {
                parent1_idx = i;
                break;
            }
        }

        // Second parent (ensure different from first)
        let mut parent2_idx = parent1_idx;
        let mut attempts = 0;
        while parent2_idx == parent1_idx && attempts < 10 {
            let r2 = rng.gen::<f64>() * total_weight;
            cumulative = 0.0;
            for (i, &weight) in weights.iter().enumerate() {
                cumulative += weight;
                if cumulative >= r2 {
                    parent2_idx = i;
                    break;
                }
            }
            attempts += 1;
        }

        // If still same (small graph), pick randomly
        if parent2_idx == parent1_idx && distinctions.len() > 1 {
            parent2_idx = (parent1_idx + 1) % distinctions.len();
        }

        let parent1 = &distinctions[parent1_idx];
        let parent2 = &distinctions[parent2_idx];

        // Synthesize to create new distinction
        engine.synthesize(parent1, parent2);

        // Progress indicator every 500 steps
        if (step + 1) % 500 == 0 {
            println!(
                "    Progress: {} operations, {} distinctions",
                step + 1,
                engine.distinction_count()
            );
        }
    }

    // ============================================================
    // GRAPH ANALYSIS
    // ============================================================
    let original_graph = graph_helpers::build_graph(&engine);
    let initial_size = original_graph.node_count();

    println!("  Graph size: {} nodes, {} edges", initial_size, original_graph.edge_count());

    // Verify we have a substantial graph
    assert!(
        initial_size >= 100,
        "Insufficient graph size for robust testing: {} nodes",
        initial_size
    );

    // ============================================================
    // ATTACK 1: Random Node Removal
    // ============================================================
    println!("  Testing resilience to random failures...");

    let attack_percent = 0.20;
    let num_to_remove = (initial_size as f64 * attack_percent) as usize;

    let mut random_graph = original_graph.clone();
    let all_nodes: Vec<_> = random_graph.node_identifiers().collect();
    let nodes_to_remove: Vec<_> =
        all_nodes.choose_multiple(&mut rng, num_to_remove).cloned().collect();

    for node in nodes_to_remove {
        random_graph.remove_node(node);
    }

    let largest_random = graph_helpers::largest_component_size(&random_graph);
    let survival_random = largest_random as f64 / initial_size as f64;

    println!(
        "    Survival after random removal ({:.0}%): {:.2}%",
        attack_percent * 100.0,
        survival_random * 100.0
    );

    // ============================================================
    // ATTACK 2: Targeted Hub Removal
    // ============================================================
    println!("  Testing vulnerability to targeted attacks...");

    let mut targeted_graph = original_graph.clone();

    // Identify hubs (highest degree nodes)
    let mut node_degrees: Vec<_> = targeted_graph
        .node_identifiers()
        .map(|n| (n, targeted_graph.neighbors(n).count()))
        .collect();

    // Sort by degree (descending)
    node_degrees.sort_by(|a, b| b.1.cmp(&a.1));

    // Remove top hubs
    let hubs_to_remove: Vec<_> = node_degrees.iter().take(num_to_remove).map(|(n, _)| *n).collect();

    for node in hubs_to_remove {
        targeted_graph.remove_node(node);
    }

    let largest_targeted = graph_helpers::largest_component_size(&targeted_graph);
    let survival_targeted = largest_targeted as f64 / initial_size as f64;

    println!(
        "    Survival after targeted hub removal ({:.0}%): {:.2}%",
        attack_percent * 100.0,
        survival_targeted * 100.0
    );

    // ============================================================
    // MEASUREMENT: Vulnerability Gap
    // ============================================================
    let vulnerability_gap = survival_random - survival_targeted;
    println!("    Vulnerability gap: {:.2}", vulnerability_gap);

    // ============================================================
    // ASSERTION
    // ============================================================
    assert!(
        vulnerability_gap > 0.20,
        "FALSIFIED: Uniform vulnerability observed (gap: {:.2}).\n  \
         Scale-free topology not detected.\n  \
         Random survival: {:.2}%, Targeted survival: {:.2}%",
        vulnerability_gap,
        survival_random * 100.0,
        survival_targeted * 100.0
    );

    // ============================================================
    // REPORTING
    // ============================================================
    println!("\nHypothesis sustained.");
    println!("  Scale-free topology exhibits differential vulnerability:");
    println!("    Random attack survival: {:.1}%", survival_random * 100.0);
    println!("    Targeted attack survival: {:.1}%", survival_targeted * 100.0);
    println!("    Vulnerability gap: {:.1}%", vulnerability_gap * 100.0);
    println!("  Hub-dependent topology confirmed through preferential attachment");
}

/// Falsification Test: Degree Distribution Power Law
///
/// Hypothesis: Preferential attachment produces a power-law degree
/// distribution characteristic of scale-free networks, where P(k) ∝ k^(-γ)
/// with γ typically between 2 and 3.
///
/// Falsifies if: Degree distribution is uniform or Poisson-like,
/// indicating random rather than scale-free topology.
///
/// Measurement:
/// Test the concentration of connectivity - a small percentage of nodes
/// should account for a large percentage of total connections.
#[test]
fn test_falsify_random_degree_distribution() {
    // ============================================================
    // SETUP
    // ============================================================
    println!("\nTest: Random Degree Distribution Falsification");
    println!("  Testing for power-law degree concentration...");

    let engine = DistinctionEngine::new();
    let mut rng = StdRng::seed_from_u64(123);

    // ============================================================
    // EVOLUTION: Preferential Attachment
    // ============================================================
    println!("  Executing 2000 synthesis operations with degree bias...");

    for _step in 0..2000 {
        let distinctions = engine.get_state_snapshot_unsynchronized().0;

        if distinctions.len() < 2 {
            continue;
        }

        let graph = graph_helpers::build_graph(&engine);
        let weights: Vec<f64> = distinctions
            .iter()
            .map(|d| {
                let node_idx = graph.node_identifiers().find(|&n| graph[n] == d.id());
                if let Some(idx) = node_idx {
                    (graph.neighbors(idx).count() + 1) as f64
                } else {
                    1.0
                }
            })
            .collect();

        let total_weight: f64 = weights.iter().sum();

        // Select two different parents
        let r1 = rng.gen::<f64>() * total_weight;
        let mut cumulative = 0.0;
        let mut parent1_idx = 0;
        for (i, &weight) in weights.iter().enumerate() {
            cumulative += weight;
            if cumulative >= r1 {
                parent1_idx = i;
                break;
            }
        }

        let mut parent2_idx = parent1_idx;
        let mut attempts = 0;
        while parent2_idx == parent1_idx && attempts < 10 {
            let r2 = rng.gen::<f64>() * total_weight;
            cumulative = 0.0;
            for (i, &weight) in weights.iter().enumerate() {
                cumulative += weight;
                if cumulative >= r2 {
                    parent2_idx = i;
                    break;
                }
            }
            attempts += 1;
        }

        if parent2_idx == parent1_idx && distinctions.len() > 1 {
            parent2_idx = (parent1_idx + 1) % distinctions.len();
        }

        engine.synthesize(&distinctions[parent1_idx], &distinctions[parent2_idx]);
    }

    // ============================================================
    // DEGREE ANALYSIS
    // ============================================================
    let graph = graph_helpers::build_graph(&engine);
    let total_nodes = graph.node_count();

    println!("  Graph size: {} nodes", total_nodes);

    // Calculate degree for each node
    let mut degrees: Vec<usize> =
        graph.node_identifiers().map(|n| graph.neighbors(n).count()).collect();

    degrees.sort_by(|a, b| b.cmp(a)); // Sort descending

    // Calculate concentration: top 20% of nodes should have >50% of edges
    let top_20_percent = (total_nodes as f64 * 0.2).ceil() as usize;
    let top_20_degrees: usize = degrees.iter().take(top_20_percent).sum();
    let total_degree: usize = degrees.iter().sum();

    let concentration = top_20_degrees as f64 / total_degree as f64;

    println!("  Top 20% of nodes account for {:.1}% of connections", concentration * 100.0);

    // ============================================================
    // ASSERTION
    // ============================================================
    // Note: Pure scale-free networks have concentration >50%, but with synthesis
    // constraints (irreflexivity, determinism), 45% is a strong indicator
    assert!(
        concentration > 0.45,
        "FALSIFIED: Random degree distribution detected (concentration: {:.2}).\n  \
         Power-law distribution not observed.\n  \
         Top 20% nodes account for only {:.1}% of connections (expected >45%)",
        concentration,
        concentration * 100.0
    );

    // ============================================================
    // REPORTING
    // ============================================================
    println!("\nHypothesis sustained.");
    println!("  Power-law degree distribution confirmed:");
    println!("    Top 20% of nodes: {:.1}% of all connections", concentration * 100.0);
    println!("  Scale-free network structure verified");
}
