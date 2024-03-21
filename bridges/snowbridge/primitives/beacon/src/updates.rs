// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use codec::{Decode, Encode};
use frame_support::{CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_std::prelude::*;

use crate::types::{BeaconHeader, SyncAggregate, SyncCommittee, VersionedExecutionPayloadHeader};

#[derive(Encode, Decode, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound, TypeInfo)]
#[cfg_attr(
	feature = "std",
	derive(serde::Serialize, serde::Deserialize),
	serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
)]
pub struct CheckpointUpdate<const COMMITTEE_SIZE: usize> {
	pub header: BeaconHeader,
	pub current_sync_committee: SyncCommittee<COMMITTEE_SIZE>,
	pub current_sync_committee_branch: Vec<H256>,
	pub validators_root: H256,
	pub block_roots_root: H256,
	pub block_roots_branch: Vec<H256>,
}

impl<const COMMITTEE_SIZE: usize> Default for CheckpointUpdate<COMMITTEE_SIZE> {
	fn default() -> Self {
		CheckpointUpdate {
			header: Default::default(),
			current_sync_committee: Default::default(),
			current_sync_committee_branch: Default::default(),
			validators_root: Default::default(),
			block_roots_root: Default::default(),
			block_roots_branch: Default::default(),
		}
	}
}

#[derive(
	Default, Encode, Decode, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound, TypeInfo,
)]
#[cfg_attr(
	feature = "std",
	derive(serde::Deserialize),
	serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
)]
pub struct Update<const COMMITTEE_SIZE: usize, const COMMITTEE_BITS_SIZE: usize> {
	/// A recent header attesting to the finalized header, using its `state_root`.
	pub attested_header: BeaconHeader,
	/// The signing data that the sync committee produced for this attested header, including
	/// who participated in the vote and the resulting signature.
	pub sync_aggregate: SyncAggregate<COMMITTEE_SIZE, COMMITTEE_BITS_SIZE>,
	/// The slot at which the sync aggregate can be found, typically attested_header.slot + 1, if
	/// the next slot block was not missed.
	pub signature_slot: u64,
	/// The next sync committee for the next sync committee period, if present.
	pub next_sync_committee_update: Option<NextSyncCommitteeUpdate<COMMITTEE_SIZE>>,
	/// The latest finalized header.
	pub finalized_header: BeaconHeader,
	/// The merkle proof testifying to the finalized header, using the `attested_header.state_root`
	/// as tree root.
	pub finality_branch: Vec<H256>,
	/// The finalized_header's `block_roots` root in the beacon state, used for ancestry proofs.
	pub block_roots_root: H256,
	/// The merkle path to prove the `block_roots_root` value.
	pub block_roots_branch: Vec<H256>,
}

#[derive(
	Default, Encode, Decode, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound, TypeInfo,
)]
#[cfg_attr(
	feature = "std",
	derive(serde::Deserialize),
	serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
)]
pub struct NextSyncCommitteeUpdate<const COMMITTEE_SIZE: usize> {
	pub next_sync_committee: SyncCommittee<COMMITTEE_SIZE>,
	pub next_sync_committee_branch: Vec<H256>,
}

#[derive(Encode, Decode, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound, TypeInfo)]
#[cfg_attr(
	feature = "std",
	derive(serde::Deserialize),
	serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
)]
pub struct ExecutionHeaderUpdate {
	/// Header for the beacon block containing the execution payload
	pub header: BeaconHeader,
	/// Proof that `header` is an ancestor of a finalized header
	pub ancestry_proof: Option<AncestryProof>,
	/// Execution header to be imported
	pub execution_header: VersionedExecutionPayloadHeader,
	/// Merkle proof that execution payload is contained within `header`
	pub execution_branch: Vec<H256>,
}

#[derive(Encode, Decode, CloneNoBound, PartialEqNoBound, RuntimeDebugNoBound, TypeInfo)]
#[cfg_attr(
	feature = "std",
	derive(serde::Deserialize),
	serde(deny_unknown_fields, bound(serialize = ""), bound(deserialize = ""))
)]
pub struct AncestryProof {
	/// Merkle proof that `header` is an ancestor of `finalized_header`
	pub header_branch: Vec<H256>,
	/// Root of a finalized block that has already been imported into the light client
	pub finalized_block_root: H256,
}
