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

//! Migrates the storage to version 4.

use crate::*;
use cumulus_primitives_core::ListChannelInfos;
use frame_support::{
	pallet_prelude::*,
	traits::{Get, OnRuntimeUpgrade},
};

/// Configs needed to run the V4 migration.
pub trait V4Config: Config {
	/// List all outbound channels with their target `ParaId` and maximum message size.
	type ChannelList: ListChannelInfos;
}

pub type MigrateV3ToV4<T> = frame_support::migrations::VersionedMigration<
	3,
	4,
	UncheckedMigrateV3ToV4<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

// V3 storage aliases
mod v3 {
	use super::*;

	#[frame_support::storage_alias]
	pub(super) type OutboundXcmpStatus<T: Config> =
		StorageValue<Pallet<T>, Vec<OutboundChannelDetails>, ValueQuery>;

	#[frame_support::storage_alias]
	pub(super) type OutboundXcmpMessages<T: Config> = StorageDoubleMap<
		Pallet<T>,
		Blake2_128Concat,
		ParaId,
		Twox64Concat,
		u16,
		Vec<u8>,
		ValueQuery,
	>;

	#[frame_support::storage_alias]
	pub(super) type SignalMessages<T: Config> =
		StorageMap<Pallet<T>, Blake2_128Concat, ParaId, Vec<u8>, ValueQuery>;
}

/// Please use [`MigrateV3ToV4`] instead.
pub struct UncheckedMigrateV3ToV4<T: V4Config>(core::marker::PhantomData<T>);

impl<T: V4Config> OnRuntimeUpgrade for UncheckedMigrateV3ToV4<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
		// We dont need any front-run protection for this since channels are opened by governance.
		let n = v3::OutboundXcmpStatus::<T>::get().len() as u32;
		ensure!(n <= T::MaxActiveOutboundChannels::get(), "Too many outbound channels.");

		// Check if any channels have a too large message max size.
		let max_msg_len = T::MaxPageSize::get() - XcmpMessageFormat::max_encoded_len() as u32;

		for channel in T::ChannelList::outgoing_channels() {
			let info = T::ChannelInfo::get_channel_info(channel)
				.expect("All listed channels must provide info");

			ensure!(
				info.max_message_size <= max_msg_len,
				"Max message size for channel is too large. This means that the V4 migration can \
				be front-run and an attacker could place a large message just right before the \
				migration to make other messages un-decodable. Please either increase \
				`MaxPageSize` or decrease the `max_message_size` for this channel.",
			);
		}

		// Now check that all pages still fit into the new `BoundedVec`s:
		for page in v3::OutboundXcmpMessages::<T>::iter_values() {
			ensure!(
				page.len() < T::MaxPageSize::get() as usize,
				"Too long message in storage. Manual intervention required."
			);
		}

		for page in v3::SignalMessages::<T>::iter_values() {
			ensure!(
				page.len() < T::MaxPageSize::get() as usize,
				"Too long signal in storage. Manual intervention required."
			);
		}

		Ok(())
	}
}
