// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Primitive types for speculative cross-chain messaging.
//!
//! This crate defines the core types used by the speculative messaging system,
//! which replaces HRMP with off-chain message passing and on-chain commitment
//! verification using Merkle Mountain Ranges (MMRs).
//!
//! # Architecture
//!
//! - **Commitments**: [`ProvidesCommitment`] and [`RequiresCommitment`] are the minimal hashes
//!   verified by the relay chain.
//! - **Merkle Tree**: A binary Merkle tree maps destination `ParaId`s to their per-destination MMR
//!   roots, producing the top-level root in [`ProvidesCommitment`].
//! - **Proofs**: [`LateBlockProof`] allows a receiving chain whose `requires` references an older
//!   root to prove consistency with the current `provides`.
//! - **Messages**: [`OutgoingMessage`] and [`MessageBatch`] describe the off-chain data exchanged
//!   between collators.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod commitments;
pub mod error;
pub mod merkle_tree;
pub mod messages;
pub mod proofs;
pub mod state;

pub use commitments::{ProvidesCommitment, RequiresCommitment};
pub use error::SpeculativeMessagingError;
pub use merkle_tree::{DestinationMerkleTree, MerkleProof, StoredMerkleTree};
pub use messages::{MessageBatch, OutgoingMessage};
pub use proofs::{LateBlockProof, MmrExtensionProof};
pub use state::{IncomingMessageState, OutgoingMessageState, SourceState};
