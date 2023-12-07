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
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! Tests for the Rococo Runtime Configuration

use crate::*;
use std::collections::HashSet;

use frame_support::traits::WhitelistedStorageKeys;
use sp_core::hexdisplay::HexDisplay;

#[test]
fn check_whitelist() {
	let whitelist: HashSet<String> = AllPalletsWithSystem::whitelisted_storage_keys()
		.iter()
		.map(|e| HexDisplay::from(&e.key).to_string())
		.collect();

	// Block number
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac"));
	// Total issuance
	assert!(whitelist.contains("c2261276cc9d1f8598ea4b6a74b15c2f57c875e4cff74148e4628f264b974c80"));
	// Execution phase
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef7ff553b5a9862a516939d82b3d3d8661a"));
	// Event count
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef70a98fdbe9ce6c55837576c60c7af3850"));
	// System events
	assert!(whitelist.contains("26aa394eea5630e07c48ae0c9558cef780d41e5e16056765bc8461851072c9d7"));
	// XcmPallet VersionDiscoveryQueue
	assert!(whitelist.contains("1405f2411d0af5a7ff397e7c9dc68d194a222ba0333561192e474c59ed8e30e1"));
	// XcmPallet SafeXcmVersion
	assert!(whitelist.contains("1405f2411d0af5a7ff397e7c9dc68d196323ae84c43568be0d1394d5d0d522c4"));
}

#[test]
fn check_treasury_pallet_id() {
	assert_eq!(
		<Treasury as frame_support::traits::PalletInfoAccess>::index() as u8,
		rococo_runtime_constants::TREASURY_PALLET_ID
	);
}

mod encoding_tests {
	use super::*;

	#[test]
	fn nis_hold_reason_encoding_is_correct() {
		assert_eq!(RuntimeHoldReason::Nis(pallet_nis::HoldReason::NftReceipt).encode(), [38, 0]);
	}
}

#[test]
fn runtime_hold_reason_vs_max_holds() {
	use frame_support::assert_ok;
	use frame_support::traits::Get;
	use frame_support::traits::fungible::MutateHold;
	use frame_support::traits::VariantCount;

	sp_io::TestExternalities::default().execute_with(|| {
		let account = AccountId::from([0; 32]);
		let u = ExistentialDeposit::get();
		assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), account.clone().into(), u * 20));

		let assert_holds = |a, expected_count| {
			assert_eq!(expected_count, pallet_balances::Holds::<Runtime, ()>::get(a).len());
		};

		let balances_max_holds: u32 = <Runtime as pallet_balances::Config<()>>::MaxHolds::get();
		let nis_max_holds: u32 = <Runtime as pallet_balances::Config<NisCounterpartInstance>>::MaxHolds::get();
		let runtime_hold_reason_variant_count = <RuntimeHoldReason as VariantCount>::VARIANT_COUNT;

		println!("Balances max_holds: {:?}", balances_max_holds);
		println!("Nis max_holds: {:?}", nis_max_holds);
		println!("RuntimeHoldReason variant_count: {:?}", runtime_hold_reason_variant_count);

		// hold for all reasons
		assert_ok!(Balances::hold(&RuntimeHoldReason::Nis(pallet_nis::HoldReason::NftReceipt).into(), &account, u));
		assert_holds(&account, 1);
		assert_ok!(Balances::hold(&RuntimeHoldReason::Preimage(pallet_preimage::HoldReason::Preimage).into(), &account, u));
		assert_holds(&account, 2);
		assert_ok!(Balances::hold(&RuntimeHoldReason::StateTrieMigration(pallet_state_trie_migration::HoldReason::SlashForContinueMigrate).into(), &account, u));
		assert_holds(&account, 3);
		assert_ok!(Balances::hold(&RuntimeHoldReason::StateTrieMigration(pallet_state_trie_migration::HoldReason::SlashForMigrateCustomTop).into(), &account, u));
		assert_holds(&account, 4);
		assert_ok!(Balances::hold(&RuntimeHoldReason::StateTrieMigration(pallet_state_trie_migration::HoldReason::SlashForMigrateCustomChild).into(), &account, u));
		assert_holds(&account, 5);
	})
}