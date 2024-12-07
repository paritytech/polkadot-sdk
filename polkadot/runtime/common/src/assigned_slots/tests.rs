#[cfg(test)]
use super::*;

use crate::{assigned_slots, assigned_slots::mock::*, mock::TestRegistrar, slots};
use frame_support::{assert_noop, assert_ok};
use polkadot_primitives_test_helpers::{dummy_head_data, dummy_validation_code};
use sp_runtime::DispatchError::BadOrigin;

#[test]
fn basic_setup_works() {
	new_test_ext().execute_with(|| {
		run_to_block(1);
		assert_eq!(AssignedSlots::current_lease_period_index(), 0);
		assert_eq!(mock::Slots::deposit_held(1.into(), &1), 0);

		run_to_block(3);
		assert_eq!(AssignedSlots::current_lease_period_index(), 1);
	});
}

#[test]
fn assign_perm_slot_fails_for_unknown_para() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_noop!(
			AssignedSlots::assign_perm_parachain_slot(RuntimeOrigin::root(), ParaId::from(1_u32),),
			Error::<Test>::ParaDoesntExist
		);
	});
}

#[test]
fn assign_perm_slot_fails_for_invalid_origin() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_noop!(
			AssignedSlots::assign_perm_parachain_slot(
				RuntimeOrigin::signed(1),
				ParaId::from(1_u32),
			),
			BadOrigin
		);
	});
}

#[test]
fn assign_perm_slot_fails_when_not_parathread() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));
		assert_ok!(TestRegistrar::<Test>::make_parachain(ParaId::from(1_u32)));

		assert_noop!(
			AssignedSlots::assign_perm_parachain_slot(RuntimeOrigin::root(), ParaId::from(1_u32),),
			Error::<Test>::NotParathread
		);
	});
}

#[test]
fn assign_perm_slot_fails_when_existing_lease() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));

		// Register lease in current lease period
		assert_ok!(mock::Slots::lease_out(ParaId::from(1_u32), &1, 1, 1, 1));
		// Try to assign a perm slot in current period fails
		assert_noop!(
			AssignedSlots::assign_perm_parachain_slot(RuntimeOrigin::root(), ParaId::from(1_u32),),
			Error::<Test>::OngoingLeaseExists
		);

		// Cleanup
		assert_ok!(mock::Slots::clear_all_leases(RuntimeOrigin::root(), 1.into()));

		// Register lease for next lease period
		assert_ok!(mock::Slots::lease_out(ParaId::from(1_u32), &1, 1, 2, 1));
		// Should be detected and also fail
		assert_noop!(
			AssignedSlots::assign_perm_parachain_slot(RuntimeOrigin::root(), ParaId::from(1_u32),),
			Error::<Test>::OngoingLeaseExists
		);
	});
}

#[test]
fn assign_perm_slot_fails_when_max_perm_slots_exceeded() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));

		assert_ok!(TestRegistrar::<Test>::register(
			2,
			ParaId::from(2_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));

		assert_ok!(TestRegistrar::<Test>::register(
			3,
			ParaId::from(3_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));

		assert_ok!(AssignedSlots::assign_perm_parachain_slot(
			RuntimeOrigin::root(),
			ParaId::from(1_u32),
		));
		assert_ok!(AssignedSlots::assign_perm_parachain_slot(
			RuntimeOrigin::root(),
			ParaId::from(2_u32),
		));
		assert_eq!(assigned_slots::PermanentSlotCount::<Test>::get(), 2);

		assert_noop!(
			AssignedSlots::assign_perm_parachain_slot(RuntimeOrigin::root(), ParaId::from(3_u32),),
			Error::<Test>::MaxPermanentSlotsExceeded
		);
	});
}

#[test]
fn assign_perm_slot_succeeds_for_parathread() {
	new_test_ext().execute_with(|| {
		let mut block = 1;
		run_to_block(block);
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));

		assert_eq!(assigned_slots::PermanentSlotCount::<Test>::get(), 0);
		assert_eq!(assigned_slots::PermanentSlots::<Test>::get(ParaId::from(1_u32)), None);

		assert_ok!(AssignedSlots::assign_perm_parachain_slot(
			RuntimeOrigin::root(),
			ParaId::from(1_u32),
		));

		// Para is a lease holding parachain for PermanentSlotLeasePeriodLength * LeasePeriod
		// blocks
		while block < 9 {
			println!("block #{}", block);

			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), true);

			assert_eq!(assigned_slots::PermanentSlotCount::<Test>::get(), 1);
			assert_eq!(AssignedSlots::has_permanent_slot(ParaId::from(1_u32)), true);
			assert_eq!(
				assigned_slots::PermanentSlots::<Test>::get(ParaId::from(1_u32)),
				Some((0, 3))
			);

			assert_eq!(mock::Slots::already_leased(ParaId::from(1_u32), 0, 2), true);

			block += 1;
			run_to_block(block);
		}

		// Para lease ended, downgraded back to parathread (on-demand parachain)
		assert_eq!(TestRegistrar::<Test>::is_parathread(ParaId::from(1_u32)), true);
		assert_eq!(mock::Slots::already_leased(ParaId::from(1_u32), 0, 5), false);
	});
}

#[test]
fn assign_temp_slot_fails_for_unknown_para() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_noop!(
			AssignedSlots::assign_temp_parachain_slot(
				RuntimeOrigin::root(),
				ParaId::from(1_u32),
				SlotLeasePeriodStart::Current
			),
			Error::<Test>::ParaDoesntExist
		);
	});
}

#[test]
fn assign_temp_slot_fails_for_invalid_origin() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_noop!(
			AssignedSlots::assign_temp_parachain_slot(
				RuntimeOrigin::signed(1),
				ParaId::from(1_u32),
				SlotLeasePeriodStart::Current
			),
			BadOrigin
		);
	});
}

#[test]
fn assign_temp_slot_fails_when_not_parathread() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));
		assert_ok!(TestRegistrar::<Test>::make_parachain(ParaId::from(1_u32)));

		assert_noop!(
			AssignedSlots::assign_temp_parachain_slot(
				RuntimeOrigin::root(),
				ParaId::from(1_u32),
				SlotLeasePeriodStart::Current
			),
			Error::<Test>::NotParathread
		);
	});
}

#[test]
fn assign_temp_slot_fails_when_existing_lease() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));

		// Register lease in current lease period
		assert_ok!(mock::Slots::lease_out(ParaId::from(1_u32), &1, 1, 1, 1));
		// Try to assign a perm slot in current period fails
		assert_noop!(
			AssignedSlots::assign_temp_parachain_slot(
				RuntimeOrigin::root(),
				ParaId::from(1_u32),
				SlotLeasePeriodStart::Current
			),
			Error::<Test>::OngoingLeaseExists
		);

		// Cleanup
		assert_ok!(mock::Slots::clear_all_leases(RuntimeOrigin::root(), 1.into()));

		// Register lease for next lease period
		assert_ok!(mock::Slots::lease_out(ParaId::from(1_u32), &1, 1, 2, 1));
		// Should be detected and also fail
		assert_noop!(
			AssignedSlots::assign_temp_parachain_slot(
				RuntimeOrigin::root(),
				ParaId::from(1_u32),
				SlotLeasePeriodStart::Current
			),
			Error::<Test>::OngoingLeaseExists
		);
	});
}

#[test]
fn assign_temp_slot_fails_when_max_temp_slots_exceeded() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		// Register 6 paras & a temp slot for each
		for n in 0..=5 {
			assert_ok!(TestRegistrar::<Test>::register(
				n,
				ParaId::from(n as u32),
				dummy_head_data(),
				dummy_validation_code()
			));

			assert_ok!(AssignedSlots::assign_temp_parachain_slot(
				RuntimeOrigin::root(),
				ParaId::from(n as u32),
				SlotLeasePeriodStart::Current
			));
		}

		assert_eq!(assigned_slots::TemporarySlotCount::<Test>::get(), 6);

		// Attempt to assign one more temp slot
		assert_ok!(TestRegistrar::<Test>::register(
			7,
			ParaId::from(7_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));
		assert_noop!(
			AssignedSlots::assign_temp_parachain_slot(
				RuntimeOrigin::root(),
				ParaId::from(7_u32),
				SlotLeasePeriodStart::Current
			),
			Error::<Test>::MaxTemporarySlotsExceeded
		);
	});
}

#[test]
fn assign_temp_slot_succeeds_for_single_parathread() {
	new_test_ext().execute_with(|| {
		let mut block = 1;
		run_to_block(block);
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));

		assert_eq!(assigned_slots::TemporarySlots::<Test>::get(ParaId::from(1_u32)), None);

		assert_ok!(AssignedSlots::assign_temp_parachain_slot(
			RuntimeOrigin::root(),
			ParaId::from(1_u32),
			SlotLeasePeriodStart::Current
		));
		assert_eq!(assigned_slots::TemporarySlotCount::<Test>::get(), 1);
		assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 1);

		// Block 1-5
		// Para is a lease holding parachain for TemporarySlotLeasePeriodLength * LeasePeriod
		// blocks
		while block < 6 {
			println!("block #{}", block);
			println!("lease period #{}", AssignedSlots::current_lease_period_index());
			println!("lease {:?}", slots::Leases::<Test>::get(ParaId::from(1_u32)));

			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), true);

			assert_eq!(AssignedSlots::has_temporary_slot(ParaId::from(1_u32)), true);
			assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 1);
			assert_eq!(
				assigned_slots::TemporarySlots::<Test>::get(ParaId::from(1_u32)),
				Some(ParachainTemporarySlot {
					manager: 1,
					period_begin: 0,
					period_count: 2, // TemporarySlotLeasePeriodLength
					last_lease: Some(0),
					lease_count: 1
				})
			);

			assert_eq!(mock::Slots::already_leased(ParaId::from(1_u32), 0, 1), true);

			block += 1;
			run_to_block(block);
		}

		// Block 6
		println!("block #{}", block);
		println!("lease period #{}", AssignedSlots::current_lease_period_index());
		println!("lease {:?}", slots::Leases::<Test>::get(ParaId::from(1_u32)));

		// Para lease ended, downgraded back to on-demand parachain
		assert_eq!(TestRegistrar::<Test>::is_parathread(ParaId::from(1_u32)), true);
		assert_eq!(mock::Slots::already_leased(ParaId::from(1_u32), 0, 3), false);
		assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 0);

		// Block 12
		// Para should get a turn after TemporarySlotLeasePeriodLength * LeasePeriod blocks
		run_to_block(12);
		println!("block #{}", block);
		println!("lease period #{}", AssignedSlots::current_lease_period_index());
		println!("lease {:?}", slots::Leases::<Test>::get(ParaId::from(1_u32)));

		assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), true);
		assert_eq!(mock::Slots::already_leased(ParaId::from(1_u32), 4, 5), true);
		assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 1);
	});
}

#[test]
fn assign_temp_slot_succeeds_for_multiple_parathreads() {
	new_test_ext().execute_with(|| {
		// Block 1, Period 0
		run_to_block(1);

		// Register 6 paras & a temp slot for each
		// (3 slots in current lease period, 3 in the next one)
		for n in 0..=5 {
			assert_ok!(TestRegistrar::<Test>::register(
				n,
				ParaId::from(n as u32),
				dummy_head_data(),
				dummy_validation_code()
			));

			assert_ok!(AssignedSlots::assign_temp_parachain_slot(
				RuntimeOrigin::root(),
				ParaId::from(n as u32),
				if (n % 2).is_zero() {
					SlotLeasePeriodStart::Current
				} else {
					SlotLeasePeriodStart::Next
				}
			));
		}

		// Block 1-5, Period 0-1
		for n in 1..=5 {
			if n > 1 {
				run_to_block(n);
			}
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(0)), true);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(2_u32)), true);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(3_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(4_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(5_u32)), false);
			assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 2);
		}

		// Block 6-11, Period 2-3
		for n in 6..=11 {
			run_to_block(n);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(0)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), true);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(2_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(3_u32)), true);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(4_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(5_u32)), false);
			assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 2);
		}

		// Block 12-17, Period 4-5
		for n in 12..=17 {
			run_to_block(n);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(0)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(2_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(3_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(4_u32)), true);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(5_u32)), true);
			assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 2);
		}

		// Block 18-23, Period 6-7
		for n in 18..=23 {
			run_to_block(n);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(0)), true);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(2_u32)), true);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(3_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(4_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(5_u32)), false);
			assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 2);
		}

		// Block 24-29, Period 8-9
		for n in 24..=29 {
			run_to_block(n);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(0)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), true);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(2_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(3_u32)), true);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(4_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(5_u32)), false);
			assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 2);
		}

		// Block 30-35, Period 10-11
		for n in 30..=35 {
			run_to_block(n);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(0)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(2_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(3_u32)), false);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(4_u32)), true);
			assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(5_u32)), true);
			assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 2);
		}
	});
}

#[test]
fn unassign_slot_fails_for_unknown_para() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_noop!(
			AssignedSlots::unassign_parachain_slot(RuntimeOrigin::root(), ParaId::from(1_u32),),
			Error::<Test>::SlotNotAssigned
		);
	});
}

#[test]
fn unassign_slot_fails_for_invalid_origin() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_noop!(
			AssignedSlots::assign_perm_parachain_slot(
				RuntimeOrigin::signed(1),
				ParaId::from(1_u32),
			),
			BadOrigin
		);
	});
}

#[test]
fn unassign_perm_slot_succeeds() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));

		assert_ok!(AssignedSlots::assign_perm_parachain_slot(
			RuntimeOrigin::root(),
			ParaId::from(1_u32),
		));

		assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), true);

		assert_ok!(AssignedSlots::unassign_parachain_slot(
			RuntimeOrigin::root(),
			ParaId::from(1_u32),
		));

		assert_eq!(assigned_slots::PermanentSlotCount::<Test>::get(), 0);
		assert_eq!(AssignedSlots::has_permanent_slot(ParaId::from(1_u32)), false);
		assert_eq!(assigned_slots::PermanentSlots::<Test>::get(ParaId::from(1_u32)), None);

		assert_eq!(mock::Slots::already_leased(ParaId::from(1_u32), 0, 2), false);
	});
}

#[test]
fn unassign_temp_slot_succeeds() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_ok!(TestRegistrar::<Test>::register(
			1,
			ParaId::from(1_u32),
			dummy_head_data(),
			dummy_validation_code(),
		));

		assert_ok!(AssignedSlots::assign_temp_parachain_slot(
			RuntimeOrigin::root(),
			ParaId::from(1_u32),
			SlotLeasePeriodStart::Current
		));

		assert_eq!(TestRegistrar::<Test>::is_parachain(ParaId::from(1_u32)), true);

		assert_ok!(AssignedSlots::unassign_parachain_slot(
			RuntimeOrigin::root(),
			ParaId::from(1_u32),
		));

		assert_eq!(assigned_slots::TemporarySlotCount::<Test>::get(), 0);
		assert_eq!(assigned_slots::ActiveTemporarySlotCount::<Test>::get(), 0);
		assert_eq!(AssignedSlots::has_temporary_slot(ParaId::from(1_u32)), false);
		assert_eq!(assigned_slots::TemporarySlots::<Test>::get(ParaId::from(1_u32)), None);

		assert_eq!(mock::Slots::already_leased(ParaId::from(1_u32), 0, 1), false);
	});
}
#[test]
fn set_max_permanent_slots_fails_for_no_root_origin() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_noop!(
			AssignedSlots::set_max_permanent_slots(RuntimeOrigin::signed(1), 5),
			BadOrigin
		);
	});
}
#[test]
fn set_max_permanent_slots_succeeds() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_eq!(MaxPermanentSlots::<Test>::get(), 2);
		assert_ok!(AssignedSlots::set_max_permanent_slots(RuntimeOrigin::root(), 10),);
		assert_eq!(MaxPermanentSlots::<Test>::get(), 10);
	});
}

#[test]
fn set_max_temporary_slots_fails_for_no_root_origin() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_noop!(
			AssignedSlots::set_max_temporary_slots(RuntimeOrigin::signed(1), 5),
			BadOrigin
		);
	});
}
#[test]
fn set_max_temporary_slots_succeeds() {
	new_test_ext().execute_with(|| {
		run_to_block(1);

		assert_eq!(MaxTemporarySlots::<Test>::get(), 6);
		assert_ok!(AssignedSlots::set_max_temporary_slots(RuntimeOrigin::root(), 12),);
		assert_eq!(MaxTemporarySlots::<Test>::get(), 12);
	});
}
