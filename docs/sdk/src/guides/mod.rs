
/// Write your first simple pallet, learning the most most basic features of FRAME along the way.
pub mod your_first_pallet;

/// Write your first real [`runtime`],
/// compiling it to [`WASM`].
pub mod your_first_runtime;

/// Running the given runtime with a node. No specific consensus mechanism is used at this stage.
pub mod your_first_node;

/// How to enhance a given runtime and node to be cumulus-enabled, run it as a parachain
/// and connect it to a relay-chain.
// pub mod your_first_parachain;

/// How to enable storage weight reclaiming in a parachain node and runtime.
pub mod enable_pov_reclaim;

/// How to enable Async Backing on parachain projects that started in 2023 or before.
pub mod async_backing_guide;

/// How to enable metadata hash verification in the runtime.
pub mod enable_metadata_hash;

/// How to enable elastic scaling MVP on a parachain.
pub mod enable_elastic_scaling_mvp;

// Link References

// Link References

// [`runtime`]: crate::reference_docs::wasm_meta_protocol
