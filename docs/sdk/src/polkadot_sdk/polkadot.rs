//! # Polkadot
//!
//!
//!
//! - [`Polkadot Parachains`]
//! - Polkadot Fellowship
//!     - [`Runtimes`]
//! 	- [`Dashboard`]
//! - [`The Polkadot Parachain Host Implementers' Guide`]
//! - [`JAM Graypaper`]
//!
//!
//! - [`KAGOME`]. C++ implementation of the Polkadot host.
//!
//!
//!
//!
//! **Polkadot 1.0**) revolves around the following arguments:
//!
//!   achieve widespread goals.
//! * A multi-chain future will inadvertently suffer from fragmentation of economic security.
//!     levels inherently unsafe.
//!
//!
//! > their execution and governance sovereignty. These chains are called "Parachains".
//!
//!   enables different parachains to pool their economic security from Polkadot (i.e. "*Relay
//! * (heterogenous) Sharded Execution: Yet, each parachain is free to have its own execution logic
//!   correct execution of all parachains, without having all of its validators re-execute all
//!   the validity of the block execution of multiple parachains using the same set of validators as
//!   security as the Relay Chain.
//! * A framework to build blockchains: In order to materialize the ecosystem of parachains, an easy
//!   [`FRAME`] and [`Cumulus`].
//!   they need a shared language. [`XCM`] is one such language, and the one
//!
//! > connected to Polkadot have the same security and can have much better guarantees about the
//! > Bridges enable transaction and information flow between different consensus systems, crucial
//! > vulnerable points. If a bridge's security measures are weaker than those of the connected
//! > attacks such as theft or disruption of services.
//!
//! themselves with the Relay Chain. To achieve this, Polkadot has been developed with an
//! core is (in simple terms) capable of progressing 1 parachain at a time. For example, a parachain
//!
//!
//! * On-demand Parachains, purchased through paying DOT to the relay-chain whenever needed.
//!
//!
//! of the community and the fellowship. This is happening most notable through the RFC process.
//!
//!   Agile periodic-sale-based model for assigning Coretime on the Polkadot Ubiquitous Computer.
//!   Interface for manipulating the usage of cores on the Polkadot Ubiquitous Computer.
//!

// [`Agile-coretime`]: https://github.com/polkadot-fellows/RFCs/blob/main/text/0001-agile-coretime.md
// [`Approval Checking`]: https://polkadot.network/blog/polkadot-v1-0-sharding-and-economic-security#approval-checking-and-finality
// [`Coretime-interface`]: https://github.com/polkadot-fellows/RFCs/blob/main/text/0005-coretime-interface.md
// [`Cumulus`]: crate::polkadot_sdk::cumulus
// [`Dashboard`]: https://polkadot-fellows.github.io/dashboard/
// [`FRAME`]: crate::polkadot_sdk::frame_runtime
// [`Gossamer`]: https://github.com/ChainSafe/gossamer
// [`JAM Graypaper`]: https://graypaper.com
// [`KAGOME`]: https://github.com/qdrvm/kagome
// [`Manifesto`]: https://github.com/polkadot-fellows/manifesto/blob/main/manifesto.pdf
// [`Polkadot (multi-chain) Explorer: Subscan`]: multi-chain) Explorer: Subscan](https://subscan.io/
// [`Polkadot Forum`]: https://forum.polkadot.network/
// [`Polkadot Parachains`]: https://parachains.info/
// [`Polkadot Specs`]: http://spec.polkadot.network
// [`Polkadot as a Computational Resource`]: https://wiki.polkadot.network/docs/polkadot-direction#polkadot-as-a-computational-resource
// [`RFCs`]: https://github.com/polkadot-fellows/rfcs
// [`Runtimes`]: https://github.com/polkadot-fellows/runtimes
// [`Smoldot`]: https://docs.rs/crate/smoldot-light/latest
// [`Substrate`]: crate::polkadot_sdk::substrate
// [`The Polkadot Parachain Host Implementers' Guide`]: https://paritytech.github.io/polkadot-sdk/book/
// [`Whitepaper`]: https://www.polkadot.network/whitepaper/
// [`XCM`]: crate::polkadot_sdk::xcm
