/// FFI Layer — Universal C API for Go / Kotlin / Swift bindings.
///
/// This module exposes the Rust core as a pure C-compatible API, enabling
/// cross-language interoperability without runtime dependencies.
///
/// # Memory ownership
///
/// Caller owns all allocated memory. Every `*_new` constructor pairs with
/// a `*_free` destructor. Pointers must not be used after the matching
/// `*_free` call.
///
/// # Concurrency contract (CHECKLIST 1.7 F2 / F8, Decision 5.4)
///
/// * The `DistinctionEngine` behind a `KoruEngine` handle is internally
///   thread-safe (`&self` everywhere via `DashMap`). Multiple C threads
///   may share a single engine handle.
/// * The `NetworkAgent` and `ConsensusValidator` behind `KoruAgent` /
///   `KoruValidator` handles are NOT inherently thread-safe — they
///   were `&mut self` types. v1.2.0 documented this as "caller must
///   serialize" but the C ABI gave no way to enforce it; concurrent
///   calls from C threads would create `&mut` aliases and trigger UB.
/// * v2.0 wraps both in `Box<Mutex<...>>` inside the FFI boundary.
///   Concurrent C-thread calls targeting the same handle now serialize
///   through the internal mutex. The mutex acquisition is single-digit
///   nanoseconds on the hot path; if you need lock-free reads, hold
///   the handle on one thread.
///
/// # Panic safety (CHECKLIST 1.7 F1 / F3)
///
/// The release profile sets `panic = "abort"`. Any panic in Rust code
/// reachable from an FFI entry point terminates the process rather
/// than unwinding across the foreign-function boundary (which is UB).
/// FFI entry points still validate null/UTF-8/length inputs explicitly
/// and return negative error codes for the recoverable failures.
use crate::{
    BatchCommitment, ConsensusValidator, Distinction, DistinctionEngine, NetworkAgent,
    PeerIdentity, TransactionBatch,
};
use std::ffi::{c_char, CStr, CString};
use std::mem::ManuallyDrop;
use std::slice;
use std::sync::{Arc, Mutex};

// ============================================================================
// OPAQUE TYPES - Hide Rust internals from C ABI
// ============================================================================
//
// F4 closure (CHECKLIST 1.7 / Phase 6 sub-branch #8). v1.2.0 typed each
// handle as `type KoruEngine = c_void`, which collapses all three types
// to the same C `void *` on the wire. A C caller could pass an engine
// pointer where the header asks for an agent and the compiler would
// not catch it. The zero-sized `_private: [u8; 0]` pattern produces
// distinct opaque structs in the generated header so cbindgen emits
// `typedef struct KoruEngine KoruEngine;` etc., and C compilers reject
// mismatched pointer types at compile time.
//
// The Rust-side allocations behind these handles are intentionally
// different from the public opaque shape — the runtime pointers point
// at internal owned structures (Arc<DistinctionEngine>,
// Box<Mutex<NetworkAgent>>, Box<Mutex<ConsensusValidator>>). The
// FFI entry points cast to those internal types before dereferencing.

/// Opaque handle to a `DistinctionEngine`. The Rust allocation behind
/// this pointer is `Arc<DistinctionEngine>` (engine has interior
/// `&self` thread-safe state via DashMap; no FFI-side lock needed).
#[repr(C)]
pub struct KoruEngine {
    _private: [u8; 0],
}

/// Opaque handle to a `NetworkAgent`. The Rust allocation behind this
/// pointer is `Box<Mutex<NetworkAgent>>` — concurrent C-thread calls
/// targeting the same handle serialize through the internal mutex
/// (F2 / F8 closure).
#[repr(C)]
pub struct KoruAgent {
    _private: [u8; 0],
}

/// Opaque handle to a `ConsensusValidator`. The Rust allocation behind
/// this pointer is `Box<Mutex<ConsensusValidator>>` — same FFI
/// concurrency contract as `KoruAgent`.
#[repr(C)]
pub struct KoruValidator {
    _private: [u8; 0],
}

/// Internal alias for the owned FFI representation of an agent handle.
type FfiAgent = Mutex<NetworkAgent>;

/// Internal alias for the owned FFI representation of a validator handle.
type FfiValidator = Mutex<ConsensusValidator>;

/// Maximum byte length accepted for any `usize`-sized buffer parameter
/// crossing the FFI boundary.
///
/// F9 closure (CHECKLIST 1.7 / Phase 6 sub-branch #8). `slice::from_raw_parts`
/// requires `len <= isize::MAX`; longer buffers are UB. The audit
/// flagged the unguarded `batch_len: usize` parameters on
/// `koru_agent_propose_commitment` / `koru_agent_finalize_batch` /
/// `koru_agent_propose_batch`. Every length-taking entry point clips
/// at this bound and returns `KORU_ERROR_INVALID_DATA` before any
/// pointer dereference. Note: `isize::MAX` is platform-dependent
/// (2³¹−1 on 32-bit; 2⁶³−1 on 64-bit).
const FFI_MAX_BUFFER_LEN: usize = isize::MAX as usize;

/// Borrow the engine handle as `ManuallyDrop<Arc<DistinctionEngine>>`
/// for the duration of the call.
///
/// F6 closure (CHECKLIST 1.7 / Phase 6 sub-branch #8). v1.2.0 used the
/// `Arc::from_raw(...) + Arc::into_raw(...)` dance to "borrow" an Arc
/// without affecting the strong count; the comment claimed the
/// re-`into_raw` re-incremented, but it actually relied on no Drop
/// running between the two calls. A panic between `from_raw` and
/// `into_raw` would silently leak — or worse, decrement to zero and
/// free the engine while the caller's pointer still held it.
///
/// `ManuallyDrop` is the explicit form: it takes ownership in the type
/// system but suppresses the destructor, so the +1 strong count that
/// the caller's raw pointer represents stays intact regardless of how
/// the FFI body exits (panic, early return, etc.). With `panic = "abort"`
/// on release the panic case is moot; this is still the correct
/// pattern under unwind, and matches the documented `Arc::from_raw`
/// contract.
///
/// # Safety
/// `engine` must be a non-null pointer originally returned by
/// `koru_engine_new` (or compatibly constructed).
#[inline]
unsafe fn borrow_engine(engine: *const KoruEngine) -> ManuallyDrop<Arc<DistinctionEngine>> {
    ManuallyDrop::new(Arc::from_raw(engine as *const DistinctionEngine))
}

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

/// Create a new NetworkAgent.
///
/// The underlying allocation is `Box<Mutex<NetworkAgent>>` (F2 / F8
/// closure). Concurrent calls from C threads targeting the same
/// handle serialize through the internal mutex; the FFI never produces
/// `&mut NetworkAgent` aliases.
///
/// Returns: Opaque pointer to agent (must be freed with koru_agent_free).
///
/// # Safety
/// engine must be a valid pointer from `koru_engine_new`. Caller must
/// free the returned pointer with `koru_agent_free` exactly once.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_new(engine: *const KoruEngine) -> *mut KoruAgent {
    if engine.is_null() {
        return std::ptr::null_mut();
    }

    let engine_md = borrow_engine(engine);
    let agent = Box::new(Mutex::new(NetworkAgent::new(&engine_md)));
    Box::into_raw(agent) as *mut KoruAgent
}

/// Free a NetworkAgent
///
/// # Safety
/// Pointer must be valid (or null) and not used after this call.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_free(agent: *mut KoruAgent) {
    if !agent.is_null() {
        let _ = Box::from_raw(agent as *mut FfiAgent);
    }
}

/// Get current epoch from agent
///
/// # Safety
/// agent must be a valid pointer.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_current_epoch(agent: *const KoruAgent) -> u64 {
    if agent.is_null() {
        return 0;
    }
    let guard = (*(agent as *const FfiAgent)).lock().expect("agent mutex poisoned");
    guard.current_epoch()
}

/// Get validator count from agent
///
/// # Safety
/// agent must be a valid pointer.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_validator_count(agent: *const KoruAgent) -> usize {
    if agent.is_null() {
        return 0;
    }
    let guard = (*(agent as *const FfiAgent)).lock().expect("agent mutex poisoned");
    guard.validator_count()
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

    let guard = (*(agent as *const FfiAgent)).lock().expect("agent mutex poisoned");
    // Bytes-on-wire architecture (DECISION 5.5): the engine produces a
    // canonical [u8; 16]; the FFI human surface emits it as 32-char hex.
    let hex_root = guard.consensus_state_root();
    debug_assert_eq!(hex_root.len(), 32, "hex root must be 32 chars");

    match CString::new(hex_root) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Get expected transaction nonce from agent's validator
///
/// # Safety
/// agent must be a valid pointer.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_expected_nonce(agent: *const KoruAgent) -> u64 {
    if agent.is_null() {
        return 0;
    }
    let guard = (*(agent as *const FfiAgent)).lock().expect("agent mutex poisoned");
    guard.consensus_validator_expected_nonce()
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

    let engine_md = borrow_engine(engine);

    let root_str = match CStr::from_ptr(root_hex).to_str() {
        Ok(s) => s,
        Err(_) => return KORU_ERROR_UTF8,
    };

    let root_id = match Distinction::from_hex(root_str) {
        Ok(d) => d,
        Err(_) => return KORU_ERROR_INVALID_DATA,
    };

    let mut guard = (*(agent as *mut FfiAgent)).lock().expect("agent mutex poisoned");
    match guard.restore_consensus_validator_state(&engine_md, root_id, nonce) {
        Ok(()) => KORU_SUCCESS,
        Err(_) => KORU_ERROR_INVALID_DATA,
    }
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

    let engine_md = borrow_engine(engine);

    let peer_id_str = match CStr::from_ptr(peer_id).to_str() {
        Ok(s) => s,
        Err(_) => return KORU_ERROR_UTF8,
    };

    let peer = match PeerIdentity::new(peer_id_str.to_string(), &engine_md) {
        Ok(p) => p,
        // N1/N2: empty or oversized peer ids are rejected before any
        // synth runs into the engine.
        Err(_) => return KORU_ERROR_INVALID_DATA,
    };

    let mut guard = (*(agent as *mut FfiAgent)).lock().expect("agent mutex poisoned");
    guard.join_peer(peer, &engine_md);
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
    // F9: reject buffers larger than `isize::MAX` before any pointer
    // dereference. `slice::from_raw_parts` is UB at `len > isize::MAX`.
    if batch_len > FFI_MAX_BUFFER_LEN {
        return KORU_ERROR_INVALID_DATA;
    }

    let engine_md = borrow_engine(engine);

    // Deserialize batch from bytes
    let batch_bytes = slice::from_raw_parts(batch_data, batch_len);
    let batch: TransactionBatch = match serde_json::from_slice(batch_bytes) {
        Ok(b) => b,
        Err(_) => return KORU_ERROR_INVALID_DATA,
    };

    let mut guard = (*(agent as *mut FfiAgent)).lock().expect("agent mutex poisoned");
    let commitment = match guard.propose_commitment(batch, &engine_md) {
        Ok(c) => c,
        Err(_) => return KORU_ERROR_BATCH_REJECTED,
    };

    std::ptr::copy_nonoverlapping(commitment.commitment_hash.as_ptr(), out_commitment, 32);
    KORU_SUCCESS
}

/// STAGE 1: Check Commitment (Light Node "Ping" Check)
///
/// Verifies a gossiped commitment matches expected nonce/epoch and is
/// well-formed against the supplied `leader_id` / `batch_size`. This is
/// how light clients participate in consensus without downloading the
/// batch.
///
/// # F7 FFI closure (CHECKLIST 1.7 / Phase 6 sub-branch #8)
///
/// v1.2.0 built a "Frankenstein" `BatchCommitment` with empty
/// `leader_id` and `batch_size = 0`. Since `BatchCommitment::compute`
/// (after the N6 fix) now hashes `leader_id`, any downstream call
/// that recomputes the hash against the Frankenstein object would
/// disagree with the gossiped hash. This FFI now requires the
/// caller to supply the real `leader_id` and `batch_size` so the
/// constructed `BatchCommitment` matches the protocol shape.
///
/// The light-node check itself remains metadata-only by design
/// (`BatchCommitment::verify(nonce, epoch)`): full hash integrity
/// requires the batch payload (or a locally-cached expected hash),
/// neither of which a light node has on hand. Higher-level
/// protocols that DO have an expected hash can compare the
/// `commitment_hash` bytes against their local copy after this
/// metadata check passes.
///
/// # Parameters
/// - agent: NetworkAgent pointer
/// - commitment_hash: pointer to 32-byte commitment hash
/// - expected_nonce: expected nonce for next transaction
/// - expected_epoch: expected current epoch
/// - leader_id: UTF-8 C string naming the proposing leader
///   (required; empty strings rejected per N2)
/// - batch_size: number of transactions in the batch the gossiped
///   commitment binds (required; must match the underlying batch)
///
/// # Returns
/// 1 if the commitment passes the metadata check, 0 if it fails the
/// metadata check, negative on input error.
///
/// # Safety
/// agent must be a valid pointer; commitment_hash must point to a
/// 32-byte buffer; leader_id must be a valid null-terminated UTF-8 C
/// string.
#[no_mangle]
pub unsafe extern "C" fn koru_agent_check_commitment(
    agent: *const KoruAgent,
    commitment_hash: *const u8,
    expected_nonce: u64,
    expected_epoch: u64,
    leader_id: *const c_char,
    batch_size: u64,
) -> i32 {
    if agent.is_null() || commitment_hash.is_null() || leader_id.is_null() {
        return KORU_ERROR_NULL_POINTER;
    }

    let leader_id_str = match CStr::from_ptr(leader_id).to_str() {
        Ok(s) => s,
        Err(_) => return KORU_ERROR_UTF8,
    };
    if leader_id_str.is_empty() {
        // Mirrors PeerIdentity::new's N2 rejection — an empty leader_id
        // is not a valid commitment field.
        return KORU_ERROR_INVALID_DATA;
    }

    let mut hash = [0u8; 32];
    std::ptr::copy_nonoverlapping(commitment_hash, hash.as_mut_ptr(), 32);

    let commitment = BatchCommitment {
        commitment_hash: hash,
        nonce: expected_nonce,
        epoch: expected_epoch,
        leader_id: leader_id_str.to_string(),
        batch_size: batch_size as usize,
    };

    let guard = (*(agent as *const FfiAgent)).lock().expect("agent mutex poisoned");
    if guard.check_commitment(&commitment) {
        1
    } else {
        0
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
    if batch_len > FFI_MAX_BUFFER_LEN {
        return KORU_ERROR_INVALID_DATA;
    }

    let engine_md = borrow_engine(engine);

    let batch_bytes = slice::from_raw_parts(batch_data, batch_len);
    let batch: TransactionBatch = match serde_json::from_slice(batch_bytes) {
        Ok(b) => b,
        Err(_) => return KORU_ERROR_INVALID_DATA,
    };

    let mut hash = [0u8; 32];
    std::ptr::copy_nonoverlapping(commitment_hash, hash.as_mut_ptr(), 32);

    let mut guard = (*(agent as *mut FfiAgent)).lock().expect("agent mutex poisoned");
    match guard.finalize_batch(batch, hash, &engine_md) {
        Ok(_) => KORU_SUCCESS,
        Err(_) => KORU_ERROR_BATCH_REJECTED,
    }
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

    let engine_md = borrow_engine(engine);

    let batch_str = match CStr::from_ptr(batch_json).to_str() {
        Ok(s) => s,
        Err(_) => return KORU_ERROR_UTF8,
    };

    let batch: TransactionBatch = match serde_json::from_str(batch_str) {
        Ok(b) => b,
        Err(_) => return KORU_ERROR_INVALID_DATA,
    };

    let mut guard = (*(agent as *mut FfiAgent)).lock().expect("agent mutex poisoned");
    match guard.propose_commitment(batch.clone(), &engine_md) {
        Ok(commitment) => match guard.finalize_batch(batch, commitment.commitment_hash, &engine_md) {
            Ok(_) => KORU_SUCCESS,
            Err(_) => KORU_ERROR_BATCH_REJECTED,
        },
        Err(_) => KORU_ERROR_BATCH_REJECTED,
    }
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

    let engine_md = borrow_engine(engine);
    let mut guard = (*(agent as *mut FfiAgent)).lock().expect("agent mutex poisoned");
    guard.advance_epoch(&engine_md);
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

    let guard = (*(agent as *const FfiAgent)).lock().expect("agent mutex poisoned");

    match guard.get_current_leader() {
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

/// Create a new ConsensusValidator.
///
/// The underlying allocation is `Box<Mutex<ConsensusValidator>>` (F2 /
/// F8 closure). Concurrent calls from C threads targeting the same
/// handle serialize through the internal mutex.
///
/// # Safety
/// engine must be a valid pointer. Caller must free the returned
/// pointer with `koru_validator_free` exactly once.
#[no_mangle]
pub unsafe extern "C" fn koru_validator_new(engine: *const KoruEngine) -> *mut KoruValidator {
    if engine.is_null() {
        return std::ptr::null_mut();
    }

    let engine_md = borrow_engine(engine);
    let validator = Box::new(Mutex::new(ConsensusValidator::new(&engine_md)));
    Box::into_raw(validator) as *mut KoruValidator
}

/// Free a ConsensusValidator
///
/// # Safety
/// Pointer must be valid (or null) and not used after this call.
#[no_mangle]
pub unsafe extern "C" fn koru_validator_free(validator: *mut KoruValidator) {
    if !validator.is_null() {
        let _ = Box::from_raw(validator as *mut FfiValidator);
    }
}

/// Get expected nonce from validator
///
/// # Safety
/// validator must be a valid pointer.
#[no_mangle]
pub unsafe extern "C" fn koru_validator_expected_nonce(validator: *const KoruValidator) -> u64 {
    if validator.is_null() {
        return 0;
    }
    let guard = (*(validator as *const FfiValidator)).lock().expect("validator mutex poisoned");
    guard.expected_nonce()
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

            // F7: must supply real leader_id + batch_size now. The
            // propose_commitment path uses "unknown" when no leader is
            // joined; pass the same string here so the constructed
            // commitment shape matches.
            let leader = CString::new("unknown").unwrap();

            // Check with correct nonce and epoch (should succeed)
            let result = koru_agent_check_commitment(
                agent,
                commitment_hash.as_ptr(),
                0, // expected_nonce
                0, // expected_epoch
                leader.as_ptr(),
                2, // batch_size
            );
            assert_eq!(result, 1); // Valid

            // Check with incorrect nonce (should fail)
            let result_bad = koru_agent_check_commitment(
                agent,
                commitment_hash.as_ptr(),
                999, // wrong nonce
                0,
                leader.as_ptr(),
                2,
            );
            assert_eq!(result_bad, 0); // Invalid

            // F7: empty leader_id is rejected as invalid input.
            let empty_leader = CString::new("").unwrap();
            let result_empty_leader = koru_agent_check_commitment(
                agent,
                commitment_hash.as_ptr(),
                0,
                0,
                empty_leader.as_ptr(),
                2,
            );
            assert_eq!(result_empty_leader, KORU_ERROR_INVALID_DATA);

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

            // Light node: Verify commitment without downloading batch.
            // F7: must supply real leader_id + batch_size.
            let leader_id = CString::new("unknown").unwrap();
            let is_valid = koru_agent_check_commitment(
                leader,
                commitment_hash.as_ptr(),
                0, // expected_nonce
                0, // expected_epoch
                leader_id.as_ptr(),
                3, // batch_size
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

    #[test]
    fn test_ffi_agent_concurrent_calls_serialize() {
        // F2 / F8 regression: 8 threads each issue 500 join_peer calls
        // through the same agent handle. Under v1.2.0 the FFI exposed
        // `&mut NetworkAgent` from a `*mut KoruAgent` per call —
        // concurrent calls would be UB. Under v2.0 the internal
        // `Mutex<NetworkAgent>` serializes them; this test must
        // complete without data races (sanitizer-clean) and end with
        // the expected validator count.
        use std::sync::atomic::{AtomicPtr, Ordering};
        use std::thread;

        unsafe {
            let engine = koru_engine_new();
            let agent = koru_agent_new(engine);

            // AtomicPtr wraps the raw pointers so we can Send them to
            // worker threads — the FFI handles are opaque to Rust's
            // borrow checker, and the underlying Mutex provides the
            // actual synchronization.
            let engine_ptr = AtomicPtr::new(engine);
            let agent_ptr = AtomicPtr::new(agent);

            const THREADS: usize = 8;
            const JOINS_PER_THREAD: usize = 500;

            thread::scope(|s| {
                for tid in 0..THREADS {
                    let engine_ref = &engine_ptr;
                    let agent_ref = &agent_ptr;
                    s.spawn(move || {
                        let engine = engine_ref.load(Ordering::Acquire);
                        let agent = agent_ref.load(Ordering::Acquire);
                        for i in 0..JOINS_PER_THREAD {
                            let id = CString::new(format!("t{}_p{}", tid, i)).unwrap();
                            let rc = koru_agent_join_peer(agent, engine, id.as_ptr());
                            assert_eq!(rc, KORU_SUCCESS);
                        }
                    });
                }
            });

            // Each unique (tid, i) produces a distinct peer id, so all
            // THREADS * JOINS_PER_THREAD joins must have been accepted
            // (no dedupe drops). N7 dedupe is on (id, distinction);
            // unique ids guarantee unique distinctions.
            assert_eq!(
                koru_agent_validator_count(agent),
                THREADS * JOINS_PER_THREAD,
                "all concurrent joins must register exactly once"
            );

            koru_agent_free(agent);
            koru_engine_free(engine);
        }
    }

    #[test]
    fn test_ffi_propose_commitment_rejects_oversized_batch_len() {
        // F9: batch_len > isize::MAX must be rejected before any
        // pointer dereference. We pass a tiny real buffer so dereffing
        // a smaller length would succeed; the F9 cap must fire first.
        unsafe {
            let engine = koru_engine_new();
            let agent = koru_agent_new(engine);
            let buf = [0u8; 8];
            let mut out_hash = [0u8; 32];
            let rc = koru_agent_propose_commitment(
                agent,
                engine,
                buf.as_ptr(),
                usize::MAX,
                out_hash.as_mut_ptr(),
            );
            assert_eq!(rc, KORU_ERROR_INVALID_DATA);
            koru_agent_free(agent);
            koru_engine_free(engine);
        }
    }
}
