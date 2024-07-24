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

//! # Glossary
//!
//! ## XCM (Cross-Consensus Messaging)
//!
//! A messaging format meant to communicate intentions between consensus systems.
//! XCM could also refer to a single message.
//!
//! ## Instructions
//!
//! XCMs are composed of a sequence of instructions.
//! Each instruction aims to convey a particular intention.
//! There are instructions for transferring and locking assets, handling fees, calling arbitrary
//! blobs, and more.
//!
//! ## Consensus system
//!
//! A system that can reach any kind of consensus.
//! For example, relay chains, parachains, smart contracts.
//! Most messaging between consensus systems has to be done asynchronously, for this, XCM is used.
//! Between two smart contracts on the same parachain, however, communication can be done
//! synchronously.
//!
//! ## [`Location`](xcm::v4::prelude::Location)
//!
//! A way of addressing consensus systems.
//! These could be relative or absolute.
//!
//! ## [`Junction`](xcm::v4::prelude::Junction)
//!
//! The different ways of descending down a [`Location`](xcm::v4::prelude::Location) hierarchy.
//! A junction can be a Parachain, an Account, or more.
//!
//! ## [`Asset`](xcm::v4::prelude::Asset)
//!
//! A way of identifying assets in the same or another consensus system, by using a
//! [`Location`](xcm::v4::prelude::Location).
//!
//! ## Sovereign account
//!
//! An account in a consensus system that is controlled by an account in another consensus system.
//!
//! Runtimes use a converter between a [`Location`](xcm::v4::prelude::Location) and an account.
//! These converters implement the [`ConvertLocation`](xcm_executor::traits::ConvertLocation) trait.
//!
//! ## Teleport
//!
//! A way of transferring assets between two consensus systems without the need of a third party.
//! It consists of the sender system burning the asset that wants to be sent over and the recipient
//! minting an equivalent amount of that asset. It requires a lot of trust between the two systems,
//! since failure to mint or burn will reduce or increase the total issuance of the token.
//!
//! ## Reserve asset transfer
//!
//! A way of transferring assets between two consensus systems that don't trust each other, by using
//! a third system they both trust, called the reserve. The real asset only exists on the reserve,
//! both sender and recipient only deal with derivatives. It consists of the sender burning a
//! certain amount of derivatives, telling the reserve to move real assets from its sovereign
//! account to the destination's sovereign account, and then telling the recipient to mint the right
//! amount of derivatives.
//! In practice, the reserve chain can also be one of the source or destination.
//!
//! ## XCVM
//!
//! The virtual machine behind XCM.
//! Every XCM is an XCVM programme.
//! Holds state in registers.
//!
//! An implementation of the virtual machine is the [`xcm-executor`](xcm_executor::XcmExecutor).
//!
//! ## Holding register
//!
//! An XCVM register used to hold arbitrary `Asset`s during the execution of an XCVM programme.
//!
//! ## Barrier
//!
//! An XCM executor configuration item that works as a firewall for incoming XCMs.
//! All XCMs have to pass the barrier to be executed, else they are dropped.
//! It can be used for whitelisting only certain types or messages or messages from certain senders.
//!
//! Lots of barrier definitions exist in [`xcm-builder`](xcm_builder).
//!
//! ## VMP (Vertical Message Passing)
//!
//! Umbrella term for both UMP (Upward Message Passing) and DMP (Downward Message Passing).
//!
//! The following diagram shows the uses of both protocols:
#![doc = simple_mermaid::mermaid!("../mermaid/transport_protocols.mmd")]
//!
//! ## UMP (Upward Message Passing)
//!
//! Transport-layer protocol that allows parachains to send messages upwards to their relay chain.
//!
//! ## DMP (Downward Message Passing)
//!
//! Transport-layer protocol that allows the relay chain to send messages downwards to one of their
//! parachains.
//!
//! ## XCMP (Cross-Consensus Message Passing)
//!
//! Transport-layer protocol that allows parachains to send messages between themselves, without
//! going through the relay chain.
//!
//! ## HRMP (Horizontal Message Passing)
//!
//! Transport-layer protocol that allows a parachain to send messages to a sibling parachain going
//! through the relay chain. It's a precursor to XCMP, also known as XCMP-lite.
//! It uses a mixture of UMP and DMP.
