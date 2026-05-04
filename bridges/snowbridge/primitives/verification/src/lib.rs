// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
//! Types for representing inbound messages
#![cfg_attr(not(feature = "std"), no_std)]
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	traits::{ConstU32, Get},
	BoundedVec, PalletError,
};
use scale_info::TypeInfo;
use snowbridge_beacon_primitives::{BeaconHeader, ExecutionProof};
use sp_core::{H160, H256};
use sp_std::prelude::*;

/// Bounded receipt proof node: one MPT node (RLP-encoded). Parameterized by max node size from
/// runtime.
pub type ReceiptProofNode<MaxNodeSize> = BoundedVec<u8, MaxNodeSize>;

/// Bounded receipt proof: MPT proof nodes from the Ethereum receipts tree.
/// Parameterized by max node size and max depth from runtime.
pub type ReceiptProof<MaxNodeSize, MaxDepth> = BoundedVec<ReceiptProofNode<MaxNodeSize>, MaxDepth>;

/// Default max MPT node size in bytes (32 KiB). Reusable for runtime config and proof bounds.
pub const DEFAULT_MAX_NODE_SIZE: u32 = 32_768;
/// Default max receipt proof depth. Reusable for runtime config and proof bounds.
///
/// Maximum number of nodes allowed in a receipt MPT proof. Ethereum's receipt trie is a 16-way
/// Merkle Patricia Trie keyed by transaction index. Even with the maximum ~100k transactions per
/// block, the trie depth is at most ceil(log16(100_000)) = 5. So a limit of 16 is generous and
/// prevents unbounded iteration.
pub const DEFAULT_MAX_DEPTH: u32 = 16;

/// Type alias for default max node size (for use in [`Proof`] / [`ReceiptProof`]).
pub type DefaultMaxNodeSize = ConstU32<{ DEFAULT_MAX_NODE_SIZE }>;
/// Type alias for default max depth (for use in [`Proof`] / [`ReceiptProof`]).
pub type DefaultMaxDepth = ConstU32<{ DEFAULT_MAX_DEPTH }>;

pub mod receipt;

/// A trait for verifying inbound messages from Ethereum.
/// The concrete proof type is given by the associated type `Proof`.
pub trait Verifier {
	/// The proof type accepted by this verifier (parameterized by runtime bounds).
	type Proof;

	fn verify(event: &Log, proof: &Self::Proof) -> Result<(), VerificationError>;
}

#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Debug, PalletError, TypeInfo)]
#[cfg_attr(feature = "std", derive(PartialEq))]
pub enum VerificationError {
	/// Execution header is missing
	HeaderNotFound,
	/// Event log was not found in the verified transaction receipt
	LogNotFound,
	/// Event log has an invalid format
	InvalidLog,
	/// Unable to verify the transaction receipt with the provided proof
	InvalidProof,
	/// Unable to verify the execution header with ancestry proof
	InvalidExecutionProof(#[codec(skip)] &'static str),
}

/// A bridge message from the Gateway contract on Ethereum.
/// Generic over the proof type (typically `Proof<MaxMptNodeSize, MaxReceiptProofDepth>` from
/// runtime).
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Debug, TypeInfo)]
pub struct EventProof<Proof> {
	/// Event log emitted by Gateway contract
	pub event_log: Log,
	/// Inclusion proof for a transaction receipt containing the event log
	pub proof: Proof,
}

/// Event log
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Debug, TypeInfo)]
pub struct Log {
	pub address: H160,
	pub topics: Vec<H256>,
	pub data: Vec<u8>,
	pub tx_index: u64,
}

/// Inclusion proof for a transaction receipt.
/// Generic over max MPT node size and max receipt proof depth (from runtime config).
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Debug)]
pub struct Proof<MaxNodeSize = DefaultMaxNodeSize, MaxDepth = DefaultMaxDepth>
where
	MaxNodeSize: Get<u32>,
	MaxDepth: Get<u32>,
{
	/// Proof values from receipts tree
	pub receipt_proof: ReceiptProof<MaxNodeSize, MaxDepth>,
	/// Proof that an execution header was finalized by the beacon chain
	pub execution_proof: ExecutionProof,
}

impl<MaxNodeSize, MaxDepth> PartialEq for Proof<MaxNodeSize, MaxDepth>
where
	MaxNodeSize: Get<u32>,
	MaxDepth: Get<u32>,
	ReceiptProof<MaxNodeSize, MaxDepth>: PartialEq,
{
	fn eq(&self, other: &Self) -> bool {
		self.receipt_proof == other.receipt_proof && self.execution_proof == other.execution_proof
	}
}

impl<MaxNodeSize, MaxDepth> TypeInfo for Proof<MaxNodeSize, MaxDepth>
where
	MaxNodeSize: Get<u32> + 'static,
	MaxDepth: Get<u32> + 'static,
{
	type Identity = Self;

	fn type_info() -> scale_info::Type {
		use scale_info::{build::Fields, Path, Type};
		// Use type-erased representation for the receipt proof so we don't require
		// ConstU32 type params to implement TypeInfo (they're from bounded_collections).
		Type::builder().path(Path::new("Proof", module_path!())).composite(
			Fields::unnamed()
				.field(|f| f.ty::<Vec<Vec<u8>>>().type_name("receipt_proof"))
				.field(|f| f.ty::<ExecutionProof>()),
		)
	}
}

#[derive(Clone, Debug)]
pub struct EventFixture<Proof> {
	pub event: EventProof<Proof>,
	pub finalized_header: BeaconHeader,
	pub block_roots_root: H256,
}

#[cfg(any(test, feature = "runtime-benchmarks", feature = "std"))]
pub mod fixtures;

#[cfg(any(test, feature = "runtime-benchmarks", feature = "std"))]
pub use fixtures::{
	build_hash_chain_proof, short_node, try_receipt_proof_from_vec, ReceiptProofBoundsError,
};
