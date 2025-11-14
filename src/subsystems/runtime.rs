/// Async Network Runtime
///
/// Integrates libp2p for P2P networking with Tokio async runtime.
/// Implements LocalCausalAgent to track runtime-level network events
/// as a causal distinction chain.
///
/// Design Principles:
/// - **LocalCausalAgent** - Runtime maintains its own causal chain
/// - **Async First** - Built on Tokio runtime
/// - **P2P Native** - Uses libp2p for peer discovery and communication
/// - **Deterministic** - Network events feed into deterministic core
///
/// Architecture:
/// ```
/// NetworkRuntime (LocalCausalAgent)
///   ├── local_root: Distinction (runtime event chain)
///   ├── swarm: libp2p Swarm (async P2P)
///   └── agent: NetworkAgent (consensus logic)
/// ```

use crate::primitives::Canonicalizable;
use crate::subsystems::local_agent::{synthesize_causal_action, LocalCausalAgent};
use crate::subsystems::network::NetworkAgent;
use crate::subsystems::validator::TransactionBatch;
use crate::{Distinction, DistinctionEngine};
use futures::StreamExt;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{
    gossipsub, mdns, noise,
    swarm::SwarmEvent,
    tcp, yamux, PeerId, Swarm, SwarmBuilder,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Network behavior combining Gossipsub and mDNS
#[derive(NetworkBehaviour)]
pub struct DistinctionBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
}

/// Message types exchanged over the network
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum NetworkMessage {
    /// Canonical Batch Order from leader
    BatchProposal {
        epoch: u64,
        batch: TransactionBatch,
        leader_id: String,
    },
    /// Epoch advancement notification
    EpochAdvance {
        new_epoch: u64,
        validator_set_hash: String,
    },
    /// Structural audit proof request
    StateRequest {
        requesting_peer: String,
        last_known_root: String,
    },
    /// Structural audit proof response
    StateResponse {
        current_root: String,
        epoch: u64,
        validator_count: usize,
    },
}

/// Runtime-level actions (canonicalizable network events)
///
/// These represent events at the P2P runtime layer, distinct from
/// the consensus-level NetworkAgent actions.
#[derive(Debug, Clone)]
pub enum RuntimeAction {
    /// Batch received and validated
    BatchReceived { epoch: u64, leader_id: String },
    /// Peer discovered via mDNS
    PeerDiscovered { peer_id: String },
    /// Epoch advanced
    EpochAdvanced { new_epoch: u64 },
}

impl Canonicalizable for RuntimeAction {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction {
        match self {
            RuntimeAction::BatchReceived { epoch, leader_id } => {
                // Canonicalize epoch
                let epoch_bytes = epoch.to_le_bytes();
                let epoch_d = epoch_bytes
                    .iter()
                    .fold(engine.d0().clone(), |acc, &byte| {
                        let byte_d = byte.to_canonical_structure(engine);
                        engine.synthesize(&acc, &byte_d)
                    });

                // Canonicalize leader_id (first 8 bytes)
                let leader_bytes = leader_id.as_bytes();
                let leader_d = leader_bytes
                    .iter()
                    .take(8)
                    .fold(engine.d1().clone(), |acc, &byte| {
                        let byte_d = byte.to_canonical_structure(engine);
                        engine.synthesize(&acc, &byte_d)
                    });

                // Synthesize: epoch ⊕ leader
                engine.synthesize(&epoch_d, &leader_d)
            }

            RuntimeAction::PeerDiscovered { peer_id } => {
                // Canonicalize peer_id
                let peer_bytes = peer_id.as_bytes();
                peer_bytes.iter().take(16).fold(
                    engine.d1().clone(),
                    |acc, &byte| {
                        let byte_d = byte.to_canonical_structure(engine);
                        engine.synthesize(&acc, &byte_d)
                    },
                )
            }

            RuntimeAction::EpochAdvanced { new_epoch } => {
                // Canonicalize epoch value
                let epoch_bytes = new_epoch.to_le_bytes();
                epoch_bytes.iter().fold(engine.d0().clone(), |acc, &byte| {
                    let byte_d = byte.to_canonical_structure(engine);
                    engine.synthesize(&acc, &byte_d)
                })
            }
        }
    }
}

/// Async Network Runtime Implementation
///
/// Bridges async P2P networking (libp2p) with deterministic consensus (NetworkAgent).
/// Maintains its own causal chain of runtime-level events via LocalCausalAgent.
///
/// **LocalCausalAgent Pattern**:
/// - local_root: Tracks runtime event chain (peer discoveries, batch receipts)
/// - ActionData: RuntimeAction (peer/batch/epoch events)
/// - Synthesis: ΔNew = ΔRuntime ⊕ ΔEvent
pub struct NetworkRuntime {
    /// Runtime's local causal chain
    local_root: Distinction,
    /// libp2p Swarm
    swarm: Swarm<DistinctionBehaviour>,
    /// Core distinction engine
    engine: Arc<DistinctionEngine>,
    /// Network consensus agent
    agent: NetworkAgent,
    /// Inbound message channel
    message_rx: Option<mpsc::UnboundedReceiver<NetworkMessage>>,
    /// Outbound message channel (reserved for future use)
    _message_tx: mpsc::UnboundedSender<NetworkMessage>,
    /// Gossipsub topic for batches
    batch_topic: gossipsub::IdentTopic,
}

impl NetworkRuntime {
    /// Create a new async network runtime
    pub async fn new(
        engine: Arc<DistinctionEngine>,
        _listen_addr: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Runtime genesis: d0 ⊕ d1 (distinct from agent's genesis)
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let genesis = engine.synthesize(&d0, &d1);

        // Create a random PeerId
        let local_key = libp2p::identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());

        println!("Local peer id: {}", local_peer_id);

        // Create Gossipsub config
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(gossipsub::ValidationMode::Strict)
            .build()
            .expect("Valid gossipsub config");

        // Create topic for batch proposals
        let batch_topic = gossipsub::IdentTopic::new("distinction-batches");

        // Build swarm using new libp2p 0.54 API
        let swarm = SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| {
                let peer_id = key.public().to_peer_id();

                // Create Gossipsub
                let mut gossipsub = gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config.clone(),
                )
                .expect("Valid gossipsub config");

                // Subscribe to topic
                gossipsub
                    .subscribe(&batch_topic)
                    .expect("Subscribe to batch topic");

                // Create mDNS
                let mdns =
                    mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
                        .expect("Create mDNS");

                Ok(DistinctionBehaviour { gossipsub, mdns })
            })?
            .build();

        // Create message channels
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        // Create network agent
        let agent = NetworkAgent::new(&engine);

        Ok(Self {
            local_root: genesis,
            swarm,
            engine,
            agent,
            message_rx: Some(message_rx),
            _message_tx: message_tx,
            batch_topic,
        })
    }

    /// Start listening on the specified address
    pub fn listen(&mut self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let listen_addr: libp2p::Multiaddr = addr.parse()?;
        self.swarm.listen_on(listen_addr)?;
        Ok(())
    }

    /// Dial a peer
    pub fn dial(&mut self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let remote_addr: libp2p::Multiaddr = addr.parse()?;
        self.swarm.dial(remote_addr)?;
        Ok(())
    }

    /// Publish a batch proposal to the network
    pub fn publish_batch(
        &mut self,
        batch: TransactionBatch,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let message = NetworkMessage::BatchProposal {
            epoch: self.agent.current_epoch(),
            batch,
            leader_id: self.swarm.local_peer_id().to_string(),
        };

        let message_bytes = serde_json::to_vec(&message)?;
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(self.batch_topic.clone(), message_bytes)?;

        Ok(())
    }

    /// Main event loop - processes network events and feeds them to core logic
    pub async fn run(mut self) {
        let mut message_rx = self.message_rx.take().expect("Message receiver exists");

        loop {
            tokio::select! {
                // Handle swarm events (P2P)
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                }

                // Handle internal messages
                Some(message) = message_rx.recv() => {
                    self.handle_internal_message(message).await;
                }
            }
        }
    }

    /// Handle swarm events from libp2p
    async fn handle_swarm_event(&mut self, event: SwarmEvent<DistinctionBehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(DistinctionBehaviourEvent::Mdns(mdns::Event::Discovered(
                peers,
            ))) => {
                for (peer_id, addr) in peers {
                    println!("Discovered peer: {} at {}", peer_id, addr);
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .add_explicit_peer(&peer_id);

                    // Synthesize peer discovery into runtime chain
                    let action = RuntimeAction::PeerDiscovered {
                        peer_id: peer_id.to_string(),
                    };
                    let engine = Arc::clone(&self.engine);
                    let _new_root = self.synthesize_action(action, &engine);
                }
            }

            SwarmEvent::Behaviour(DistinctionBehaviourEvent::Mdns(mdns::Event::Expired(
                peers,
            ))) => {
                for (peer_id, _) in peers {
                    println!("Peer expired: {}", peer_id);
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);
                }
            }

            SwarmEvent::Behaviour(DistinctionBehaviourEvent::Gossipsub(
                gossipsub::Event::Message {
                    propagation_source,
                    message,
                    ..
                },
            )) => {
                self.handle_gossipsub_message(propagation_source, message.data)
                    .await;
            }

            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on: {}", address);
            }

            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("Connected to peer: {}", peer_id);
            }

            _ => {}
        }
    }

    /// Handle gossipsub messages (batch proposals, epoch changes)
    async fn handle_gossipsub_message(&mut self, _peer: PeerId, data: Vec<u8>) {
        match serde_json::from_slice::<NetworkMessage>(&data) {
            Ok(NetworkMessage::BatchProposal {
                epoch,
                batch,
                leader_id,
            }) => {
                println!(
                    "Received batch from leader {} (epoch {})",
                    leader_id, epoch
                );

                // Validate batch using deterministic consensus agent
                match self.agent.propose_batch(batch, &self.engine) {
                    Ok(new_root) => {
                        println!("✅ Batch validated: {}", new_root.id());

                        // Synthesize batch receipt into runtime chain
                        let action = RuntimeAction::BatchReceived {
                            epoch,
                            leader_id: leader_id.clone(),
                        };
                        let engine = Arc::clone(&self.engine);
                        let _runtime_root = self.synthesize_action(action, &engine);
                    }
                    Err(e) => {
                        println!("❌ Batch rejected: {}", e);
                    }
                }
            }

            Ok(NetworkMessage::EpochAdvance {
                new_epoch,
                validator_set_hash,
            }) => {
                println!(
                    "Epoch advanced to {} (validator set: {})",
                    new_epoch, validator_set_hash
                );

                // Advance consensus epoch
                let _consensus_root = self.agent.advance_epoch(&self.engine);

                // Synthesize epoch advancement into runtime chain
                let action = RuntimeAction::EpochAdvanced { new_epoch };
                let engine = Arc::clone(&self.engine);
                let _runtime_root = self.synthesize_action(action, &engine);
            }

            Ok(NetworkMessage::StateRequest {
                requesting_peer,
                last_known_root,
            }) => {
                println!(
                    "State request from {} (last known: {})",
                    requesting_peer, last_known_root
                );
                // Could respond with current state
            }

            Ok(NetworkMessage::StateResponse {
                current_root,
                epoch,
                validator_count,
            }) => {
                println!(
                    "State response: root={}, epoch={}, validators={}",
                    current_root, epoch, validator_count
                );
            }

            Err(e) => {
                println!("Failed to deserialize message: {}", e);
            }
        }
    }

    /// Handle internal messages
    async fn handle_internal_message(&mut self, message: NetworkMessage) {
        println!("Internal message: {:?}", message);
    }

    /// Get current consensus state
    pub fn get_state(&self) -> (String, u64) {
        (
            self.agent.consensus_state_root().to_string(),
            self.agent.current_epoch(),
        )
    }

    /// Get runtime state root (distinct from consensus state)
    pub fn runtime_root_id(&self) -> &str {
        self.local_root.id()
    }
}

/// LocalCausalAgent Implementation
///
/// The runtime maintains its own causal chain tracking P2P-level events
/// (peer discoveries, batch receipts, epoch changes) distinct from the
/// consensus-level NetworkAgent chain.
impl LocalCausalAgent for NetworkRuntime {
    type ActionData = RuntimeAction;

    fn get_current_root(&self) -> &Distinction {
        &self.local_root
    }

    fn synthesize_action(
        &mut self,
        action_data: Self::ActionData,
        engine: &Arc<DistinctionEngine>,
    ) -> Distinction {
        // ΔNew = ΔRuntime ⊕ ΔEvent
        let new_root = synthesize_causal_action(&self.local_root, action_data, engine);
        self.local_root = new_root.clone();
        new_root
    }

    fn update_local_root(&mut self, new_root: Distinction) {
        self.local_root = new_root;
    }
}

/// Helper to create message channel
pub fn create_message_channel() -> (
    mpsc::UnboundedSender<NetworkMessage>,
    mpsc::UnboundedReceiver<NetworkMessage>,
) {
    mpsc::unbounded_channel()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_action_peer_discovered_canonical() {
        let engine = Arc::new(DistinctionEngine::new());
        let action = RuntimeAction::PeerDiscovered {
            peer_id: "12D3KooWTest".to_string(),
        };

        let d1 = action.to_canonical_structure(&engine);
        let d2 = action.to_canonical_structure(&engine);

        // Canonicalization is deterministic
        assert_eq!(d1.id(), d2.id());
        assert!(!d1.id().is_empty());
    }

    #[test]
    fn test_runtime_action_batch_received_canonical() {
        let engine = Arc::new(DistinctionEngine::new());
        let action = RuntimeAction::BatchReceived {
            epoch: 42,
            leader_id: "node_1".to_string(),
        };

        let d1 = action.to_canonical_structure(&engine);
        let d2 = action.to_canonical_structure(&engine);

        // Deterministic canonicalization
        assert_eq!(d1.id(), d2.id());

        // Different epoch produces different distinction
        let action2 = RuntimeAction::BatchReceived {
            epoch: 43,
            leader_id: "node_1".to_string(),
        };
        let d3 = action2.to_canonical_structure(&engine);
        assert_ne!(d1.id(), d3.id());
    }

    #[test]
    fn test_runtime_action_epoch_advanced_canonical() {
        let engine = Arc::new(DistinctionEngine::new());
        let action = RuntimeAction::EpochAdvanced { new_epoch: 100 };

        let d = action.to_canonical_structure(&engine);
        assert!(!d.id().is_empty());

        // Same epoch produces same distinction
        let action2 = RuntimeAction::EpochAdvanced { new_epoch: 100 };
        let d2 = action2.to_canonical_structure(&engine);
        assert_eq!(d.id(), d2.id());
    }

    #[test]
    fn test_network_message_serialization() {
        use crate::subsystems::validator::{TransactionAction, TransactionBatch};

        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3],
            }],
            previous_root: "root_123".to_string(),
        };

        let msg = NetworkMessage::BatchProposal {
            epoch: 5,
            batch: batch.clone(),
            leader_id: "node_1".to_string(),
        };

        // Serialize and deserialize
        let json = serde_json::to_string(&msg).expect("Serialize");
        let msg2: NetworkMessage = serde_json::from_str(&json).expect("Deserialize");

        // Verify round-trip
        match (msg, msg2) {
            (
                NetworkMessage::BatchProposal {
                    epoch: e1,
                    batch: b1,
                    leader_id: l1,
                },
                NetworkMessage::BatchProposal {
                    epoch: e2,
                    batch: b2,
                    leader_id: l2,
                },
            ) => {
                assert_eq!(e1, e2);
                assert_eq!(l1, l2);
                assert_eq!(b1.transactions.len(), b2.transactions.len());
                assert_eq!(b1.previous_root, b2.previous_root);
            }
            _ => panic!("Message type mismatch"),
        }
    }

    #[test]
    fn test_message_channel_creation() {
        let (tx, mut rx) = create_message_channel();

        let msg = NetworkMessage::EpochAdvance {
            new_epoch: 10,
            validator_set_hash: "hash_abc".to_string(),
        };

        tx.send(msg.clone()).expect("Send message");
        let received = rx.try_recv().expect("Receive message");

        match (msg, received) {
            (
                NetworkMessage::EpochAdvance {
                    new_epoch: e1,
                    validator_set_hash: h1,
                },
                NetworkMessage::EpochAdvance {
                    new_epoch: e2,
                    validator_set_hash: h2,
                },
            ) => {
                assert_eq!(e1, e2);
                assert_eq!(h1, h2);
            }
            _ => panic!("Message mismatch"),
        }
    }
}
