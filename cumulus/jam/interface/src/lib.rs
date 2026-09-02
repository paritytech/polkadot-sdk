// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Traits a Cumulus collator uses to talk to a JAM node.
//!
//! Three small traits instead of one big `RelayChainInterface`-style one:
//!
//! - [`JamChainSource`] — follow the JAM chain (best/finalized blocks, parents, roots),
//! - [`JamStateSource`] — read JAM state, values and range proofs,
//! - [`JamWorkPackageSubmission`] — submit work packages and follow their status.
//!
//! Traits only — no I/O here. `cumulus-jam-rpc-interface` implements them over one websocket to a
//! JAM node (JIP-2); a future light client can implement the same traits. All JAM types used in the
//! signatures are re-exported so consumers have one import point.

pub use futures::stream::BoxStream;
pub use jam_std_common::{
	AuthPool, AuthPools, AuthQueues, AvailabilityAssignment, AvailabilityAssignments, BlockDesc,
	ChainSubUpdate, NodeError as Error, NodeResult as Result, RangeProof, ReadyQueue, ReadyRecord,
	Service, ServiceKey, StorageKey, SystemKey, VersionedParameters, WorkPackageStatus, WorkReport,
};
pub use jam_types::{
	AuthorizerHash, CoreIndex, Hash, HeaderHash, MmrPeakHash, ServiceId, Slot, StateRootHash,
	WorkPackage, WorkPackageHash,
};

use jam_codec::DecodeAll;

/// Follow the JAM chain.
///
/// Backed by JIP-2: `bestBlock`, `finalizedBlock`, `subscribeBestBlock`,
/// `subscribeFinalizedBlock`, `parent`, `stateRoot`, `beefyRoot`.
#[async_trait::async_trait]
pub trait JamChainSource: Send + Sync {
	async fn best_block(&self) -> Result<BlockDesc>;

	async fn finalized_block(&self) -> Result<BlockDesc>;

	/// Stream of best-block updates. Updates may coalesce (intermediate blocks skipped).
	async fn best_block_stream(&self) -> Result<BoxStream<'static, BlockDesc>>;

	/// Stream of finalized-block updates. Updates may coalesce. On non-validator polkajam nodes
	/// finality currently comes from a dummy gadget (roughly best-minus-one).
	async fn finalized_block_stream(&self) -> Result<BoxStream<'static, BlockDesc>>;

	async fn parent(&self, header_hash: HeaderHash) -> Result<BlockDesc>;

	/// The posterior state root of the block with the given header hash.
	async fn state_root(&self, header_hash: HeaderHash) -> Result<StateRootHash>;

	/// The BEEFY root of the block with the given header hash. Part of the refine context the
	/// builder assembles around an anchor.
	async fn beefy_root(&self, header_hash: HeaderHash) -> Result<MmrPeakHash>;

	/// The chain parameters (gas limits, recent-block window, slot period, ...).
	async fn parameters(&self) -> Result<VersionedParameters>;
}

/// Read JAM state: raw values, per-service values, range proofs, and typed decode helpers.
///
/// Backed by JIP-2 `serviceValue`/`subscribeServiceValue` plus the JIPs-PR-#16 methods
/// `stateValue`, `subscribeStateValue` and `stateProof`. The typed helpers (`auth_pools`,
/// `auth_queues`) are client-side decodes on top of `state_value`, NOT extra RPC methods.
#[async_trait::async_trait]
pub trait JamStateSource: Send + Sync {
	/// Value under the raw 31-byte state key in the posterior state of `at`.
	async fn state_value(&self, at: HeaderHash, key: StorageKey) -> Result<Option<Vec<u8>>>;

	/// Subscribe to changes of the value under the raw state key. An update is sent only when
	/// the value changes.
	async fn state_value_stream(
		&self,
		key: StorageKey,
		finalized: bool,
	) -> Result<BoxStream<'static, ChainSubUpdate<Option<Vec<u8>>>>>;

	/// Merkle proof for the state entries in the **inclusive** key range `[start_key, end_key]`
	/// in the posterior state of `at`. To prove a single key `k`, pass `(k, k)`.
	///
	/// `size_limit` softly bounds the total size of returned keys and values, in octets; if the
	/// limit cuts the range short, continue from just above the last returned key.
	async fn state_proof(
		&self,
		at: HeaderHash,
		start_key: StorageKey,
		end_key: StorageKey,
		size_limit: u32,
	) -> Result<RangeProof>;

	/// Value under `key` in the storage of service `service`, in the posterior state of `at`.
	async fn service_value(
		&self,
		at: HeaderHash,
		service: ServiceId,
		key: &[u8],
	) -> Result<Option<Vec<u8>>>;

	/// Subscribe to changes of a service-storage value. This is what "done" detection watches:
	/// the para head under key `[0x00] ‖ SCALE(ParaId)` in the parachain service.
	async fn service_value_stream(
		&self,
		service: ServiceId,
		key: &[u8],
		finalized: bool,
	) -> Result<BoxStream<'static, ChainSubUpdate<Option<Vec<u8>>>>>;

	/// The on-chain record of service `id` (code hash, balance, gas minimums), decoded from the
	/// service-info state key. `None` if there is no such service.
	async fn service_info(&self, at: HeaderHash, id: ServiceId) -> Result<Option<Service>> {
		let Some(bytes) = self.state_value(at, ServiceKey::Info { id }.into()).await? else {
			return Ok(None);
		};
		Ok(Some(
			Service::decode_all(&mut &bytes[..])
				.map_err(|error| Error::Other(format!("Service info decode failed: {error}")))?,
		))
	}

	/// The authorizer pools of all cores (state key C(1)), decoded. The pool is live headroom —
	/// a soft pre-flight check, never a per-block gate.
	async fn auth_pools(&self, at: HeaderHash) -> Result<AuthPools> {
		decode_system_value(self.state_value(at, SystemKey::AuthPools.into()).await?, "AuthPools")
	}

	/// The authorizer queues of all cores (state key C(2)), decoded. Scanning these for the own
	/// authorizer hash is how the collator confirms its core schedule.
	async fn auth_queues(&self, at: HeaderHash) -> Result<AuthQueues> {
		decode_system_value(self.state_value(at, SystemKey::AuthQueues.into()).await?, "AuthQueues")
	}

	/// The slot the block at `at` was authored in (state key C(11)).
	///
	/// No RPC maps a header hash back to its slot — `BlockDesc` only comes from the chain tips and
	/// from `parent` — so state is the way to date an arbitrary block.
	async fn current_time(&self, at: HeaderHash) -> Result<Slot> {
		decode_system_value(
			self.state_value(at, SystemKey::CurrentTime.into()).await?,
			"CurrentTime",
		)
	}

	/// The availability assignments of all cores (state key C(10)), decoded. One entry per core,
	/// holding the report guaranteed on that core while its data is still being made available.
	/// This is where a just-reported work package first becomes visible on chain.
	async fn availability(&self, at: HeaderHash) -> Result<AvailabilityAssignments> {
		decode_system_value(
			self.state_value(at, SystemKey::Availability.into()).await?,
			"AvailabilityAssignments",
		)
	}

	/// The accumulation queue (state key C(14)), decoded. Indexed by epoch phase rather than by
	/// core: each entry holds the reports that became available in that phase and are still
	/// waiting on their dependencies.
	async fn ready_queue(&self, at: HeaderHash) -> Result<ReadyQueue> {
		decode_system_value(self.state_value(at, SystemKey::ReadyQueue.into()).await?, "ReadyQueue")
	}
}

/// Submit work packages and follow their status.
///
/// Backed by JIP-2: `submitWorkPackage`, `submitWorkPackageBundle`, `subscribeWorkPackageStatus`.
#[async_trait::async_trait]
pub trait JamWorkPackageSubmission: Send + Sync {
	/// Submit a work package to the guarantors currently assigned to `core`.
	///
	/// Submission is one-shot: "submitted to at least one guarantor", no retry, no failure
	/// feedback. The only feedback is the status stream.
	async fn submit_work_package(
		&self,
		core: CoreIndex,
		package: &WorkPackage,
		extrinsics: Vec<Vec<u8>>,
	) -> Result<()>;

	/// Submit a pre-assembled work-package bundle to the guarantors assigned to `core`.
	async fn submit_bundle(&self, core: CoreIndex, bundle: Vec<u8>) -> Result<()>;

	/// Follow the status of a submitted work package.
	///
	/// `anchor` must be the anchor of the work package; `finalized` selects whether the status
	/// follows the finalized or the best chain. There is no "Accumulated" status by design —
	/// [`WorkPackageStatus::Ready`] only means "queued for accumulation"; completion is detected
	/// by watching the para head change ([`JamStateSource::service_value_stream`]).
	///
	/// A subscription error (e.g. anchor too old) surfaces as a final
	/// [`WorkPackageStatus::Failed`] item, after which the stream ends.
	async fn work_package_status_stream(
		&self,
		package_hash: WorkPackageHash,
		anchor: HeaderHash,
		finalized: bool,
	) -> Result<BoxStream<'static, WorkPackageStatus>>;
}

fn decode_system_value<T: DecodeAll>(value: Option<Vec<u8>>, what: &str) -> Result<T> {
	let bytes = value.ok_or_else(|| Error::Other(format!("{what} missing from JAM state")))?;
	T::decode_all(&mut &bytes[..])
		.map_err(|error| Error::Other(format!("{what} decode failed: {error}")))
}

#[cfg(test)]
mod tests {
	use super::*;
	use jam_codec::Encode;
	use jam_types::{AuthQueue, FixedVec};

	struct FixedState(Vec<u8>);

	#[async_trait::async_trait]
	impl JamStateSource for FixedState {
		async fn state_value(&self, _at: HeaderHash, _key: StorageKey) -> Result<Option<Vec<u8>>> {
			Ok(Some(self.0.clone()))
		}

		async fn state_value_stream(
			&self,
			_key: StorageKey,
			_finalized: bool,
		) -> Result<BoxStream<'static, ChainSubUpdate<Option<Vec<u8>>>>> {
			unimplemented!()
		}

		async fn state_proof(
			&self,
			_at: HeaderHash,
			_start_key: StorageKey,
			_end_key: StorageKey,
			_size_limit: u32,
		) -> Result<RangeProof> {
			unimplemented!()
		}

		async fn service_value(
			&self,
			_at: HeaderHash,
			_service: ServiceId,
			_key: &[u8],
		) -> Result<Option<Vec<u8>>> {
			unimplemented!()
		}

		async fn service_value_stream(
			&self,
			_service: ServiceId,
			_key: &[u8],
			_finalized: bool,
		) -> Result<BoxStream<'static, ChainSubUpdate<Option<Vec<u8>>>>> {
			unimplemented!()
		}
	}

	#[test]
	fn system_key_layout_matches_jip2() {
		let pools_key: StorageKey = SystemKey::AuthPools.into();
		let queues_key: StorageKey = SystemKey::AuthQueues.into();
		assert_eq!(pools_key.0[0], 1);
		assert_eq!(queues_key.0[0], 2);
		assert_eq!(StorageKey::from(SystemKey::Availability).0[0], 10);
		assert_eq!(StorageKey::from(SystemKey::ReadyQueue).0[0], 14);
		assert_eq!(&pools_key.0[1..], &[0u8; 30]);
	}

	#[test]
	fn auth_queues_default_impl_decodes_scale() {
		let authorizer_hash = AuthorizerHash([42u8; 32]);
		let queues: AuthQueues = FixedVec::new(AuthQueue::new(authorizer_hash));
		let source = FixedState(queues.encode());

		let decoded = futures::executor::block_on(source.auth_queues(HeaderHash::from([0u8; 32])))
			.expect("queues decode");
		assert!(decoded.iter().all(|queue| queue.iter().all(|hash| *hash == authorizer_hash)));
	}

	#[test]
	fn availability_default_impl_decodes_scale() {
		// An all-empty assignment is the common case, and it still has to round-trip: an empty
		// `FixedVec` is one entry per core, not zero bytes.
		let empty: AvailabilityAssignments = FixedVec::new(None);
		let source = FixedState(empty.encode());

		let decoded = futures::executor::block_on(source.availability(HeaderHash::from([0u8; 32])))
			.expect("availability decode");
		assert!(decoded.iter().all(Option::is_none));
	}

	#[test]
	fn decode_error_and_absence_are_reported() {
		let garbage = FixedState(vec![0xff]);
		let result: Result<AuthQueues> =
			futures::executor::block_on(garbage.auth_queues(HeaderHash::from([0u8; 32])));
		assert!(result.is_err());
	}
}
