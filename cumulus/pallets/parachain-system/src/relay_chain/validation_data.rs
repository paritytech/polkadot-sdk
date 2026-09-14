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

//! Reads of relay-chain state performed by the `set_validation_data` inherent.

use alloc::vec::Vec;
use cumulus_primitives_core::{
	relay_chain::{BlockNumber, Header as RelayHeader, UpgradeGoAhead, UpgradeRestriction},
	AbridgedHostConfiguration, PersistedValidationData, VerifySchedulingSignature,
};
use frame_support::traits::Get;
use sp_trie::StorageProof;

use crate::{
	relay_chain::{
		descendant_validation,
		relay_state_snapshot::{MessagingStateSnapshot, RelayChainStateProof},
		Hash,
	},
	AggregatedUnincludedSegment, CheckAssociatedRelayNumber, Config, LastRelayChainBlockNumber,
	SegmentTracker,
};

/// Check that the associated relay chain block number is as expected.
pub(crate) fn check_associated_relay_number<T: Config>(current: BlockNumber) {
	T::CheckAssociatedRelayNumber::check_associated_relay_number(
		current,
		LastRelayChainBlockNumber::<T>::get(),
	);
}

/// Build the verified relay-state proof carried by the inherent.
pub(crate) fn relay_state_proof<T: Config>(
	vfp: &PersistedValidationData,
	relay_chain_state: StorageProof,
) -> RelayChainStateProof {
	RelayChainStateProof::from_inherent_proof(
		T::SelfParaId::get(),
		vfp.relay_parent_storage_root,
		relay_chain_state,
	)
	.expect("Invalid relay chain state proof")
}

/// Validate the provided relay parent descendants, unless V3 scheduling validation is enabled.
pub(crate) fn verify_relay_parent_descendants<T: Config>(
	relay_state_proof: &RelayChainStateProof,
	relay_parent_descendants: Vec<RelayHeader>,
	relay_parent_storage_root: Hash,
) {
	let expected_rp_descendants_num = T::RelayParentOffset::get();
	let v3_enabled = T::SchedulingSignatureVerifier::V3_SCHEDULING_ENABLED;

	if expected_rp_descendants_num > 0 && !v3_enabled {
		if let Err(err) = descendant_validation::verify_relay_parent_descendants(
			relay_state_proof,
			relay_parent_descendants,
			relay_parent_storage_root,
			expected_rp_descendants_num,
		) {
			panic!(
				"Unable to verify provided relay parent descendants. \
				expected_rp_descendants_num: {expected_rp_descendants_num} \
				error: {err:?}"
			);
		};
	}
}

/// Deposit a log indicating the relay-parent storage root.
pub(crate) fn deposit_relay_parent_storage_root<T: Config>(
	relay_parent_storage_root: Hash,
	relay_parent_number: BlockNumber,
) {
	frame_system::Pallet::<T>::deposit_log(
		cumulus_primitives_core::rpsr_digest::relay_parent_storage_root_item(
			relay_parent_storage_root,
			relay_parent_number,
		),
	);
}

/// The relay-chain reads extracted from the `set_validation_data` inherent.
pub(crate) struct RelayStateReads {
	/// The upgrade go-ahead signal from the relay chain.
	pub upgrade_go_ahead_signal: Option<UpgradeGoAhead>,
	/// The upgrade signal already consumed by an unincluded ancestor, if any.
	pub upgrade_signal_in_segment: Option<UpgradeGoAhead>,
	/// The upgrade restriction signal from the relay chain.
	pub upgrade_restriction_signal: Option<UpgradeRestriction>,
	/// The abridged host configuration from the relay chain.
	pub host_config: AbridgedHostConfiguration,
	/// The relevant messaging state snapshot from the relay chain.
	pub relevant_messaging_state: MessagingStateSnapshot,
}

/// Read the relay-chain state carried by the validated proof.
pub(crate) fn read_relay_state<T: Config>(
	relay_state_proof: &RelayChainStateProof,
) -> RelayStateReads {
	let upgrade_go_ahead_signal = relay_state_proof
		.read_upgrade_go_ahead_signal()
		.expect("Invalid upgrade go ahead signal");

	let upgrade_signal_in_segment = AggregatedUnincludedSegment::<T>::get()
		.as_ref()
		.and_then(SegmentTracker::consumed_go_ahead_signal);

	let upgrade_restriction_signal = relay_state_proof
		.read_upgrade_restriction_signal()
		.expect("Invalid upgrade restriction signal");

	let host_config = relay_state_proof
		.read_abridged_host_configuration()
		.expect("Invalid host configuration in relay chain state proof");

	let relevant_messaging_state = relay_state_proof
		.read_messaging_state_snapshot(&host_config)
		.expect("Invalid messaging state in relay chain state proof");

	RelayStateReads {
		upgrade_go_ahead_signal,
		upgrade_signal_in_segment,
		upgrade_restriction_signal,
		host_config,
		relevant_messaging_state,
	}
}
