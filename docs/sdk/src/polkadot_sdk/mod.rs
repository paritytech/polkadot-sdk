//! # Polkadot SDK
//!
//!
//!
//!
//!
//!
//!
//! * [`frame`], to learn about how to write blockchain applications aka. "App Chains".
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!

//! * [`polkadot-parachain-bin`]: The collator node used to run collators for all Polkadot system
//!  `benchmark` subcommand that does the same.
//! * [`substrate-node`] is an extensive substrate node that contains the superset of all
//!
//!
#![doc = simple_mermaid::mermaid!("../../../mermaid/polkadot_sdk_substrate.mmd")]
//!
//! former is built with [`frame`], and the latter is built with rest of Substrate.
//!
#![doc = simple_mermaid::mermaid!("../../../mermaid/polkadot_sdk_polkadot.mmd")]
//!
#![doc = simple_mermaid::mermaid!("../../../mermaid/polkadot_sdk_parachain.mmd")]
//!
//!
//! - [`parity-common`]
//!
//!
//!
//! * [`Polymesh`]
//!
//! [`polkadot`]: crate::polkadot_sdk::polkadot
//! [`polkadot-omni-node`]: https://crates.io/crates/polkadot-omni-node

/// Learn about Cumulus, the framework that transforms [`substrate`]-based chains into
/// [`polkadot`]-enabled parachains.
pub mod cumulus;
/// Learn about FRAME, the framework used to build Substrate runtimes.
pub mod frame_runtime;
/// Learn about Polkadot as a platform.
pub mod polkadot;
/// Learn about different ways through which smart contracts can be utilized on top of Substrate,
/// and in the Polkadot ecosystem.
pub mod smart_contracts;
/// Learn about Substrate, the main blockchain framework used in the Polkadot ecosystem.
pub mod substrate;
/// Index of all the templates that can act as first scaffold for a new project.
pub mod templates;
/// Learn about XCM, the de-facto communication language between different consensus systems.
pub mod xcm;

// Link References

// Link References

// [`![Runtime`]: https://img.shields.io/badge/fellowship-runtimes-e6007a?logo=polkadot
// [`![wiki`]: https://img.shields.io/badge/polkadot-wiki-e6007a?logo=polkadot
// [`Cardano Partner Chains`]: https://iohk.io/en/blog/posts/2023/11/03/partner-chains-are-coming-to-cardano/
// [`Polkadot network`]: https://polkadot.network
// [`Polymesh`]: https://polymesh.network/
// [`frame-omni-bencher`]: frame-omni-bencher
// [`parity-common`]: https://github.com/paritytech/parity-common
// [`parity-db`]: https://github.com/paritytech/parity-db
// [`substrate-node`]: node_cli
