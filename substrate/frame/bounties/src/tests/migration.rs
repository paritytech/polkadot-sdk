// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Bounties pallet tests.

use crate::{Bounty, BountyStatus};
use crate as pallet_bounties;
use super::mock::*;

use sp_runtime::Storage;
use frame_support::pallet_prelude::Encode;

#[test]
fn test_migration_v4() {
	let mut s = Storage::default();

	let index: u32 = 10;

	let bounty = Bounty::<u128, u64, u64, u64, (), u64, u64> {
		proposer: 0,
		asset_kind: (),
		value: 20,
		fee: 20,
		curator_deposit: 20,
		bond: 50,
		status: BountyStatus::<u128, u64, u64, u64>::Proposed,
	};

	let data = vec![
		(pallet_bounties::BountyCount::<Test>::hashed_key().to_vec(), (10 as u32).encode().to_vec()),
		(pallet_bounties::Bounties::<Test>::hashed_key_for(index), bounty.encode().to_vec()),
		(pallet_bounties::BountyDescriptions::<Test>::hashed_key_for(index), vec![0, 0]),
		(
			pallet_bounties::BountyApprovals::<Test>::hashed_key().to_vec(),
			vec![10 as u32].encode().to_vec(),
		),
	];

	s.top = data.into_iter().collect();

	sp_io::TestExternalities::new(s).execute_with(|| {
		use frame_support::traits::PalletInfo;
		let old_pallet_name = <Test as frame_system::Config>::PalletInfo::name::<Bounties>()
			.expect("Bounties is part of runtime, so it has a name; qed");
		let new_pallet_name = "NewBounties";

		crate::migrations::v4::pre_migration::<Test, Bounties, _>(old_pallet_name, new_pallet_name);
		crate::migrations::v4::migrate::<Test, Bounties, _>(old_pallet_name, new_pallet_name);
		crate::migrations::v4::post_migration::<Test, Bounties, _>(
			old_pallet_name,
			new_pallet_name,
		);
	});
}