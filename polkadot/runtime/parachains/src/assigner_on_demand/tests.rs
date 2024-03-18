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
	assigner_on_demand::{mock_helpers::GenesisConfigBuilder, Error},
	initializer::SessionChangeNotification,
	mock::{
		new_test_ext, Balances, OnDemandAssigner, Paras, ParasShared, RuntimeOrigin, Scheduler,
		System, Test,
	},
	paras::{ParaGenesisArgs, ParaKind},
};
use frame_support::{assert_noop, assert_ok, error::BadOrigin};
use pallet_balances::Error as BalancesError;
use primitives::{BlockNumber, SessionIndex, ValidationCode};
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

		// We need to update the spot traffic on every block.
		OnDemandAssigner::on_initialize(b + 1);

		// In the real runtime this is expected to be called by the `InclusionInherent` pallet.
		Scheduler::free_cores_and_fill_claimqueue(BTreeMap::new(), b + 1);
	}
}

fn place_order(para_id: ParaId) {
	let alice = 100u64;
	let amt = 10_000_000u128;

	Balances::make_free_balance_be(&alice, amt);

	run_to_block(101, |n| if n == 101 { Some(Default::default()) } else { None });
	OnDemandAssigner::place_order_allow_death(RuntimeOrigin::signed(alice), amt, para_id).unwrap()
}

#[test]
fn spot_traffic_capacity_zero_returns_none() {
	match OnDemandAssigner::calculate_spot_traffic(
		FixedU128::from(u128::MAX),
		0u32,
		u32::MAX,
		Perbill::from_percent(100),
		Perbill::from_percent(1),
	) {
		Ok(_) => panic!("Error"),
		Err(e) => assert_eq!(e, SpotTrafficCalculationErr::QueueCapacityIsZero),
	};
}

#[test]
fn spot_traffic_queue_size_larger_than_capacity_returns_none() {
	match OnDemandAssigner::calculate_spot_traffic(
		FixedU128::from(u128::MAX),
		1u32,
		2u32,
		Perbill::from_percent(100),
		Perbill::from_percent(1),
	) {
		Ok(_) => panic!("Error"),
		Err(e) => assert_eq!(e, SpotTrafficCalculationErr::QueueSizeLargerThanCapacity),
	}
}

#[test]
fn spot_traffic_calculation_identity() {
	match OnDemandAssigner::calculate_spot_traffic(
		FixedU128::from_u32(1),
		1000,
		100,
		Perbill::from_percent(10),
		Perbill::from_percent(3),
	) {
		Ok(res) => {
			assert_eq!(res, FixedU128::from_u32(1))
		},
		_ => (),
	}
}

#[test]
fn spot_traffic_calculation_u32_max() {
	match OnDemandAssigner::calculate_spot_traffic(
		FixedU128::from_u32(1),
		u32::MAX,
		u32::MAX,
		Perbill::from_percent(100),
		Perbill::from_percent(3),
	) {
		Ok(res) => {
			assert_eq!(res, FixedU128::from_u32(1))
		},
		_ => panic!("Error"),
	};
}

#[test]
fn spot_traffic_calculation_u32_traffic_max() {
	match OnDemandAssigner::calculate_spot_traffic(
		FixedU128::from(u128::MAX),
		u32::MAX,
		u32::MAX,
		Perbill::from_percent(1),
		Perbill::from_percent(1),
	) {
		Ok(res) => assert_eq!(res, FixedU128::from(u128::MAX)),
		_ => panic!("Error"),
	};
}

#[test]
fn sustained_target_increases_spot_traffic() {
	let mut traffic = FixedU128::from_u32(1u32);
	for _ in 0..50 {
		traffic = OnDemandAssigner::calculate_spot_traffic(
			traffic,
			100,
			12,
			Perbill::from_percent(10),
			Perbill::from_percent(100),
		)
		.unwrap()
	}
	assert_eq!(traffic, FixedU128::from_inner(2_718_103_312_071_174_015u128))
}

#[test]
fn spot_traffic_can_decrease() {
	let traffic = FixedU128::from_u32(100u32);
	match OnDemandAssigner::calculate_spot_traffic(
		traffic,
		100u32,
		0u32,
		Perbill::from_percent(100),
		Perbill::from_percent(100),
	) {
		Ok(new_traffic) =>
			assert_eq!(new_traffic, FixedU128::from_inner(50_000_000_000_000_000_000u128)),
		_ => panic!("Error"),
	}
}

#[test]
fn spot_traffic_decreases_over_time() {
	let mut traffic = FixedU128::from_u32(100u32);
	for _ in 0..5 {
		traffic = OnDemandAssigner::calculate_spot_traffic(
			traffic,
			100u32,
			0u32,
			Perbill::from_percent(100),
			Perbill::from_percent(100),
		)
		.unwrap();
		println!("{traffic}");
	}
	assert_eq!(traffic, FixedU128::from_inner(3_125_000_000_000_000_000u128))
}

#[test]
fn spot_traffic_decreases_between_idle_blocks() {
	// Testing spot traffic assumptions, but using the mock runtime and default on demand
	// configuration values. Ensuring that blocks with no on demand activity (idle)
	// decrease traffic.

	let para_id = ParaId::from(111);

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// Initialize the parathread and wait for it to be ready.
		schedule_blank_para(para_id, ParaKind::Parathread);
		assert!(!Paras::is_parathread(para_id));
		run_to_block(100, |n| if n == 100 { Some(Default::default()) } else { None });
		assert!(Paras::is_parathread(para_id));

		// Set the spot traffic to a large number
		OnDemandAssigner::set_queue_status(QueueStatusType {
			traffic: FixedU128::from_u32(10),
			..Default::default()
		});

		assert_eq!(OnDemandAssigner::get_queue_status().traffic, FixedU128::from_u32(10));

		// Run to block 101 and ensure that the traffic decreases.
		run_to_block(101, |n| if n == 100 { Some(Default::default()) } else { None });
		assert!(OnDemandAssigner::get_queue_status().traffic < FixedU128::from_u32(10));

		// Run to block 102 and observe that we've hit the default traffic value.
		run_to_block(102, |n| if n == 100 { Some(Default::default()) } else { None });
		assert_eq!(
			OnDemandAssigner::get_queue_status().traffic,
			OnDemandAssigner::get_traffic_default_value()
		);
	})
}

#[test]
fn place_order_works() {
	let alice = 1u64;
	let amt = 10_000_000u128;
	let para_id = ParaId::from(111);

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// Initialize the parathread and wait for it to be ready.
		schedule_blank_para(para_id, ParaKind::Parathread);

		assert!(!Paras::is_parathread(para_id));

		run_to_block(100, |n| if n == 100 { Some(Default::default()) } else { None });

		assert!(Paras::is_parathread(para_id));

		// Does not work unsigned
		assert_noop!(
			OnDemandAssigner::place_order_allow_death(RuntimeOrigin::none(), amt, para_id),
			BadOrigin
		);

		// Does not work with max_amount lower than fee
		let low_max_amt = 1u128;
		assert_noop!(
			OnDemandAssigner::place_order_allow_death(
				RuntimeOrigin::signed(alice),
				low_max_amt,
				para_id,
			),
			Error::<Test>::SpotPriceHigherThanMaxAmount,
		);

		// Does not work with insufficient balance
		assert_noop!(
			OnDemandAssigner::place_order_allow_death(RuntimeOrigin::signed(alice), amt, para_id),
			BalancesError::<Test, _>::InsufficientBalance
		);

		// Works
		Balances::make_free_balance_be(&alice, amt);
		run_to_block(101, |n| if n == 101 { Some(Default::default()) } else { None });
		assert_ok!(OnDemandAssigner::place_order_allow_death(
			RuntimeOrigin::signed(alice),
			amt,
			para_id
		));
	});
}

#[test]
fn place_order_keep_alive_keeps_alive() {
	let alice = 1u64;
	let amt = 1u128; // The same as crate::mock's EXISTENTIAL_DEPOSIT
	let max_amt = 10_000_000u128;
	let para_id = ParaId::from(111);

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		// Initialize the parathread and wait for it to be ready.
		schedule_blank_para(para_id, ParaKind::Parathread);
		Balances::make_free_balance_be(&alice, amt);

		assert!(!Paras::is_parathread(para_id));
		run_to_block(100, |n| if n == 100 { Some(Default::default()) } else { None });
		assert!(Paras::is_parathread(para_id));

		assert_noop!(
			OnDemandAssigner::place_order_keep_alive(
				RuntimeOrigin::signed(alice),
				max_amt,
				para_id
			),
			BalancesError::<Test, _>::InsufficientBalance
		);
	});
}

#[test]
fn pop_assignment_for_core_works() {
	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		let para_a = ParaId::from(111);
		let para_b = ParaId::from(110);
		schedule_blank_para(para_a, ParaKind::Parathread);
		schedule_blank_para(para_b, ParaKind::Parathread);

		run_to_block(11, |n| if n == 11 { Some(Default::default()) } else { None });

		// Pop should return none with empty queue
		assert_eq!(OnDemandAssigner::pop_assignment_for_core(CoreIndex(0)), None);

		// Add enough assignments to the order queue.
		for _ in 0..2 {
			place_order(para_a);
			place_order(para_b);
		}

		// Popped assignments should be for the correct paras and cores
		assert_eq!(
			OnDemandAssigner::pop_assignment_for_core(CoreIndex(0)).map(|a| a.para_id()),
			Some(para_a)
		);
		assert_eq!(
			OnDemandAssigner::pop_assignment_for_core(CoreIndex(1)).map(|a| a.para_id()),
			Some(para_b)
		);
		assert_eq!(
			OnDemandAssigner::pop_assignment_for_core(CoreIndex(0)).map(|a| a.para_id()),
			Some(para_a)
		);
		assert_eq!(
			OnDemandAssigner::pop_assignment_for_core(CoreIndex(1)).map(|a| a.para_id()),
			Some(para_b)
		);
	});
}

#[test]
fn push_back_assignment_works() {
	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		let para_a = ParaId::from(111);
		let para_b = ParaId::from(110);
		schedule_blank_para(para_a, ParaKind::Parathread);
		schedule_blank_para(para_b, ParaKind::Parathread);

		run_to_block(11, |n| if n == 11 { Some(Default::default()) } else { None });

		// Add enough assignments to the order queue.
		place_order(para_a);
		place_order(para_b);

		// Pop order a
		assert_eq!(
			OnDemandAssigner::pop_assignment_for_core(CoreIndex(0)).unwrap().para_id(),
			para_a
		);

		// Para a should have affinity for core 0
		assert_eq!(OnDemandAssigner::get_affinity_map(para_a).unwrap().count, 1);
		assert_eq!(OnDemandAssigner::get_affinity_map(para_a).unwrap().core_index, CoreIndex(0));

		// Push back order a
		OnDemandAssigner::push_back_assignment(para_a, CoreIndex(0));

		// Para a should have no affinity
		assert_eq!(OnDemandAssigner::get_affinity_map(para_a).is_none(), true);

		// Queue should contain orders a, b. A in front of b.
		assert_eq!(
			OnDemandAssigner::pop_assignment_for_core(CoreIndex(0)).unwrap().para_id(),
			para_a
		);
		assert_eq!(
			OnDemandAssigner::pop_assignment_for_core(CoreIndex(0)).unwrap().para_id(),
			para_b
		);
	});
}

#[test]
fn affinity_prohibits_parallel_scheduling() {
	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		let para_a = ParaId::from(111);
		let para_b = ParaId::from(222);

		schedule_blank_para(para_a, ParaKind::Parathread);
		schedule_blank_para(para_b, ParaKind::Parathread);

		run_to_block(11, |n| if n == 11 { Some(Default::default()) } else { None });

		// There should be no affinity before starting.
		assert!(OnDemandAssigner::get_affinity_map(para_a).is_none());
		assert!(OnDemandAssigner::get_affinity_map(para_b).is_none());

		// Add 2 assignments for para_a for every para_b.
		place_order(para_a);
		place_order(para_a);
		place_order(para_b);

		// Approximate having 1 core.
		for _ in 0..3 {
			assert!(OnDemandAssigner::pop_assignment_for_core(CoreIndex(0)).is_some());
		}
		assert!(OnDemandAssigner::pop_assignment_for_core(CoreIndex(0)).is_none());

		// Affinity on one core is meaningless.
		assert_eq!(OnDemandAssigner::get_affinity_map(para_a).unwrap().count, 2);
		assert_eq!(OnDemandAssigner::get_affinity_map(para_b).unwrap().count, 1);
		assert_eq!(
			OnDemandAssigner::get_affinity_map(para_a).unwrap().core_index,
			OnDemandAssigner::get_affinity_map(para_b).unwrap().core_index,
		);

		// Clear affinity
		OnDemandAssigner::report_processed(para_a, 0.into());
		OnDemandAssigner::report_processed(para_a, 0.into());
		OnDemandAssigner::report_processed(para_b, 0.into());

		// Add 2 assignments for para_a for every para_b.
		place_order(para_a);
		place_order(para_a);
		place_order(para_b);

		// Approximate having 3 cores. CoreIndex 2 should be unable to obtain an assignment
		for _ in 0..3 {
			OnDemandAssigner::pop_assignment_for_core(CoreIndex(0));
			OnDemandAssigner::pop_assignment_for_core(CoreIndex(1));
			assert!(OnDemandAssigner::pop_assignment_for_core(CoreIndex(2)).is_none());
		}

		// Affinity should be the same as before, but on different cores.
		assert_eq!(OnDemandAssigner::get_affinity_map(para_a).unwrap().count, 2);
		assert_eq!(OnDemandAssigner::get_affinity_map(para_b).unwrap().count, 1);
		assert_eq!(OnDemandAssigner::get_affinity_map(para_a).unwrap().core_index, CoreIndex(0));
		assert_eq!(OnDemandAssigner::get_affinity_map(para_b).unwrap().core_index, CoreIndex(1));

		// Clear affinity
		OnDemandAssigner::report_processed(para_a, CoreIndex(0));
		OnDemandAssigner::report_processed(para_a, CoreIndex(0));
		OnDemandAssigner::report_processed(para_b, CoreIndex(1));

		// There should be no affinity after clearing.
		assert!(OnDemandAssigner::get_affinity_map(para_a).is_none());
		assert!(OnDemandAssigner::get_affinity_map(para_b).is_none());
	});
}

#[test]
fn affinity_changes_work() {
	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		let para_a = ParaId::from(111);
		let core_index = CoreIndex(0);
		schedule_blank_para(para_a, ParaKind::Parathread);

		run_to_block(11, |n| if n == 11 { Some(Default::default()) } else { None });

		// There should be no affinity before starting.
		assert!(OnDemandAssigner::get_affinity_map(para_a).is_none());

		// Add enough assignments to the order queue.
		for _ in 0..10 {
			place_order(para_a);
		}

		// There should be no affinity before the scheduler pops.
		assert!(OnDemandAssigner::get_affinity_map(para_a).is_none());

		OnDemandAssigner::pop_assignment_for_core(core_index);

		// Affinity count is 1 after popping.
		assert_eq!(OnDemandAssigner::get_affinity_map(para_a).unwrap().count, 1);

		OnDemandAssigner::report_processed(para_a, 0.into());
		OnDemandAssigner::pop_assignment_for_core(core_index);

		// Affinity count is 1 after popping with a previous para.
		assert_eq!(OnDemandAssigner::get_affinity_map(para_a).unwrap().count, 1);

		for _ in 0..3 {
			OnDemandAssigner::pop_assignment_for_core(core_index);
		}

		// Affinity count is 4 after popping 3 times without a previous para.
		assert_eq!(OnDemandAssigner::get_affinity_map(para_a).unwrap().count, 4);

		for _ in 0..5 {
			OnDemandAssigner::report_processed(para_a, 0.into());
			assert!(OnDemandAssigner::pop_assignment_for_core(core_index).is_some());
		}

		// Affinity count should still be 4 but queue should be empty.
		assert!(OnDemandAssigner::pop_assignment_for_core(core_index).is_none());
		assert_eq!(OnDemandAssigner::get_affinity_map(para_a).unwrap().count, 4);

		// Pop 4 times and get to exactly 0 (None) affinity.
		for _ in 0..4 {
			OnDemandAssigner::report_processed(para_a, 0.into());
			assert!(OnDemandAssigner::pop_assignment_for_core(core_index).is_none());
		}
		assert!(OnDemandAssigner::get_affinity_map(para_a).is_none());

		// Decreasing affinity beyond 0 should still be None.
		OnDemandAssigner::report_processed(para_a, 0.into());
		assert!(OnDemandAssigner::pop_assignment_for_core(core_index).is_none());
		assert!(OnDemandAssigner::get_affinity_map(para_a).is_none());
	});
}

#[test]
fn new_affinity_for_a_core_must_come_from_free_entries() {
	// If affinity count for a core was zero before, and is 1 now, then the entry
	// must have come from free_entries.
	let parachains =
		vec![ParaId::from(111), ParaId::from(222), ParaId::from(333), ParaId::from(444)];
	let core_indices = vec![CoreIndex(0), CoreIndex(1), CoreIndex(2), CoreIndex(3)];

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		parachains.iter().for_each(|chain| {
			schedule_blank_para(*chain, ParaKind::Parathread);
		});

		run_to_block(11, |n| if n == 11 { Some(Default::default()) } else { None });

		// Place orders for all chains.
		parachains.iter().for_each(|chain| {
			place_order(*chain);
		});

		// There are 4 entries in free_entries.
		let start_free_entries = OnDemandAssigner::get_free_entries().len();
		assert_eq!(start_free_entries, 4);

		// Pop assignments on all cores.
		core_indices.iter().enumerate().for_each(|(n, core_index)| {
			// There is no affinity on the core prior to popping.
			assert!(OnDemandAssigner::get_affinity_entries(*core_index).is_empty());

			// There's always an order to be popped for each core.
			let free_entries = OnDemandAssigner::get_free_entries();
			let next_order = free_entries.peek();

			// There is no affinity on the paraid prior to popping.
			assert!(OnDemandAssigner::get_affinity_map(next_order.unwrap().para_id).is_none());

			match OnDemandAssigner::pop_assignment_for_core(*core_index) {
				Some(assignment) => {
					// The popped assignment came from free entries.
					assert_eq!(
						start_free_entries - 1 - n,
						OnDemandAssigner::get_free_entries().len()
					);
					// The popped assignment has the same para id as the next order.
					assert_eq!(assignment.para_id(), next_order.unwrap().para_id);
				},
				None => panic!("Should not happen"),
			}
		});

		// All entries have been removed from free_entries.
		assert!(OnDemandAssigner::get_free_entries().is_empty());

		// All chains have an affinity count of 1.
		parachains.iter().for_each(|chain| {
			assert_eq!(OnDemandAssigner::get_affinity_map(*chain).unwrap().count, 1);
		});
	});
}

#[test]
#[should_panic]
fn queue_index_ordering_is_unsound_over_max_size() {
	// NOTE: Unsoundness proof. If the number goes sufficiently over the max_queue_max_size
	// the overflow will cause an opposite comparison to what would be expected.
	let max_num = u32::MAX - ON_DEMAND_MAX_QUEUE_MAX_SIZE;
	// 0 < some large number.
	assert_eq!(QueueIndex(0).cmp(&QueueIndex(max_num + 1)), Ordering::Less);
}

#[test]
fn queue_index_ordering_works() {
	// The largest accepted queue size.
	let max_num = ON_DEMAND_MAX_QUEUE_MAX_SIZE;

	// 0 == 0
	assert_eq!(QueueIndex(0).cmp(&QueueIndex(0)), Ordering::Equal);
	// 0 < 1
	assert_eq!(QueueIndex(0).cmp(&QueueIndex(1)), Ordering::Less);
	// 1 > 0
	assert_eq!(QueueIndex(1).cmp(&QueueIndex(0)), Ordering::Greater);
	// 0 < max_num
	assert_eq!(QueueIndex(0).cmp(&QueueIndex(max_num)), Ordering::Less);
	// 0 > max_num + 1
	assert_eq!(QueueIndex(0).cmp(&QueueIndex(max_num + 1)), Ordering::Less);

	// Ordering within the bounds of ON_DEMAND_MAX_QUEUE_MAX_SIZE works.
	let mut v = vec![3, 6, 2, 1, 5, 4];
	v.sort_by_key(|&num| QueueIndex(num));
	assert_eq!(v, vec![1, 2, 3, 4, 5, 6]);

	v = vec![max_num, 4, 5, 1, 6];
	v.sort_by_key(|&num| QueueIndex(num));
	assert_eq!(v, vec![1, 4, 5, 6, max_num]);

	// Ordering with an element outside of the bounds of the max size also works.
	v = vec![max_num + 2, 0, 6, 2, 1, 5, 4];
	v.sort_by_key(|&num| QueueIndex(num));
	assert_eq!(v, vec![0, 1, 2, 4, 5, 6, max_num + 2]);

	// Numbers way above the max size will overflow
	v = vec![u32::MAX - 1, u32::MAX, 6, 2, 1, 5, 4];
	v.sort_by_key(|&num| QueueIndex(num));
	assert_eq!(v, vec![u32::MAX - 1, u32::MAX, 1, 2, 4, 5, 6]);
}

#[test]
fn reverse_queue_index_does_reverse() {
	let mut v = vec![1, 2, 3, 4, 5, 6];

	// Basic reversal of a vector.
	v.sort_by_key(|&num| ReverseQueueIndex(num));
	assert_eq!(v, vec![6, 5, 4, 3, 2, 1]);

	// Example from rust docs on `Reverse`. Should work identically.
	v.sort_by_key(|&num| (num > 3, ReverseQueueIndex(num)));
	assert_eq!(v, vec![3, 2, 1, 6, 5, 4]);

	let mut v2 = vec![1, 2, u32::MAX];
	v2.sort_by_key(|&num| ReverseQueueIndex(num));
	assert_eq!(v2, vec![2, 1, u32::MAX]);
}

#[test]
fn queue_status_size_fn_works() {
	// Add orders to the on demand queue, and make sure that they are properly represented
	// by the QueueStatusType::size fn.
	let parachains = vec![ParaId::from(111), ParaId::from(222), ParaId::from(333)];
	let core_indices = vec![CoreIndex(0), CoreIndex(1)];

	new_test_ext(GenesisConfigBuilder::default().build()).execute_with(|| {
		parachains.iter().for_each(|chain| {
			schedule_blank_para(*chain, ParaKind::Parathread);
		});

		assert_eq!(OnDemandAssigner::get_queue_status().size(), 0);

		run_to_block(11, |n| if n == 11 { Some(Default::default()) } else { None });

		// Place orders for all chains.
		parachains.iter().for_each(|chain| {
			// 2 per chain for a total of 6
			place_order(*chain);
			place_order(*chain);
		});

		// 6 orders in free entries
		assert_eq!(OnDemandAssigner::get_free_entries().len(), 6);
		// 6 orders via queue status size
		assert_eq!(
			OnDemandAssigner::get_free_entries().len(),
			OnDemandAssigner::get_queue_status().size() as usize
		);

		core_indices.iter().for_each(|core_index| {
			OnDemandAssigner::pop_assignment_for_core(*core_index);
		});

		// There should be 2 orders in the scheduler's claimqueue,
		// 2 in assorted AffinityMaps and 2 in free.
		// ParaId 111
		assert_eq!(OnDemandAssigner::get_affinity_entries(core_indices[0]).len(), 1);
		// ParaId 222
		assert_eq!(OnDemandAssigner::get_affinity_entries(core_indices[1]).len(), 1);
		// Free entries are from ParaId 333
		assert_eq!(OnDemandAssigner::get_free_entries().len(), 2);
		// For a total size of 4.
		assert_eq!(OnDemandAssigner::get_queue_status().size(), 4)
	});
}
