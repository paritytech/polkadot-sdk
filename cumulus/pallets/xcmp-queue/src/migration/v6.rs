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

//! Migrates the storage to version 6.

use crate::*;
use frame_support::{pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade};

/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), that is performed only
/// when the on-chain version is 5.
pub type MigrateV5ToV6<T> = frame_support::migrations::VersionedMigration<
	5,
	6,
	unversioned::UncheckedMigrateV5ToV6<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

// V5 storage aliases
mod v5 {
	use super::*;

	#[derive(Clone, Eq, PartialEq, Encode, Decode, TypeInfo, Debug, MaxEncodedLen)]
	pub struct OutboundChannelDetails {
		/// The `ParaId` of the parachain that this channel is connected with.
		pub recipient: ParaId,
		/// The state of the channel.
		pub state: OutboundState,
		/// Whether any signals exist in this channel.
		pub signals_exist: bool,
		/// The index of the first outbound message.
		pub first_index: u16,
		/// The index of the last outbound message.
		pub last_index: u16,
	}

	#[frame_support::storage_alias]
	pub(super) type OutboundXcmpStatus<T: Config> = StorageValue<
		Pallet<T>,
		BoundedVec<OutboundChannelDetails, <T as Config>::MaxActiveOutboundChannels>,
		ValueQuery,
	>;
}

mod unversioned {
	use super::*;
	pub struct UncheckedMigrateV5ToV6<T: Config>(PhantomData<T>);
}

impl<T: Config> UncheckedOnRuntimeUpgrade for unversioned::UncheckedMigrateV5ToV6<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let translate = |pre: BoundedVec<
			v5::OutboundChannelDetails,
			<T as Config>::MaxActiveOutboundChannels,
		>|
		 -> BoundedVec<OutboundChannelDetails, T::MaxActiveOutboundChannels> {
			BoundedVec::truncate_from(
				pre.into_inner()
					.iter()
					.map(|pre_channel_details| OutboundChannelDetails {
						recipient: pre_channel_details.recipient,
						state: pre_channel_details.state,
						signals_exist: pre_channel_details.signals_exist,
						first_index: pre_channel_details.first_index,
						last_index: pre_channel_details.last_index,
						flags: OutboundChannelFlags::empty(),
					})
					.collect(),
			)
		};

		if OutboundXcmpStatus::<T>::translate(|pre| pre.map(translate)).is_err() {
			tracing::error!(
				target: LOG_TARGET,
				"unexpected error when performing translation of the OutboundXcmpStatus type \
				during storage upgrade to v6"
			);
		}

		T::DbWeight::get().reads_writes(1, 1)
	}
}
