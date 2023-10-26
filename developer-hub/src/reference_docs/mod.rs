//! # Polkadot SDK Reference Docs.
//!
//! This is the entry point for all reference documents that enhance one's learning experience in
//! the Polkadot SDK.
//!
//! ## What is a "reference document"?
//!
//! First, see [why we use rust-docs for everything](crate#why-rust-docs) and our documentation
//! [principles](crate#principles). We acknowledge that as much of the crucial information should be
//! embedded in the low level rust-docs. Then, high level scenarios should be covered in
//! [`crate::tutorial`]. Finally, we acknowledge that there is a cateogry of information that is:
//!
//! 1. crucial to know.
//! 2. is too high level to be in the rust-doc of any one `type`, `trait` or `fn`.
//! 3. is too low level to be encompassed in a [`crate::tutorial`].
//!
//! We can this class of documents "reference documents". Our goal should be to minimize the number
//! of "reference" docs, as they incur maintenance burden.
//!
//! ## Ownership
//!
//! Every page must have an owner or a list of owners, who are responsible for maintaining the page.

/// Learn how Substrate and FRAME use traits and associated types to make modules generic in a
/// type-safe manner.
pub mod trait_based_programming;

/// Learn about the way Substrate and FRAME view their blockchains as state machines.
pub mod blockchain_state_machines;

/// The glossary.
pub mod glossary;

/// Learn about the WASM meta-protocol of all substrate-based chains.
pub mod wasm_meta_protocol;

/// Learn about the differences between smart contracts and a FRAME-based runtime. They are both
/// "code stored onchain", but how do they differ?
pub mod runtime_vs_smart_contract;

/// Learn about *"Origin"* A topic in FRAME that enables complex account abstractions to be built.
pub mod origin_account_abstraction;

/// Learn about how to write safe and defensive code in your FRAME runtime.
pub mod safe_defensive_programming;

/// Learn about composite enums in FRAME-based runtimes, such as "RuntimeEvent" and "RuntimeCall".
pub mod frame_composite_enums;
