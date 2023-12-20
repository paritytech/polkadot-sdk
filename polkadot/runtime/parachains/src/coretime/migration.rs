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

//! Migrations for the Coretime pallet.

pub use v_coretime::{GetLegacyLease, MigrateToCoretime};

mod v_coretime {
	#[cfg(feature = "try-runtime")]
	use crate::scheduler::common::FixedAssignmentProvider;
	use crate::{
		assigner_coretime, configuration,
		coretime::{BrokerRuntimePallets, Config, PartsOf57600, WeightInfo},
		paras,
	};
	#[cfg(feature = "try-runtime")]
	use frame_support::ensure;
	use frame_support::{
		traits::{OnRuntimeUpgrade, PalletInfoAccess, StorageVersion},
		weights::Weight,
	};
	use frame_system::pallet_prelude::BlockNumberFor;
	use pallet_broker::{CoreAssignment, CoreMask, ScheduleItem};
	use parity_scale_codec::Encode;
	use polkadot_parachain_primitives::primitives::IsSystem;
	use primitives::{CoreIndex, Id as ParaId};
	use sp_core::Get;
	use sp_runtime::BoundedVec;
	#[cfg(feature = "try-runtime")]
	use sp_std::vec::Vec;
	use sp_std::{iter, prelude::*, result};
	use xcm::v3::{
		send_xcm, Instruction, Junction, Junctions, MultiLocation, OriginKind, SendError,
		WeightLimit, Xcm,
	};

	/// Return information about a legacy lease of a parachain.
	pub trait GetLegacyLease<N> {
		/// If parachain is a lease holding parachain, return the block at which the lease expires.
		fn get_parachain_lease_in_blocks(para: ParaId) -> Option<N>;
	}

	/// Migrate a chain to use coretime.
	///
	/// This assumes that the `Coretime` and the `AssignerCoretime` pallets are added at the same
	/// time to a runtime.
	pub struct MigrateToCoretime<T, SendXcm, LegacyLease>(
		sp_std::marker::PhantomData<(T, SendXcm, LegacyLease)>,
	);

	impl<T: Config, SendXcm: xcm::v3::SendXcm, LegacyLease: GetLegacyLease<BlockNumberFor<T>>>
		MigrateToCoretime<T, SendXcm, LegacyLease>
	{
		fn already_migrated() -> bool {
			// We are using the assigner coretime because the coretime pallet doesn't has any
			// storage data. But both pallets are introduced at the same time, so this is fine.
			let name_hash = assigner_coretime::Pallet::<T>::name_hash();
			let mut next_key = name_hash.to_vec();
			let storage_version_key = StorageVersion::storage_key::<assigner_coretime::Pallet<T>>();

			loop {
				match sp_io::storage::next_key(&next_key) {
					// StorageVersion is initialized before, so we need to ingore it.
					Some(key) if &key == &storage_version_key => {
						next_key = key;
					},
					// If there is any other key with the prefix of the pallet,
					// we already have executed the migration.
					Some(key) if key.starts_with(&name_hash) => {
						log::info!("`MigrateToCoretime` already executed!");
						return true
					},
					// Any other key/no key means that we did not yet have migrated.
					None | Some(_) => return false,
				}
			}
		}
	}

	impl<T: Config, SendXcm: xcm::v3::SendXcm, LegacyLease: GetLegacyLease<BlockNumberFor<T>>>
		OnRuntimeUpgrade for MigrateToCoretime<T, SendXcm, LegacyLease>
	{
		fn on_runtime_upgrade() -> Weight {
			if Self::already_migrated() {
				return Weight::zero()
			}

			log::info!("Migrating existing parachains to coretime.");
			migrate_to_coretime::<T, SendXcm, LegacyLease>()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			if Self::already_migrated() {
				return Ok(Vec::new())
			}

			let legacy_paras = paras::Parachains::<T>::get();
			let config = <configuration::Pallet<T>>::config();
			let total_core_count = config.coretime_cores + legacy_paras.len() as u32;

			let bytes = u32::to_be_bytes(total_core_count as u32);

			Ok(bytes.to_vec())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			if state.is_empty() {
				return Ok(())
			}

			log::trace!("Running post_upgrade()");

			let prev_core_count = u32::from_be_bytes(state.try_into().unwrap());
			let new_core_count = assigner_coretime::Pallet::<T>::session_core_count();
			ensure!(new_core_count == prev_core_count, "Total number of cores need to not change.");

			Ok(())
		}
	}

	// Migrate to Coretime.
	//
	// NOTE: Also migrates coretime_cores config value in configuration::ActiveConfig.
	fn migrate_to_coretime<
		T: Config,
		SendXcm: xcm::v3::SendXcm,
		LegacyLease: GetLegacyLease<BlockNumberFor<T>>,
	>() -> Weight {
		let legacy_paras = paras::Pallet::<T>::parachains();
		let legacy_count = legacy_paras.len() as u32;
		let now = <frame_system::Pallet<T>>::block_number();
		for (core, para_id) in legacy_paras.into_iter().enumerate() {
			let r = assigner_coretime::Pallet::<T>::assign_core(
				CoreIndex(core as u32),
				now,
				vec![(CoreAssignment::Task(para_id.into()), PartsOf57600::FULL)],
				None,
			);
			if let Err(err) = r {
				log::error!(
					"Creating assignment for existing para failed: {:?}, error: {:?}",
					para_id,
					err
				);
			}
		}

		let config = <configuration::Pallet<T>>::config();
		// coretime_cores was on_demand_cores until now:
		for on_demand in 0..config.coretime_cores {
			let core = CoreIndex(legacy_count.saturating_add(on_demand as _));
			let r = assigner_coretime::Pallet::<T>::assign_core(
				core,
				now,
				vec![(CoreAssignment::Pool, PartsOf57600::FULL)],
				None,
			);
			if let Err(err) = r {
				log::error!("Creating assignment for existing on-demand core, failed: {:?}", err);
			}
		}
		let total_cores = config.coretime_cores + legacy_count;
		configuration::ActiveConfig::<T>::mutate(|c| {
			c.coretime_cores = total_cores;
		});

		if let Err(err) = migrate_send_assignments_to_coretime_chain::<T, SendXcm, LegacyLease>() {
			log::error!("Sending legacy chain data to coretime chain failed: {:?}", err);
		}

		let single_weight = <T as Config>::WeightInfo::assign_core(1);
		single_weight
			.saturating_mul(u64::from(legacy_count.saturating_add(config.coretime_cores)))
			// Second read from sending assignments to the coretime chain.
			.saturating_add(T::DbWeight::get().reads_writes(2, 1))
	}

	fn migrate_send_assignments_to_coretime_chain<
		T: Config,
		SendXcm: xcm::v3::SendXcm,
		LegacyLease: GetLegacyLease<BlockNumberFor<T>>,
	>() -> result::Result<(), SendError> {
		let legacy_paras = paras::Pallet::<T>::parachains();
		let (system_chains, lease_holding): (Vec<_>, Vec<_>) =
			legacy_paras.into_iter().partition(IsSystem::is_system);

		let reservations = system_chains.into_iter().map(|p| {
			let schedule = BoundedVec::truncate_from(vec![ScheduleItem {
				mask: CoreMask::complete(),
				assignment: CoreAssignment::Task(p.into()),
			}]);
			mk_coretime_call(crate::coretime::CoretimeCalls::Reserve(schedule))
		});

		let leases = lease_holding.into_iter().filter_map(|p| {
			let Some(valid_until) = LegacyLease::get_parachain_lease_in_blocks(p) else {
				log::error!("Lease holding chain with no lease information?!");
				return None
			};
			let valid_until: u32 = match valid_until.try_into() {
				Ok(val) => val,
				Err(_) => {
					log::error!("Converting block number to u32 failed!");
					return None
				},
			};
			// We assume the coretime chain set this parameter to the recommened value in RFC-1:
			const TIME_SLICE_PERIOD: u32 = 80;
			let round_up = if valid_until % TIME_SLICE_PERIOD > 0 { 1 } else { 0 };
			let time_slice = valid_until / TIME_SLICE_PERIOD + TIME_SLICE_PERIOD * round_up;
			Some(mk_coretime_call(crate::coretime::CoretimeCalls::SetLease(p.into(), time_slice)))
		});

		let message_content = iter::once(Instruction::UnpaidExecution {
			weight_limit: WeightLimit::Unlimited,
			check_origin: None,
		})
		.chain(reservations)
		.chain(leases)
		.collect();

		let message = Xcm(message_content);
		send_xcm::<SendXcm>(
			MultiLocation {
				parents: 0,
				interior: Junctions::X1(Junction::Parachain(T::BrokerId::get())),
			},
			message,
		)?;
		Ok(())
	}

	fn mk_coretime_call(call: crate::coretime::CoretimeCalls) -> Instruction<()> {
		Instruction::Transact {
			origin_kind: OriginKind::Native,
			require_weight_at_most: Weight::from_parts(1000000000, 200000),
			call: BrokerRuntimePallets::Broker(call).encode().into(),
		}
	}
}
