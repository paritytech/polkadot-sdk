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

//! Migrates the storage to version 5.

use crate::*;
use cumulus_primitives_core::ListChannelInfos;
use frame_support::{pallet_prelude::*, traits::UncheckedOnRuntimeUpgrade};

/// Configs needed to run the V5 migration.
pub trait V5Config: Config {
	/// List all outbound channels with their target `ParaId` and maximum message size.
	type ChannelList: ListChannelInfos;
}

/// Ensures that the storage migrates cleanly to V5.
///
/// The migration itself is a no-op, but it checks that none of the `BoundedVec`s would truncate on
/// the next decode after the upgrade was applied.
pub type MigrateV4ToV5<T> = frame_support::migrations::VersionedMigration<
	4,
	5,
	unversioned::UncheckedMigrateV4ToV5<T>,
	Pallet<T>,
	<T as frame_system::Config>::DbWeight,
>;

// V4 storage aliases
mod v4 {
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

// Private module to hide the migration.
mod unversioned {
	/// Please use [`MigrateV4ToV5`] instead.
	pub struct UncheckedMigrateV4ToV5<T: super::V5Config>(core::marker::PhantomData<T>);
}

impl<T: V5Config> UncheckedOnRuntimeUpgrade for unversioned::UncheckedMigrateV4ToV5<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		Default::default()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
		// We dont need any front-run protection for this since channels are opened by governance.
		ensure!(
			v4::OutboundXcmpStatus::<T>::get().len() as u32 <= T::MaxActiveOutboundChannels::get(),
			"Too many outbound channels. Close some channels or increase `MaxActiveOutboundChannels`."
		);

		ensure!(T::MaxPageSize::get() >= 16, "Sanity check failed: MaxPageSize too small");

		// Check if any channels have a too large message max sizes.
		let max_msg_len = T::MaxPageSize::get() - XcmpMessageFormat::max_encoded_len() as u32;
		for channel in T::ChannelList::outgoing_channels() {
			let info = T::ChannelInfo::get_channel_info(channel)
				.expect("All listed channels must provide info");

			if info.max_message_size > max_msg_len {
				log::error!(
					"Max message size for channel is too large. This means that the V5 \
				migration can be front-run and an attacker could place a large message just right \
				before the migration to make other messages un-decodable. Please either increase \
				`MaxPageSize` or decrease the `max_message_size` for this channel. Channel max: {}, \
				MaxPageSize: {}",
					info.max_message_size,
					max_msg_len
				);
				return Err("Migration can be front-run".into());
			}
		}

		Ok(())
	}
}
