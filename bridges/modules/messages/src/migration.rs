// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! A module that is responsible for migration of storage.

use crate::{Config, Pallet};
use frame_support::{
	traits::{Get, StorageVersion},
	weights::Weight,
};

/// The in-code storage version.
pub const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

/// This module contains data structures that are valid for the initial state of `0`.
/// (used with v1 migration).
pub mod v0 {
	use super::Config;
	use crate::BridgedChainOf;
	use bp_messages::{MessageNonce, UnrewardedRelayer};
	use bp_runtime::AccountIdOf;
	use codec::{Decode, Encode};
	use sp_std::collections::vec_deque::VecDeque;

	#[derive(Encode, Decode, Clone, PartialEq, Eq)]
	pub(crate) struct StoredInboundLaneData<T: Config<I>, I: 'static>(
		pub(crate) InboundLaneData<AccountIdOf<BridgedChainOf<T, I>>>,
	);
	#[derive(Encode, Decode, Clone, PartialEq, Eq)]
	pub(crate) struct InboundLaneData<RelayerId> {
		pub(crate) relayers: VecDeque<UnrewardedRelayer<RelayerId>>,
		pub(crate) last_confirmed_nonce: MessageNonce,
	}
	#[derive(Encode, Decode, Clone, PartialEq, Eq)]
	pub(crate) struct OutboundLaneData {
		pub(crate) oldest_unpruned_nonce: MessageNonce,
		pub(crate) latest_received_nonce: MessageNonce,
		pub(crate) latest_generated_nonce: MessageNonce,
	}
}

/// This migration to `1` updates the metadata of `InboundLanes` and `OutboundLanes` to the new
/// structures.
pub mod v1 {
	use super::*;
	use crate::{
		InboundLaneData, InboundLanes, OutboundLaneData, OutboundLanes, StoredInboundLaneData,
	};
	use bp_messages::LaneState;
	use frame_support::traits::UncheckedOnRuntimeUpgrade;
	use sp_std::marker::PhantomData;

	/// Migrates the pallet storage to v1.
	pub struct UncheckedMigrationV0ToV1<T, I>(PhantomData<(T, I)>);

	impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for UncheckedMigrationV0ToV1<T, I> {
		fn on_runtime_upgrade() -> Weight {
			let mut weight = T::DbWeight::get().reads(1);

			// `InboundLanes` - add state to the old structs
			let translate_inbound =
				|pre: v0::StoredInboundLaneData<T, I>| -> Option<v1::StoredInboundLaneData<T, I>> {
					weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
					Some(v1::StoredInboundLaneData(v1::InboundLaneData {
						state: LaneState::Opened,
						relayers: pre.0.relayers,
						last_confirmed_nonce: pre.0.last_confirmed_nonce,
					}))
				};
			InboundLanes::<T, I>::translate_values(translate_inbound);

			// `OutboundLanes` - add state to the old structs
			let translate_outbound = |pre: v0::OutboundLaneData| -> Option<v1::OutboundLaneData> {
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
				Some(v1::OutboundLaneData {
					state: LaneState::Opened,
					oldest_unpruned_nonce: pre.oldest_unpruned_nonce,
					latest_received_nonce: pre.latest_received_nonce,
					latest_generated_nonce: pre.latest_generated_nonce,
				})
			};
			OutboundLanes::<T, I>::translate_values(translate_outbound);

			weight
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, sp_runtime::DispatchError> {
			use codec::Encode;

			let number_of_inbound_to_migrate = InboundLanes::<T, I>::iter_keys().count();
			let number_of_outbound_to_migrate = OutboundLanes::<T, I>::iter_keys().count();
			Ok((number_of_inbound_to_migrate as u32, number_of_outbound_to_migrate as u32).encode())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: sp_std::vec::Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			use codec::Decode;
			const LOG_TARGET: &str = "runtime::bridge-messages-migration";

			let (number_of_inbound_to_migrate, number_of_outbound_to_migrate): (u32, u32) =
				Decode::decode(&mut &state[..]).unwrap();
			let number_of_inbound = InboundLanes::<T, I>::iter_keys().count();
			let number_of_outbound = OutboundLanes::<T, I>::iter_keys().count();

			tracing::info!(target: LOG_TARGET, %number_of_inbound_to_migrate, "post-upgrade expects inbound lanes to have been migrated.");
			tracing::info!(target: LOG_TARGET, %number_of_outbound_to_migrate, "post-upgrade expects outbound lanes to have been migrated.");

			frame_support::ensure!(
				number_of_inbound_to_migrate as usize == number_of_inbound,
				"must migrate all `InboundLanes`."
			);
			frame_support::ensure!(
				number_of_outbound_to_migrate as usize == number_of_outbound,
				"must migrate all `OutboundLanes`."
			);

			tracing::info!(target: LOG_TARGET, "migrated all.");
			Ok(())
		}
	}

	/// [`UncheckedMigrationV0ToV1`] wrapped in a
	/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), ensuring the
	/// migration is only performed when on-chain version is 0.
	pub type MigrationToV1<T, I> = frame_support::migrations::VersionedMigration<
		0,
		1,
		UncheckedMigrationV0ToV1<T, I>,
		Pallet<T, I>,
		<T as frame_system::Config>::DbWeight,
	>;
}
