//! # Polkadot
//!
//! Implementation of the Polkadot host in Rust.
//!
//! ## Getting Involved
//!
//! - [Polkadot Forum](https://forum.polkadot.network/)
//! - [Polkadot Parachains](https://parachains.info/)
//! - [Polkadot (multi-chain) Explorer](https://subscan.io/)
//! - Polkadot Fellowship
//! 	- [Runtimes](https://github.com/polkadot-fellows/runtimes)
//! 	- [RFCs](https://github.com/polkadot-fellows/rfcs)
//! - [Polkadot Specs](spec.polkadot.network)
//! - [The Polkadot Parachain Host Implementers' Guide](https://paritytech.github.io/polkadot-sdk/book/)
//! - [Whitepaper](https://www.polkadot.network/whitepaper/)
//!
//! ## Alternative Clients 🌈
//!
//! - [Smoldot](https://crates.io/crates/smoldot-light). Polkadot light client.
//! - https://github.com/qdrvm/kagome
//! - https://github.com/ChainSafe/gossamer
//!
//! ## Platform
//!
//! ### Polkadot 1.x
//!
//! The original vision of Polkadot (i.e. **Polkadot 1**) revolves around the following arguments:
//!
//! * Future is multi-chain, because we need different chains with different specialization to
//!   achieve widespread goals.
//! * In other words, no single chain is good enough to achieve this.
//! * A multi-chain future will inadvertently suffer from fragmentation of economic security.
//!   * This stake fragmentation will make communication over consensus system with varying security
//!     levels inherently unsafe.
//!
//! Polkadot's answer to the above is:
//!
//! * Shared Security: The idea of shared economic security sits at the core of Polkadot. Polkadot
//!   enables different blockchains (i.e. "*Parachains*") to pool their economic security from
//!   Polkadot (i.e. "*Relay Chain*").
//! * A framework to build blockchains: In order to materialize the multi-chain future, an easy
//!   blockchain framework must exist. This is [Substrate](crate::polkadot_sdk::substrate),
//!   [FRAME](crate::polkadot_sdk::frame_runtime) and [Cumulus](crate::polkadot_sdk::cumulus).
//! * A communication language between blockchains: In order for these blockchains to communicate,
//!   they need a shared language. [XCM](crate::polkadot_sdk::xcm) is one such language.
//!
//! > Note that the interoperability promised by Polkadot is unparalleled in that any two parachains
//! > connected to Polkadot have the same security and can have much higher guarantees about the
//! > security of the recipient of any message.
//!
//! Polkadot delivers the above vision, alongside a flexible means for parachains to schedule
//! themselves with the Relay Chain. To achieve this, Polkadot has been developed with an
//! architecture similar to that of a computer. Polkadot Relay Chain has a number of "cores". Each
//! is (in simple terms) capable of progressing 1 parachain at a time. For example, a parachain
//! can schedule itself on a single core for 5 blocks.
//!
//! Within the scope of Polkadot 1.x, two main scheduling ways have been considered:
//!
//! * Long term Parachains, obtained through locking a sum of DOT in an auction system.
//! * on-demand Parachains, purchased through paying DOT to the relay-chain whenever needed.
//!
//! This scheduling system, and its evolution is the segway into Polkadot 2.x
//!
//! ### Polkadot 2.x
//!
//! TODO
