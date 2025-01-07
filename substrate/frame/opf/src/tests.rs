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

//! Tests for OPF pallet.

pub use super::*;
use crate::mock::*;
use frame_support::{assert_noop, assert_ok, traits::OnIdle};

pub fn next_block() {
	System::set_block_number(<Test as Config>::BlockNumberProvider::current_block_number() + 1);
	AllPalletsWithSystem::on_initialize(
		<Test as Config>::BlockNumberProvider::current_block_number(),
	);
	AllPalletsWithSystem::on_idle(
		<Test as Config>::BlockNumberProvider::current_block_number(),
		Weight::MAX,
	);
}

pub fn project_list() -> Vec<ProjectId<Test>>{
	vec![ALICE, BOB, DAVE]
	
}

pub fn run_to_block(n: BlockNumberFor<Test>) {
	while <Test as Config>::BlockNumberProvider::current_block_number() < n {
		if <Test as Config>::BlockNumberProvider::current_block_number() > 1 {
			AllPalletsWithSystem::on_finalize(
				<Test as Config>::BlockNumberProvider::current_block_number(),
			);
		}
		next_block();
	}
}

#[test]
fn project_registration_works() {
	new_test_ext().execute_with(|| {
		let batch = project_list();
		assert_ok!(Opf::register_projects_batch(RuntimeOrigin::signed(EVE), batch));
		let project_list = WhiteListedProjectAccounts::<Test>::get(BOB);
		assert!(project_list.is_some());
		// we should have 3 referendum started
		assert_eq!(pallet_democracy::PublicProps::<Test>::get().len(), 3);
		assert_eq!(pallet_democracy::ReferendumCount::<Test>::get(), 3);
	})
}
