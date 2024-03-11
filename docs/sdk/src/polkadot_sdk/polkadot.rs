//! # Polkadot
//!
//! Implementation of the Polkadot node/host in Rust.
//!
//! ## Learn More and Get Involved
//!
//! - [Polkadot Forum](https://forum.polkadot.network/)
//! - [Polkadot Parachains](https://parachains.info/)
//! - [Polkadot (multi-chain) Explorer](https://subscan.io/)
//! - Polkadot Fellowship
//!     - [Manifesto](https://github.com/polkadot-fellows/manifesto)
//!     - [Runtimes](https://github.com/polkadot-fellows/runtimes)
//!     - [RFCs](https://github.com/polkadot-fellows/rfcs)
//! - [Polkadot Specs](spec.polkadot.network)
//! - [The Polkadot Parachain Host Implementers' Guide](https://paritytech.github.io/polkadot-sdk/book/)
//! - [Whitepaper](https://www.polkadot.network/whitepaper/)
//!
//! ## Alternative Node Implementations 🌈
//!
//! - [Smoldot](https://crates.io/crates/smoldot-light). Polkadot light node/client.
//! - [KAGOME](https://github.com/qdrvm/kagome). C++ implementation of the Polkadot host.
//! - [Gossamer](https://github.com/ChainSafe/gossamer). Golang implementation of the Polkadot host.
//!
//! ## Platform
//!
//! In this section, we examine what platform Polkadot exactly provides to developers.
//!
//! ### Polkadot White Paper
//!
//! The original vision of Polkadot (everything in the whitepaper, which was eventually called
//! **Polkadot 1.0**) revolves around the following arguments:
//!
//! * Future is multi-chain, because we need different chains with different specialization to
//!   achieve widespread goals.
//! * In other words, no single chain is good enough to achieve all goals.
//! * A multi-chain future will inadvertently suffer from fragmentation of economic security.
//!   * This stake fragmentation will make communication over consensus system with varying security
//!     levels inherently unsafe.
//!
//! Polkadot's answer to the above is:
//!
//! > The chains of the future must have a way to share their economic security, whilst maintaining
//! > their execution and governance sovereignty. These chains are called "Parachains".
//!
//! * Shared Security: The idea of shared economic security sits at the core of Polkadot. Polkadot
//!   enables different parachains* to pool their economic security from Polkadot (i.e. "*Relay
//!   Chain*").
//! * (heterogenous) Sharded Execution: Yet, each parachain is free to have its own execution logic
//!   (runtime), which also encompasses governance and sovereignty. Moreover, Polkadot ensures the
//!   correct execution of all parachains, without having all of its validators re-execute all
//!   parachain blocks. When seen from this perspective, Polkadot achieves the ability to verify
//!   the validity of the block execution of multiple parachains using the same set of validators as
//!   the Relay Chain. In practice, this means that the shards (parachains) share the same economic
//!   security as the Relay Chain.
//!   Learn about this process called [Approval Checking](https://polkadot.network/blog/polkadot-v1-0-sharding-and-economic-security#approval-checking-and-finality).
//! * A framework to build blockchains: In order to materialize the ecosystem of parachains, an easy
//!   blockchain framework must exist. This is [Substrate](crate::polkadot_sdk::substrate),
//!   [FRAME](crate::polkadot_sdk::frame_runtime) and [Cumulus](crate::polkadot_sdk::cumulus).
//! * A communication language between blockchains: In order for these blockchains to communicate,
//!   they need a shared language. [XCM](crate::polkadot_sdk::xcm) is one such language, and the one
//!   that is most endorsed in the Polkadot ecosystem.
//!
//! > Note that the interoperability promised by Polkadot is unparalleled in that any two parachains
//! > connected to Polkadot have the same security and can have much better guarantees about the
//! > security of the recipient of any message.
//! > Bridges enable transaction and information flow between different consensus systems, crucial
//! > for Polkadot's multi-chain architecture. However, they can become the network's most
//! > vulnerable points. If a bridge's security measures are weaker than those of the connected
//! > blockchains, it poses a significant risk. Attackers might exploit these weaknesses to launch
//! > attacks such as theft or disruption of services.
//!
//! Polkadot delivers the above vision, alongside a flexible means for parachains to schedule
//! themselves with the Relay Chain. To achieve this, Polkadot has been developed with an
//! architecture similar to that of a computer. Polkadot Relay Chain has a number of "cores". Each
//! core is (in simple terms) capable of progressing 1 parachain at a time. For example, a parachain
//! can schedule itself on a single core for 5 relay chain blocks.
//!
//! Within the scope of Polkadot 1.x, two main scheduling ways have been considered:
//!
//! * Long term Parachains, obtained through locking a sum of DOT in an auction system.
//! * on-demand Parachains, purchased through paying DOT to the relay-chain whenever needed.
//!
//! ### The Future
//!
//! After delivering Polkadot 1.x, the future of Polkadot as a protocol and platform is in the hands
//! of the community and the fellowship. This is happening most notable through the RFC process.
//! Some of the RFCs that do alter Polkadot as a platform and have already passed are as follows:
//!
//! - RFC#1: [Agile-coretime](https://github.com/polkadot-fellows/RFCs/blob/main/text/0001-agile-coretime.md):
//!   Agile periodic-sale-based model for assigning Coretime on the Polkadot Ubiquitous Computer.
//! - RFC#5: [Coretime-interface](https://github.com/polkadot-fellows/RFCs/blob/main/text/0005-coretime-interface.md):
//!   Interface for manipulating the usage of cores on the Polkadot Ubiquitous Computer.
//!
//! Learn more about [Polkadot as a Computational Resource](https://wiki.polkadot.network/docs/polkadot-direction#polkadot-as-a-computational-resource).
