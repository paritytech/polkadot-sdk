//! # WASM Meta Protocol
//!
//! ecosystem. We refer to this design as the "**WASM Meta Protocol**".
//!
//! **Upgrading any part of the system implies upgrading the entire system**. This has historically
//! words, the entire node software is the specification of the blockchain's [`state transition
//!
//! platforms, but has not been expanded further.
//!
//! blockchain's main "state transition function" in the main blockchain state, in the same fashion
//! [`blockchain_state_machines`], this state transition function is called
//! in the state (see [`sp_core::storage::well_known_keys`]) and can be updated as a part of the
//!
//! > are fundamental differences between the two, explained in
//!
//! [`**Node**`], and is a normal binary that is compiled from
//!
//! Runtime can be updated on the fly, within the execution of a block, and the node is (for the
//!
//! follows:
#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_simple.mmd")]
//!
//!
//!    defined in [`sp_io`]. For example, [`sp_io::storage`] are the set of host functions that
//! 2. **Runtime APIs**: a way for the node to talk to the WASM runtime. Runtime APIs are defined
//!    fundamental runtime API that any blockchain must implement in order to be able to (re)
#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_client_runtime.mmd")]
//!
//! functionality, but it can also expose more APIs. See
//! APIs to your FRAME-based runtime.
//!
//! host functions that the runtime at any point in time requires. Given the fact that a runtime can
//! previous blocks, this means that a node must always maintain support for the old host functions.
//! care**. This is why, for example, adding a new host function to Polkadot always requires an RFC.
//! [`custom_host_functions`].
//!
//!
//! ones of the runtime?
//!
//! transition function. Anything that needs to influence how your blockchain's state is updated,
//! any other application-specific logic that has to do with the state is part of the runtime.
//!
//! facilitate/enable it is part of the node. For example, the database, networking, and even
//!
//! > engine that enables trustless execution of the runtime in a distributed manner whilst
#![doc = simple_mermaid::mermaid!("../../../mermaid/substrate_with_frame.mmd")]
//!
//!
//! runtime. We also hinted that a set of host functions ([`sp_io::storage`]) are how the runtime
//!
//! node side. Nonetheless, the runtime is the only component that:
//!
//! 2. Can fully interpret the state.
//!
//! aware of. The rest of the state, including what logic the runtime has, what balance each user
#![doc = simple_mermaid::mermaid!("../../../mermaid/state.mmd")]
//!
//! does not know what they mean, and it does not know what is the type of the corresponding value
//! keys, and the type of the values.
//!
//! because the node side code is kept oblivious to all of the details of the state transition
//! to know.
//!
//!
//! compilation within it. This used to be called the "Native Runtime". The main purpose of the
//! infrastructure of native code. However, neither of the two arguments strongly hold and the
//!
//!
//! > chains built with Substrate only use WASM execution.
//!
//!
//! obviously, only uses the native runtime if it is the same code as with the wasm blob stored
//! do nodes determine if two runtimes are the same? Through the very important
//! ([`sp_api::Core::version`]) that returns this struct. The node software, or other applications,
//! the same. Namely, [`sp_version::RuntimeVersion::spec_version`] is the main key that implies two
//!
//! updated.
//!
//!
//! received in the node side software as opaque blobs and in the networking layer.
//!
//! *validate*) a block.
//!
//! imported.
//! * The [`sp_api::Core::execute_block`] runtime API is called and the block is passed in as an
//! * The runtime will then execute the block, and update the state accordingly. Any state update is
//! * Both the runtime and node will check the state-root of the state after the block execution to
//!
//! > lecture`]

// Link References
// [`custom_host_functions`]: crate::reference_docs::custom_host_functions
// [`runtime_vs_smart_contract`]: crate::reference_docs::runtime_vs_smart_contract

// Link References
// [`custom_host_functions`]: crate::reference_docs::custom_host_functions
// [`runtime_vs_smart_contract`]: crate::reference_docs::runtime_vs_smart_contract

// [`**Node**`]: crate::reference_docs::glossary#node
// [`this

