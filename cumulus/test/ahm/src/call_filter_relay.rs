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

//! Asset Hub Migration tests.

use crate::porting_prelude::*;

use frame_support::{sp_runtime::traits::Dispatchable, traits::Contains};
use pallet_rc_migrator::*;
use polkadot_primitives::Id as ParaId;
use polkadot_runtime::{
	Block, BuildStorage, RcMigrator, Runtime as T, RuntimeCall, RuntimeOrigin, System,
};
use remote_externalities::{Builder, Mode, OfflineConfig, RemoteExternalities};
use runtime_parachains::inclusion::AggregateMessageOrigin;
use sp_runtime::AccountId32;

/// Check that the call filtering mechanism works.
#[test]
#[cfg(not(feature = "ahm-westend"))] // FIXME make work on Westend
fn call_filter_works() {
	let mut t: sp_io::TestExternalities =
		frame_system::GenesisConfig::<T>::default().build_storage().unwrap().into();

	// MQ calls are never filtered:
	let mq_call = RuntimeCall::MessageQueue(pallet_message_queue::Call::<T>::reap_page {
		message_origin: AggregateMessageOrigin::Ump(
			runtime_parachains::inclusion::UmpQueueId::Para(ParaId::from(1000)),
		),
		page_index: 0,
	});
	// Balances calls are filtered during the migration:
	let balances_call = RuntimeCall::Balances(pallet_balances::Call::<T>::transfer_all {
		dest: AccountId32::from([0; 32]).into(),
		keep_alive: false,
	});
	// Indices calls are filtered during and after the migration:
	let indices_call = RuntimeCall::Indices(pallet_indices::Call::<T>::claim { index: 0 });

	let is_allowed = |call: &RuntimeCall| Pallet::<T>::contains(call);

	// Try the BaseCallFilter
	t.execute_with(|| {
		// Before the migration starts
		{
			RcMigrationStage::<T>::put(MigrationStage::Pending);

			assert!(is_allowed(&mq_call));
			assert!(is_allowed(&balances_call));
			assert!(is_allowed(&indices_call));
		}

		// During the migration
		{
			RcMigrationStage::<T>::put(MigrationStage::ProxyMigrationInit);

			assert!(is_allowed(&mq_call));
			assert!(!is_allowed(&balances_call));
			assert!(!is_allowed(&indices_call));
		}

		// After the migration
		{
			RcMigrationStage::<T>::put(MigrationStage::MigrationDone);

			assert!(is_allowed(&mq_call));
			assert!(is_allowed(&balances_call));
			assert!(!is_allowed(&indices_call));
		}
	});

	// Try to actually dispatch the calls
	t.execute_with(|| {
		let _ =
			<pallet_balances::Pallet<T> as frame_support::traits::Currency<_>>::deposit_creating(
				&AccountId32::from([0; 32]),
				u64::MAX.into(),
			);

		// Before the migration starts
		{
			RcMigrationStage::<T>::put(MigrationStage::Pending);

			assert!(!is_forbidden(&mq_call));
			assert!(!is_forbidden(&balances_call));
			assert!(!is_forbidden(&indices_call));
		}

		// During the migration
		{
			RcMigrationStage::<T>::put(MigrationStage::ProxyMigrationInit);

			assert!(!is_forbidden(&mq_call));
			assert!(is_forbidden(&balances_call));
			assert!(is_forbidden(&indices_call));
		}

		// After the migration
		{
			RcMigrationStage::<T>::put(MigrationStage::MigrationDone);

			assert!(!is_forbidden(&mq_call));
			assert!(!is_forbidden(&balances_call));
			assert!(is_forbidden(&indices_call));
		}
	});
}

/// Whether a call is forbidden by the call filter.
fn is_forbidden(call: &RuntimeCall) -> bool {
	let Err(err) = call.clone().dispatch(RuntimeOrigin::signed(AccountId32::from([0; 32]))) else {
		return false;
	};

	let filtered_err: sp_runtime::DispatchError = frame_system::Error::<T>::CallFiltered.into();
	err.error == filtered_err
}
