// Copyright Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! # XCM Fundamentals
//!
//! XCM standardizes usual actions users take in consensus systems, for example
//! dealing with assets locally, on other chains, and locking them.
//! XCM programs can both be executed locally or sent to a different consensus system.
//! Examples of consensus systems are blockchains and smart contracts.
//!
//! The goal of XCM is to allow multi-chain ecosystems to thrive via specialization.
//! Very specific functionalities can be abstracted away and standardized in this common language.
//! Then, every member of the ecosystem can implement the subset of the language that makes sense
//! for them.
//!
//! The language evolves over time to accomodate the needs of the community
//! via the [RFC process](https://github.com/paritytech/xcm-format/blob/master/proposals/0032-process.md).
//!
//! XCM is the language, it deals with interpreting and executing programs.
//! It does not deal with actually **sending** these programs from one consensus system to another.
//! This responsibility falls to a transport protocol.
//! XCM can even be interpreted on the local system, with no need of a transport protocol.
//! However, automatic and composable workflows can be achieved via the use of one.
//!
//! At the core of XCM lies the XCVM, the Cross-Consensus Virtual Machine.
//! It's the virtual machine that executes XCM programs.
//! It is a specification that comes with the language.
//!
//! For these docs, we'll use a Rust implementation of XCM and the XCVM, consisting of the following
//! parts:
//! - [`XCM`](xcm): Holds the definition of an XCM program, the instructions and main concepts.
//! - [`Executor`](xcm_executor): Implements the XCVM, capable of executing XCMs. Highly
//!   configurable.
//! - [`Builder`](xcm_builder): A collection of types used to configure the executor.
//! - [`XCM Pallet`](pallet_xcm): A FRAME pallet for interacting with the executor.
//! - [`Simulator`](xcm_simulator): A playground to tinker with different XCM programs and executor
//!   configurations.
//!
//! XCM programs are composed of Instructions, which reference Locations and Assets.
//!
//! ## Locations
//!
//! Locations are XCM's vocabulary of places we want to talk about in our XCM programs.
//! They are used to reference things like 32-byte accounts, governance bodies, smart contracts,
//! blockchains and more.
//!
//! Locations are hierarchical.
//! This means some places in consensus are wholly encapsulated in other places.
//! Say we have two systems A and B.
//! If any change in A's state implies a change in B's state, then we say A is interior to B.
#![doc = simple_mermaid::mermaid!("../mermaid/location_hierarchy.mmd")]
//!
//! Parachains are interior to their Relay Chain, since a change in their state implies a change in
//! the Relay Chain's state.
//!
//! Because of this hierarchy, the way we represent locations is with both a number of **parents**,
//! times we move __up__ the hierarchy, and a sequence of **junctions**, the steps we take __down__
//! the hierarchy after going up the specified number of parents.
//!
//! In Rust, this is specified with the following datatype:
//! ```ignore
//! pub struct Location {
//!   parents: u8,
//!   interior: Junctions,
//! }
//! ```
//!
//! Many junctions are available; parachains, pallets, 32 and 20 byte accounts, governance bodies,
//! and arbitrary indices are the most common.
//! A full list of available junctions can be found in the [format](https://github.com/paritytech/xcm-format#interior-locations--junctions)
//! and [Junction enum](xcm::v4::prelude::Junction).
//!
//! We'll use a file system notation to represent locations, and start with relative locations.
//! In the diagram, the location of parachain 1000 as seen from all other locations is as follows:
//! - From the relaychain: `Parachain(1000)`
//! - From parachain 1000 itself: `Here`
//! - From parachain 2000: `../Parachain(1000)`
//!
//! Relative locations are interpreted by the system that is executing an XCM program, which is the
//! receiver of a message in the case where it's sent.
//!
//! Locations can also be absolute.
//! Keeping in line with our filesystem analogy, we can imagine the root of our filesystem to exist.
//! This would be a location with no parents, that is also the parent of all systems that derive
//! their own consensus, say Polkadot or Ethereum or Bitcoin.
//! Such a location does not exist concretely, but we can still use this definition for it.
//! This is the **universal location**.
//! We need the universal location to be able to describe locations in an absolute way.
#![doc = simple_mermaid::mermaid!("../mermaid/universal_location.mmd")]
//!
//! Here, the absolute location of parachain 1000 would be
//! `GlobalConsensus(Polkadot)/Parachain(1000)`.
//!
//! ## Assets
//!
//! We want to be able to reference assets in our XCM programs, if only to be able to pay for fees.
//! Assets are represented using locations.
//!
//! The native asset of a chain is represented by the location of that chain.
//! For example, DOT is represented by the location of the Polkadot relaychain.
//! If the interpreting chain has its own asset, it would be represented by `Here`.
//!
//! How do we represent other assets?
//! The asset hub system parachain in Polkadot, for example, holds a lot of assets.
//! To represent each of them, it uses the indices we mentioned, and it makes them interior to the
//! assets pallet instance it uses.
//! USDT, an example asset that lives on asset hub, is identified by the location
//! `Parachain(1000)/PalletInstance(53)/GeneralIndex(1984)`, when seen from the Polkadot relaychain.
#![doc = simple_mermaid::mermaid!("../mermaid/usdt_location.mmd")]
//!
//! Asset Hub also has another type of assets called `ForeignAssets`.
//! These assets are identified by the XCM Location to their origin.
//! Two such assets are a Parachain asset, like Moonbeam's GLMR, and KSM, from the cousin Kusama
//! network. These are represented as `../Parachain(2004)/PalletInstance(10)` and
//! `../../GlobalConsensus(Kusama)` respectively.
//!
//! The whole type can be seen in the [format](https://github.com/paritytech/xcm-format#6-universal-asset-identifiers)
//! and [rust docs](xcm::v4::prelude::Asset).
//!
//! ## Instructions
//!
//! Given the vocabulary to talk about both locations -- chains and accounts -- and assets, we now
//! need a way to express what we want the consensus system to do when executing our programs.
//! We need a way of writing our programs.
//!
//! XCM programs are composed of a sequence of instructions.
//!
//! All available instructions can be seen in the [format](https://github.com/paritytech/xcm-format#5-the-xcvm-instruction-set)
//! and the [Instruction enum](xcm::v4::prelude::Instruction).
//!
//! A very simple example is the following:
//!
//! ```ignore
//! let message = Xcm(vec![
//!   TransferAsset { assets, beneficiary },
//! ]);
//! ```
//!
//! This instruction is enough to transfer `assets` from the account of the **origin** of a message
//! to the `beneficiary` account. However, because of XCM's generality, fees need to be paid
//! explicitly. This next example sheds more light on this:
//!
//! ```ignore
//! let message = Xcm(vec![
//!   WithdrawAsset(assets),
//!   BuyExecution { fees: assets, weight_limit },
//!   DepositAsset { assets: AssetFilter(Wild(All)), beneficiary },
//! ]);
//! ```
//!
//! Here we see the process of transferring assets was broken down into smaller instructions, and we
//! add the explicit fee payment step in the middle.
//! `WithdrawAsset` withdraws assets from the account of the **origin** of the message for usage
//! inside this message's execution. `BuyExecution` explicitly buys execution for this program using
//! the assets specified in `fees`, with a sanity check of `weight_limit`. `DepositAsset` uses a
//! wildcard, specifying all remaining `assets` after subtracting the fees and a `beneficiary`
//! account.
//!
//! ## Next steps
//!
//! Continue with the [guides](crate::guides) for step-by-step tutorials on XCM,
//! or jump to the [cookbook](crate::cookbook) to see examples.
//!
//! The [glossary](crate::glossary) can be useful if some of the terms are confusing.
