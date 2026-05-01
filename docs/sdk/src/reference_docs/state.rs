//! # State
//!
//! The state is abstracted as a key-value like database. Every item that
//! needs to be persisted by the [State Transition
//! Function](crate::reference_docs::blockchain_state_machines) is written to the state.
//!
//! ## Special keys
//!
//! The key-value pairs in the state are represented as byte sequences. The node
//! doesn't know how to interpret most the key-value pairs. However, there exist some
//! special keys and its values that are known to the node, the so-called
//! [`well-known-keys`](sp_storage::well_known_keys).
