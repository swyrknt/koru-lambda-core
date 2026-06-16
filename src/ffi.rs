/// FFI Layer - Universal C API for Go/Kotlin/Swift Bindings
///
/// This module exposes the Rust core as a pure C-compatible API, enabling
/// cross-language interoperability without runtime dependencies.
///
/// Uses direct pointer operations with zero overhead. Caller owns all allocated
/// memory with explicit free functions. All operations are thread-safe via
/// Arc/DashMap. All functions are pure or explicitly mutate via pointers.
use crate::{
    BatchCommitment, ConsensusValidator, Distinction, DistinctionEngine, NetworkAgent,
    PeerIdentity, TransactionBatch,
};
use std::ffi::{c_char, c_void, CStr, CString};
use std::slice;
use std::sync::Arc;

// ============================================================================
// OPAQUE TYPES - Hide Rust internals from C ABI
// ============================================================================

/// Opaque pointer to DistinctionEngine
pub type KoruEngine = c_void;

/// Opaque pointer to NetworkAgent
pub type KoruAgent = c_void;

/// Opaque pointer to ConsensusValidator
pub type KoruValidator = c_void;

// ============================================================================
// ERROR CODES
// ============================================================================

pub const KORU_SUCCESS: i32 = 0;
pub const KORU_ERROR_NULL_POINTER: i32 = -1;
pub const KORU_ERROR_INVALID_DATA: i32 = -2;
pub const KORU_ERROR_BATCH_REJECTED: i32 = -3;
pub const KORU_ERROR_UTF8: i32 = -4;

// ============================================================================
// ENGINE MANAGEMENT
// ============================================================================

/// Create a new DistinctionEngine
///
/// Returns: Opaque pointer to engine (must be freed with koru_engine_free)
///
/// # Safety
/// Caller must call koru_engine_free when done
#[no_mangle]
pub extern "C" fn koru_engine_new() -> *mut KoruEngine {
    let engine = Arc::new(DistinctionEngine::new());
    Arc::into_raw(engine) as *mut KoruEngine
}

/// Free a DistinctionEngine
///
/// # Safety
/// Pointer must be valid and not used after this call
#[no_mangle]
pub unsafe extern "C" fn koru_engine_free(engine: *mut KoruEngine) {
    if !engine.is_null() {
        let _ = Arc::from_raw(engine as *const DistinctionEngine);
    }
}

/// Get distinction count from engine
///
/// # Safety
/// engine must be valid pointer
#[no_mangle]
pub unsafe extern "C" fn koru_engine_distinction_count(engine: *const KoruEngine) -> usize {
    if engine.is_null() {
        return 0;
    }
    let engine = &*(engine as *const DistinctionEngine);
    engine.distinction_count()
}

/// Get relationship count from engine
///
/// # Safety
/// engine must be valid pointer
#[no_mangle]
pub unsafe extern "C" fn koru_engine_relationship_count(engine: *const KoruEngine) -> usize {
    if engine.is_null() {
        return 0;
    }
    let engine = &*(engine as *const DistinctionEngine);
    engine.relationship_count()
}

// ============================================================================
// NETWORK AGENT MANAGEMENT
// ============================================================================

/// Create a new NetworkAgent
///
/// Returns: Opaque pointer to agent (must be freed with koru_agent_free)
///
/// # Safety
/// engine must be valid pointer from koru_engine_new. Caller must free returned pointer.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_new(engine: *const KoruEngine) -> *mut KoruAgent {
    if engine.is_null() {
        return std::ptr::null_mut();
    }

    let engine_arc = Arc::from_raw(engine as *const DistinctionEngine);
    let agent = Box::new(NetworkAgent::new(&engine_arc));

    // Re-increment ref count (we borrowed it)
    let _ = Arc::into_raw(engine_arc);

    Box::into_raw(agent) as *mut KoruAgent
}

/// Free a NetworkAgent
///
/// # Safety
/// Pointer must be valid and not used after this call
#[no_mangle]
pub unsafe extern "C" fn koru_agent_free(agent: *mut KoruAgent) {
    if !agent.is_null() {
        let _ = Box::from_raw(agent as *mut NetworkAgent);
    }
}

/// Get current epoch from agent
///
/// # Safety
/// agent must be valid pointer
#[no_mangle]
pub unsafe extern "C" fn koru_agent_current_epoch(agent: *const KoruAgent) -> u64 {
    if agent.is_null() {
        return 0;
    }
    let agent = &*(agent as *const NetworkAgent);
    agent.current_epoch()
}

/// Get validator count from agent
///
/// # Safety
/// agent must be valid pointer
#[no_mangle]
pub unsafe extern "C" fn koru_agent_validator_count(agent: *const KoruAgent) -> usize {
    if agent.is_null() {
        return 0;
    }
    let agent = &*(agent as *const NetworkAgent);
    agent.validator_count()
}

/// Get current consensus state root as a 32-character lowercase hex C string.
///
/// The returned string is the hex encoding of the 16-byte canonical
/// Distinction ID held by the agent's consensus validator. The C signature
/// is unchanged from v1.2.0; the internal path is now explicitly
/// `as_bytes()` + `hex::encode` rather than relying on a cached String
/// inside `Distinction`.
///
/// # Safety
/// agent must be a valid pointer. Caller must free returned string with
/// `koru_free_string`.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_state_root(agent: *const KoruAgent) -> *mut c_char {
    if agent.is_null() {
        return std::ptr::null_mut();
    }

    let agent = &*(agent as *const NetworkAgent);
    // Bytes-on-wire architecture (DECISION 5.5): the engine produces a
    // canonical [u8; 16]; the FFI human surface emits it as 32-char hex.
    let hex_root = agent.consensus_state_root();
    debug_assert_eq!(hex_root.len(), 32, "hex root must be 32 chars");

    match CString::new(hex_root) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Get expected transaction nonce from agent's validator
///
/// # Safety
/// agent must be valid pointer
#[no_mangle]
pub unsafe extern "C" fn koru_agent_expected_nonce(agent: *const KoruAgent) -> u64 {
    if agent.is_null() {
        return 0;
    }
    let agent = &*(agent as *const NetworkAgent);
    agent.consensus_validator_expected_nonce()
}

/// Restore the agent's internal validator to a previously-persisted
/// state. Replaces the v1.2.0 `koru_agent_restore_nonce` setter.
///
/// V6 (CHECKLIST 1.6 / Phase 6 sub-branch #7). The legacy entry point
/// allowed `(root, nonce)` to be set independently — operators could
/// install a nonce that did not match the agent's current root,
/// admitting forged batches at startup. The new entry point requires
/// both the previously-persisted root (32-char lowercase hex) and
/// the matching nonce, and rejects fabricated roots that are not
/// registered in the supplied engine.
///
/// # Parameters
/// - agent: NetworkAgent pointer
/// - engine: Engine pointer (root must be registered here)
/// - root_hex: 32-char lowercase hex distinction id (null-terminated)
/// - nonce: expected nonce paired with `root_hex`
///
/// # Returns
/// `KORU_SUCCESS` on success, `KORU_ERROR_INVALID_DATA` if the root
/// is malformed or unregistered, or the appropriate null/utf8 error.
///
/// # Safety
/// agent, engine, and root_hex must be valid non-null pointers.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_restore_state(
    agent: *mut KoruAgent,
    engine: *const KoruEngine,
    root_hex: *const c_char,
    nonce: u64,
) -> i32 {
    if agent.is_null() || engine.is_null() || root_hex.is_null() {
        return KORU_ERROR_NULL_POINTER;
    }

    let agent = &mut *(agent as *mut NetworkAgent);
    let engine_arc = Arc::from_raw(engine as *const DistinctionEngine);

    let root_str = match CStr::from_ptr(root_hex).to_str() {
        Ok(s) => s,
        Err(_) => {
            let _ = Arc::into_raw(engine_arc);
            return KORU_ERROR_UTF8;
        },
    };

    let root_id = match Distinction::from_hex(root_str) {
        Ok(d) => d,
        Err(_) => {
            let _ = Arc::into_raw(engine_arc);
            return KORU_ERROR_INVALID_DATA;
        },
    };

    let code = match agent.restore_consensus_validator_state(&engine_arc, root_id, nonce) {
        Ok(()) => KORU_SUCCESS,
        Err(_) => KORU_ERROR_INVALID_DATA,
    };

    let _ = Arc::into_raw(engine_arc);
    code
}

// ============================================================================
// PEER MANAGEMENT
// ============================================================================

/// Join a peer to the validator set
///
/// # Safety
/// agent and engine must be valid pointers. peer_id must be valid UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_join_peer(
    agent: *mut KoruAgent,
    engine: *const KoruEngine,
    peer_id: *const c_char,
) -> i32 {
    if agent.is_null() || engine.is_null() || peer_id.is_null() {
        return KORU_ERROR_NULL_POINTER;
    }

    let agent = &mut *(agent as *mut NetworkAgent);
    let engine_arc = Arc::from_raw(engine as *const DistinctionEngine);

    let peer_id_str = match CStr::from_ptr(peer_id).to_str() {
        Ok(s) => s,
        Err(_) => {
            // Re-increment ref count before returning
            let _ = Arc::into_raw(engine_arc);
            return KORU_ERROR_UTF8;
        },
    };

    let peer = match PeerIdentity::new(peer_id_str.to_string(), &engine_arc) {
        Ok(p) => p,
        Err(_) => {
            // N1/N2: empty or oversized peer ids are rejected before
            // any synth runs into the engine.
            let _ = Arc::into_raw(engine_arc);
            return KORU_ERROR_INVALID_DATA;
        },
    };
    agent.join_peer(peer, &engine_arc);

    // Re-increment ref count (we borrowed it)
    let _ = Arc::into_raw(engine_arc);

    KORU_SUCCESS
}

// ============================================================================
// COMMITMENT PROTOCOL (Two-Stage Gossip)
// ============================================================================

/// STAGE 1: Propose Commitment (Leader Action)
///
/// Leader computes lightweight commitment hash, caches batch, returns commitment.
/// Runtime broadcasts the returned commitment (80 bytes) via gossip.
///
/// # Parameters
/// - agent: NetworkAgent pointer
/// - engine: DistinctionEngine pointer
/// - batch_data: Raw byte buffer containing serialized TransactionBatch
/// - batch_len: Length of batch_data buffer
/// - out_commitment: Output buffer (must be 32 bytes) for commitment hash
///
/// # Returns
/// KORU_SUCCESS on success, error code on failure
///
/// # Safety
/// All pointers must be valid. out_commitment must point to 32-byte buffer.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_propose_commitment(
    agent: *mut KoruAgent,
    engine: *const KoruEngine,
    batch_data: *const u8,
    batch_len: usize,
    out_commitment: *mut u8,
) -> i32 {
    if agent.is_null() || engine.is_null() || batch_data.is_null() || out_commitment.is_null() {
        return KORU_ERROR_NULL_POINTER;
    }

    let agent = &mut *(agent as *mut NetworkAgent);
    let engine_arc = Arc::from_raw(engine as *const DistinctionEngine);

    // Deserialize batch from bytes
    let batch_bytes = slice::from_raw_parts(batch_data, batch_len);
    let batch: TransactionBatch = match serde_json::from_slice(batch_bytes) {
        Ok(b) => b,
        Err(_) => {
            let _ = Arc::into_raw(engine_arc);
            return KORU_ERROR_INVALID_DATA;
        },
    };

    // Propose commitment (Stage 1)
    let commitment = match agent.propose_commitment(batch, &engine_arc) {
        Ok(c) => c,
        Err(_) => {
            let _ = Arc::into_raw(engine_arc);
            return KORU_ERROR_BATCH_REJECTED;
        },
    };

    // Copy commitment hash to output buffer
    std::ptr::copy_nonoverlapping(commitment.commitment_hash.as_ptr(), out_commitment, 32);

    // Re-increment ref count
    let _ = Arc::into_raw(engine_arc);
    KORU_SUCCESS
}

/// STAGE 1: Check Commitment (Light Node "Ping" Check)
///
/// Verifies commitment matches expected nonce/epoch WITHOUT downloading batch.
/// This is how light clients participate in consensus efficiently.
///
/// # Parameters
/// - agent: NetworkAgent pointer
/// - commitment_hash: 32-byte commitment hash
/// - expected_nonce: Expected nonce for next transaction
/// - expected_epoch: Expected current epoch
///
/// # Returns
/// 1 if valid, 0 if invalid, negative on error
///
/// # Safety
/// agent must be valid. commitment_hash must point to 32-byte buffer.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_check_commitment(
    agent: *const KoruAgent,
    commitment_hash: *const u8,
    expected_nonce: u64,
    expected_epoch: u64,
) -> i32 {
    if agent.is_null() || commitment_hash.is_null() {
        return KORU_ERROR_NULL_POINTER;
    }

    let agent = &*(agent as *const NetworkAgent);

    // Reconstruct commitment for verification
    let mut hash = [0u8; 32];
    std::ptr::copy_nonoverlapping(commitment_hash, hash.as_mut_ptr(), 32);

    // Create minimal commitment for verification
    let commitment = BatchCommitment {
        commitment_hash: hash,
        nonce: expected_nonce,
        epoch: expected_epoch,
        leader_id: String::new(), // Not needed for verification
        batch_size: 0,            // Not needed for verification
    };

    // Check commitment
    if agent.check_commitment(&commitment) {
        1 // Valid
    } else {
        0 // Invalid
    }
}

/// STAGE 2: Finalize Batch (Full Validator Execution)
///
/// Applies full batch after fetching data and verifying it matches commitment.
/// Called by full validators after downloading batch data from peers.
///
/// # Parameters
/// - agent: NetworkAgent pointer
/// - engine: DistinctionEngine pointer
/// - batch_data: Raw byte buffer containing serialized TransactionBatch
/// - batch_len: Length of batch_data buffer
/// - commitment_hash: 32-byte commitment hash (must match batch)
///
/// # Returns
/// KORU_SUCCESS on success, error code on failure
///
/// # Safety
/// All pointers must be valid. commitment_hash must point to 32-byte buffer.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_finalize_batch(
    agent: *mut KoruAgent,
    engine: *const KoruEngine,
    batch_data: *const u8,
    batch_len: usize,
    commitment_hash: *const u8,
) -> i32 {
    if agent.is_null() || engine.is_null() || batch_data.is_null() || commitment_hash.is_null() {
        return KORU_ERROR_NULL_POINTER;
    }

    let agent = &mut *(agent as *mut NetworkAgent);
    let engine_arc = Arc::from_raw(engine as *const DistinctionEngine);

    // Deserialize batch from bytes
    let batch_bytes = slice::from_raw_parts(batch_data, batch_len);
    let batch: TransactionBatch = match serde_json::from_slice(batch_bytes) {
        Ok(b) => b,
        Err(_) => {
            let _ = Arc::into_raw(engine_arc);
            return KORU_ERROR_INVALID_DATA;
        },
    };

    // Copy commitment hash
    let mut hash = [0u8; 32];
    std::ptr::copy_nonoverlapping(commitment_hash, hash.as_mut_ptr(), 32);

    // Finalize batch (Stage 2)
    let result = match agent.finalize_batch(batch, hash, &engine_arc) {
        Ok(_) => KORU_SUCCESS,
        Err(_) => KORU_ERROR_BATCH_REJECTED,
    };

    // Re-increment ref count
    let _ = Arc::into_raw(engine_arc);
    result
}

// ============================================================================
// DEPRECATED: Old Synchronous Batch Proposal
// ============================================================================

/// DEPRECATED: Propose a batch for validation
///
/// This function bypasses the two-stage commitment protocol.
/// Use koru_agent_propose_commitment() + koru_agent_finalize_batch() instead.
///
/// # Safety
/// agent, engine, and batch_json must be valid pointers
#[deprecated(note = "Use koru_agent_propose_commitment and koru_agent_finalize_batch")]
#[no_mangle]
pub unsafe extern "C" fn koru_agent_propose_batch(
    agent: *mut KoruAgent,
    engine: *const KoruEngine,
    batch_json: *const c_char,
) -> i32 {
    if agent.is_null() || engine.is_null() || batch_json.is_null() {
        return KORU_ERROR_NULL_POINTER;
    }

    let agent = &mut *(agent as *mut NetworkAgent);
    let engine_arc = Arc::from_raw(engine as *const DistinctionEngine);

    let batch_str = match CStr::from_ptr(batch_json).to_str() {
        Ok(s) => s,
        Err(_) => {
            let _ = Arc::into_raw(engine_arc);
            return KORU_ERROR_UTF8;
        },
    };

    let batch: TransactionBatch = match serde_json::from_str(batch_str) {
        Ok(b) => b,
        Err(_) => {
            let _ = Arc::into_raw(engine_arc);
            return KORU_ERROR_INVALID_DATA;
        },
    };

    // Two-stage commit: propose_commitment() + finalize_batch()
    let result = match agent.propose_commitment(batch.clone(), &engine_arc) {
        Ok(commitment) => {
            match agent.finalize_batch(batch, commitment.commitment_hash, &engine_arc) {
                Ok(_) => KORU_SUCCESS,
                Err(_) => KORU_ERROR_BATCH_REJECTED,
            }
        },
        Err(_) => KORU_ERROR_BATCH_REJECTED,
    };

    // Re-increment ref count
    let _ = Arc::into_raw(engine_arc);
    result
}

/// Advance epoch (triggers leader rotation)
///
/// # Safety
/// agent and engine must be valid pointers
#[no_mangle]
pub unsafe extern "C" fn koru_agent_advance_epoch(
    agent: *mut KoruAgent,
    engine: *const KoruEngine,
) -> i32 {
    if agent.is_null() || engine.is_null() {
        return KORU_ERROR_NULL_POINTER;
    }

    let agent = &mut *(agent as *mut NetworkAgent);
    let engine_arc = Arc::from_raw(engine as *const DistinctionEngine);

    agent.advance_epoch(&engine_arc);

    // Re-increment ref count
    let _ = Arc::into_raw(engine_arc);
    KORU_SUCCESS
}

/// Get current leader ID (returns allocated C string)
///
/// # Safety
/// agent must be valid pointer. Caller must free returned string with koru_free_string.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_get_leader(agent: *const KoruAgent) -> *mut c_char {
    if agent.is_null() {
        return std::ptr::null_mut();
    }

    let agent = &*(agent as *const NetworkAgent);

    match agent.get_current_leader() {
        Some(leader) => match CString::new(leader.id.clone()) {
            Ok(s) => s.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

// ============================================================================
// MEMORY MANAGEMENT UTILITIES
// ============================================================================

/// Free a C string allocated by this library
///
/// # Safety
/// s must be a string allocated by this library (e.g., from koru_agent_state_root)
#[no_mangle]
pub unsafe extern "C" fn koru_free_string(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

// ============================================================================
// CONSENSUS VALIDATOR FFI
// ============================================================================

/// Create a new ConsensusValidator
///
/// # Safety
/// engine must be valid pointer. Caller must free returned pointer.
#[no_mangle]
pub unsafe extern "C" fn koru_validator_new(engine: *const KoruEngine) -> *mut KoruValidator {
    if engine.is_null() {
        return std::ptr::null_mut();
    }

    let engine_arc = Arc::from_raw(engine as *const DistinctionEngine);
    let validator = Box::new(ConsensusValidator::new(&engine_arc));

    // Re-increment ref count
    let _ = Arc::into_raw(engine_arc);

    Box::into_raw(validator) as *mut KoruValidator
}

/// Free a ConsensusValidator
///
/// # Safety
/// Pointer must be valid and not used after this call
#[no_mangle]
pub unsafe extern "C" fn koru_validator_free(validator: *mut KoruValidator) {
    if !validator.is_null() {
        let _ = Box::from_raw(validator as *mut ConsensusValidator);
    }
}

/// Get expected nonce from validator
///
/// # Safety
/// validator must be valid pointer
#[no_mangle]
pub unsafe extern "C" fn koru_validator_expected_nonce(validator: *const KoruValidator) -> u64 {
    if validator.is_null() {
        return 0;
    }
    let validator = &*(validator as *const ConsensusValidator);
    validator.expected_nonce()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_engine_lifecycle() {
        unsafe {
            let engine = koru_engine_new();
            assert!(!engine.is_null());

            let count = koru_engine_distinction_count(engine);
            assert_eq!(count, 2); // d0 and d1

            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_agent_lifecycle() {
        unsafe {
            let engine = koru_engine_new();
            let agent = koru_agent_new(engine);
            assert!(!agent.is_null());

            let epoch = koru_agent_current_epoch(agent);
            assert_eq!(epoch, 0);

            koru_agent_free(agent);
            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_string_allocation() {
        unsafe {
            let engine = koru_engine_new();
            let agent = koru_agent_new(engine);

            let root_str = koru_agent_state_root(agent);
            assert!(!root_str.is_null());

            // Verify it's valid UTF-8
            let root_cstr = CStr::from_ptr(root_str);
            assert!(root_cstr.to_str().is_ok());

            koru_free_string(root_str);
            koru_agent_free(agent);
            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_propose_commitment() {
        unsafe {
            let engine = koru_engine_new();
            let agent = koru_agent_new(engine);

            // Create a valid TransactionBatch JSON
            let batch = r#"{"transactions":[{"nonce":0,"data":[1,2,3]},{"nonce":1,"data":[4,5,6]}],"previous_root":"genesis"}"#;
            let batch_bytes = batch.as_bytes();

            // Buffer for commitment hash (32 bytes)
            let mut commitment_hash = [0u8; 32];

            // Propose commitment
            let result = koru_agent_propose_commitment(
                agent,
                engine,
                batch_bytes.as_ptr(),
                batch_bytes.len(),
                commitment_hash.as_mut_ptr(),
            );

            assert_eq!(result, KORU_SUCCESS);

            // Verify hash is non-zero (was actually computed)
            assert_ne!(commitment_hash, [0u8; 32]);

            koru_agent_free(agent);
            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_check_commitment() {
        unsafe {
            let engine = koru_engine_new();
            let agent = koru_agent_new(engine);

            // Propose a commitment first
            let batch = r#"{"transactions":[{"nonce":0,"data":[1,2,3]},{"nonce":1,"data":[4,5,6]}],"previous_root":"genesis"}"#;
            let batch_bytes = batch.as_bytes();
            let mut commitment_hash = [0u8; 32];

            koru_agent_propose_commitment(
                agent,
                engine,
                batch_bytes.as_ptr(),
                batch_bytes.len(),
                commitment_hash.as_mut_ptr(),
            );

            // Check with correct nonce and epoch (should succeed)
            let result = koru_agent_check_commitment(
                agent,
                commitment_hash.as_ptr(),
                0, // expected_nonce
                0, // expected_epoch
            );
            assert_eq!(result, 1); // Valid

            // Check with incorrect nonce (should fail)
            let result_bad = koru_agent_check_commitment(
                agent,
                commitment_hash.as_ptr(),
                999, // wrong nonce
                0,
            );
            assert_eq!(result_bad, 0); // Invalid

            koru_agent_free(agent);
            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_finalize_batch() {
        unsafe {
            let engine = koru_engine_new();
            let agent = koru_agent_new(engine);

            // Get current state root to use as previous_root
            let root_str = koru_agent_state_root(agent);
            let root_cstr = CStr::from_ptr(root_str);
            let root = root_cstr.to_str().unwrap();

            // Stage 1: Propose commitment
            let batch = format!(
                r#"{{"transactions":[{{"nonce":0,"data":[1,2,3]}},{{"nonce":1,"data":[4,5,6]}}],"previous_root":"{}"}}"#,
                root
            );
            let batch_bytes = batch.as_bytes();
            let mut commitment_hash = [0u8; 32];

            let result = koru_agent_propose_commitment(
                agent,
                engine,
                batch_bytes.as_ptr(),
                batch_bytes.len(),
                commitment_hash.as_mut_ptr(),
            );
            assert_eq!(result, KORU_SUCCESS);

            // Stage 2: Finalize batch
            let result = koru_agent_finalize_batch(
                agent,
                engine,
                batch_bytes.as_ptr(),
                batch_bytes.len(),
                commitment_hash.as_ptr(),
            );
            assert_eq!(result, KORU_SUCCESS);

            koru_free_string(root_str);
            koru_agent_free(agent);
            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_commitment_two_stage_flow() {
        unsafe {
            // This test simulates the full two-stage commitment protocol:
            // 1. Leader proposes commitment (Stage 1 - gossip)
            // 2. Validators check commitment without downloading batch
            // 3. Full validators finalize batch (Stage 2 - lazy fetch)

            let engine = koru_engine_new();
            let leader = koru_agent_new(engine);

            // Get current state root for the batch
            let root_str = koru_agent_state_root(leader);
            let root_cstr = CStr::from_ptr(root_str);
            let root = root_cstr.to_str().unwrap();

            // Leader: Stage 1 - Propose commitment
            let batch = format!(
                r#"{{"transactions":[{{"nonce":0,"data":[1,2,3]}},{{"nonce":1,"data":[4,5,6]}},{{"nonce":2,"data":[7,8,9]}}],"previous_root":"{}"}}"#,
                root
            );
            let batch_bytes = batch.as_bytes();
            let mut commitment_hash = [0u8; 32];

            let result = koru_agent_propose_commitment(
                leader,
                engine,
                batch_bytes.as_ptr(),
                batch_bytes.len(),
                commitment_hash.as_mut_ptr(),
            );
            assert_eq!(result, KORU_SUCCESS);

            // Light node: Verify commitment without downloading batch
            let is_valid = koru_agent_check_commitment(
                leader,
                commitment_hash.as_ptr(),
                0, // expected_nonce
                0, // expected_epoch
            );
            assert_eq!(is_valid, 1);

            // Full validator: Stage 2 - Fetch and finalize batch
            let result = koru_agent_finalize_batch(
                leader,
                engine,
                batch_bytes.as_ptr(),
                batch_bytes.len(),
                commitment_hash.as_ptr(),
            );
            assert_eq!(result, KORU_SUCCESS);

            koru_free_string(root_str);
            koru_agent_free(leader);
            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_agent_expected_nonce() {
        unsafe {
            let engine = koru_engine_new();
            let agent = koru_agent_new(engine);

            // Initial nonce should be 0
            let nonce = koru_agent_expected_nonce(agent);
            assert_eq!(nonce, 0);

            koru_agent_free(agent);
            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_agent_restore_state_at_genesis() {
        // V6: replaces test_ffi_agent_restore_nonce. The agent's
        // genesis root is registered in the engine, so restoring
        // (genesis_root, 42) is accepted and the nonce updates.
        unsafe {
            let engine = koru_engine_new();
            let agent = koru_agent_new(engine);

            assert_eq!(koru_agent_expected_nonce(agent), 0);

            let root_str = koru_agent_state_root(agent);
            let result = koru_agent_restore_state(agent, engine, root_str, 42);
            assert_eq!(result, KORU_SUCCESS);
            assert_eq!(koru_agent_expected_nonce(agent), 42);

            koru_free_string(root_str);
            koru_agent_free(agent);
            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_agent_restore_state_rejects_fabricated_root() {
        // V6: fabricated bytes (well-formed hex, never synthesized)
        // must be refused with KORU_ERROR_INVALID_DATA.
        unsafe {
            let engine = koru_engine_new();
            let agent = koru_agent_new(engine);

            let fabricated = CString::new("f".repeat(32)).unwrap();
            let result = koru_agent_restore_state(agent, engine, fabricated.as_ptr(), 100);
            assert_eq!(result, KORU_ERROR_INVALID_DATA);
            // Nonce must remain at 0.
            assert_eq!(koru_agent_expected_nonce(agent), 0);

            koru_agent_free(agent);
            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_null_pointer_safety() {
        unsafe {
            let nonce = koru_agent_expected_nonce(std::ptr::null());
            assert_eq!(nonce, 0);

            let root = CString::new("0".repeat(32)).unwrap();
            let result = koru_agent_restore_state(
                std::ptr::null_mut(),
                std::ptr::null(),
                root.as_ptr(),
                42,
            );
            assert_eq!(result, KORU_ERROR_NULL_POINTER);
        }
    }
}
