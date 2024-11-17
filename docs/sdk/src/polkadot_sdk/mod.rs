//! # Polkadot SDK
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
//!
//!
//!
//!
//!
//!
//!

//!  `benchmark` subcommand that does the same.
//!
//!
#![doc = simple_mermaid::mermaid!("../../../mermaid/polkadot_sdk_substrate.mmd")]
//!
//!
#![doc = simple_mermaid::mermaid!("../../../mermaid/polkadot_sdk_polkadot.mmd")]
//!
#![doc = simple_mermaid::mermaid!("../../../mermaid/polkadot_sdk_parachain.mmd")]
//!
//!
//!
//!
//!
//!
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

// [`![wiki`]: https://img.shields.io/badge/polkadot-wiki-e6007a?logo=polkadot
// [`Polkadot network`]: https://polkadot.network
// [`frame-omni-bencher`]: frame-omni-bencher
// [`parity-db`]: https://github.com/paritytech/parity-db

// [`frame-omni-bencher`]: frame-omni-bencher
