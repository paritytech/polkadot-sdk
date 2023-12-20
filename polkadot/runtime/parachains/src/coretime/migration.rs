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

//! Migration of legacy parachains to Coretime.

use sp_runtime::BoundedVec;
use sp_std::{iter, prelude::*, result};

use parity_scale_codec::Encode;

use sp_core::Get;

use frame_support::weights::Weight;
use pallet_broker::{CoreAssignment, CoreMask, ScheduleItem};
use xcm::v3::{
	send_xcm, Instruction, Junction, Junctions, MultiLocation, OriginKind, SendError, WeightLimit,
	Xcm,
};

use crate::{configuration, paras};
use polkadot_parachain_primitives::primitives::IsSystem;
use primitives::CoreIndex;

use super::{BrokerRuntimePallets, Config, GetLegacyLease, Pallet, PartsOf57600, WeightInfo};
use crate::assigner_coretime;

pub mod v_coretime {

	#[cfg(feature = "try-runtime")]
	use sp_std::vec::Vec;

	#[cfg(feature = "try-runtime")]
	use frame_support::ensure;
	use frame_support::{
		migrations::VersionedMigration, traits::OnRuntimeUpgrade, weights::Weight,
	};

	#[cfg(feature = "try-runtime")]
	use crate::{
		assigner_coretime, configuration, paras, scheduler::common::FixedAssignmentProvider,
	};

	use super::{Config, Pallet};

	#[allow(deprecated)]
	pub type MigrateToCoretime<T> = VersionedMigration<
		0,
		1,
		UncheckedMigrateToCoretime<T>,
		Pallet<T>,
		<T as frame_system::Config>::DbWeight,
	>;

	pub struct UncheckedMigrateToCoretime<T>(sp_std::marker::PhantomData<T>);

	impl<T: Config> OnRuntimeUpgrade for UncheckedMigrateToCoretime<T> {
		fn on_runtime_upgrade() -> Weight {
			log::info!("Migrating existing parachains to coretime.");
			super::migrate_to_coretime::<T>()
		}

		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			let legacy_paras = paras::Parachains::<T>::get();
			let config = <configuration::Pallet<T>>::config();
			let total_core_count = config.coretime_cores + legacy_paras.len() as u32;

			let bytes = u32::to_be_bytes(total_core_count as u32);

			Ok(bytes.to_vec())
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			log::trace!("Running post_upgrade()");

			let prev_core_count = u32::from_be_bytes(state.try_into().unwrap());
			let new_core_count = assigner_coretime::Pallet::<T>::session_core_count();
			ensure!(new_core_count == prev_core_count, "Total number of cores need to not change.");

			Ok(())
		}
	}
}

// Migrate to Coretime.
//
// NOTE: Also migrates coretime_cores config value in configuration::ActiveConfig.
pub fn migrate_to_coretime<T: Config>() -> Weight {
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

	if let Err(err) = migrate_send_assignments_to_coretime_chain::<T>() {
		log::error!("Sending legacy chain data to coretime chain failed: {:?}", err);
	}

	let single_weight = <T as Config>::WeightInfo::assign_core(1);
	single_weight
		.saturating_mul(u64::from(legacy_count.saturating_add(config.coretime_cores)))
		// Second read from sending assignments to the coretime chain.
		.saturating_add(T::DbWeight::get().reads_writes(2, 1))
}

fn migrate_send_assignments_to_coretime_chain<T: Config>() -> result::Result<(), SendError> {
	let legacy_paras = paras::Pallet::<T>::parachains();
	let (system_chains, lease_holding): (Vec<_>, Vec<_>) =
		legacy_paras.into_iter().partition(IsSystem::is_system);

	let reservations = system_chains.into_iter().map(|p| {
		let schedule = BoundedVec::truncate_from(vec![ScheduleItem {
			mask: CoreMask::complete(),
			assignment: CoreAssignment::Task(p.into()),
		}]);
		mk_coretime_call(super::CoretimeCalls::Reserve(schedule))
	});

	let leases = lease_holding.into_iter().filter_map(|p| {
		let Some(valid_until) = T::GetLegacyLease::get_parachain_lease_in_blocks(p) else {
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
		Some(mk_coretime_call(super::CoretimeCalls::SetLease(p.into(), time_slice)))
	});

	let message_content = iter::once(Instruction::UnpaidExecution {
		weight_limit: WeightLimit::Unlimited,
		check_origin: None,
	})
	.chain(reservations)
	.chain(leases)
	.collect();

	let message = Xcm(message_content);
	send_xcm::<T::SendXcm>(
		MultiLocation {
			parents: 0,
			interior: Junctions::X1(Junction::Parachain(T::BrokerId::get())),
		},
		message,
	)?;
	Ok(())
}

fn mk_coretime_call(call: super::CoretimeCalls) -> Instruction<()> {
	Instruction::Transact {
		origin_kind: OriginKind::Native,
		require_weight_at_most: Weight::from_parts(1000000000, 200000),
		call: BrokerRuntimePallets::Broker(call).encode().into(),
	}
}
