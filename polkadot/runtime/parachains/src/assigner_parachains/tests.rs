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

use super::*;
use crate::{
	assigner_parachains::mock_helpers::GenesisConfigBuilder,
	initializer::SessionChangeNotification,
	mock::{
		new_test_ext, ParachainsAssigner, Paras, ParasShared, RuntimeOrigin, Scheduler, System,
	},
	paras::{ParaGenesisArgs, ParaKind},
};
use frame_support::{assert_ok, pallet_prelude::*};
use primitives::{BlockNumber, Id as ParaId, SessionIndex, ValidationCode};
use sp_std::collections::btree_map::BTreeMap;

fn schedule_blank_para(id: ParaId, parakind: ParaKind) {
	let validation_code: ValidationCode = vec![1, 2, 3].into();
	assert_ok!(Paras::schedule_para_initialize(
		id,
		ParaGenesisArgs {
			genesis_head: Vec::new().into(),
			validation_code: validation_code.clone(),
			para_kind: parakind,
		}
	));

	assert_ok!(Paras::add_trusted_validation_code(RuntimeOrigin::root(), validation_code));
}

fn run_to_block(
	to: BlockNumber,
	new_session: impl Fn(BlockNumber) -> Option<SessionChangeNotification<BlockNumber>>,
) {
	while System::block_number() < to {
		let b = System::block_number();

		Scheduler::initializer_finalize();
		Paras::initializer_finalize(b);

		if let Some(notification) = new_session(b + 1) {
			let mut notification_with_session_index = notification;
			// We will make every session change trigger an action queue. Normally this may require
			// 2 or more session changes.
			if notification_with_session_index.session_index == SessionIndex::default() {
				notification_with_session_index.session_index = ParasShared::scheduled_session();
			}
			Paras::initializer_on_new_session(&notification_with_session_index);
			Scheduler::initializer_on_new_session(&notification_with_session_index);
		}

		System::on_finalize(b);

		System::on_initialize(b + 1);
		System::set_block_number(b + 1);

		Paras::initializer_initialize(b + 1);
		Scheduler::initializer_initialize(b + 1);

		// In the real runtime this is expected to be called by the `InclusionInherent` pallet.
		Scheduler::free_cores_and_fill_claimqueue(BTreeMap::new(), b + 1);
	}
}

// This and the scheduler test schedule_schedules_including_just_freed together
// ensure that next_up_on_available and next_up_on_time_out will always be
// filled with scheduler claims for lease holding parachains. (Removes the need
// for two other scheduler tests)
#[test]
fn parachains_assigner_pop_assignment_is_always_some() {
	let core_index = CoreIndex(0);
	let para_id = ParaId::from(10);
	let expected_assignment = Assignment::Bulk(para_id);

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// Register the para_id as a lease holding parachain
		schedule_blank_para(para_id, ParaKind::Parachain);

		assert!(!Paras::is_parachain(para_id));
		run_to_block(10, |n| if n == 10 { Some(Default::default()) } else { None });
		assert!(Paras::is_parachain(para_id));

		for _ in 0..20 {
			assert!(
				ParachainsAssigner::pop_assignment_for_core(core_index) ==
					Some(expected_assignment.clone())
			);
		}

		run_to_block(20, |n| if n == 20 { Some(Default::default()) } else { None });

		for _ in 0..20 {
			assert!(
				ParachainsAssigner::pop_assignment_for_core(core_index) ==
					Some(expected_assignment.clone())
			);
		}
	});
}
