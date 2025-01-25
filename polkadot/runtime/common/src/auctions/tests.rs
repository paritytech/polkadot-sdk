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

//! Tests for the auctions pallet.

#[cfg(test)]
use super::*;
use crate::{auctions::mock::*, mock::TestRegistrar};
use frame_support::{assert_noop, assert_ok, assert_storage_noop};
use pallet_balances;
use polkadot_primitives::Id as ParaId;
use polkadot_primitives_test_helpers::{dummy_hash, dummy_head_data, dummy_validation_code};
use sp_core::H256;
use sp_runtime::DispatchError::BadOrigin;

#[test]
fn basic_setup_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(AuctionCounter::<Test>::get(), 0);
		assert_eq!(TestLeaser::deposit_held(0u32.into(), &1), 0);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::NotStarted
		);

		System::run_to_block::<AllPalletsWithSystem>(10);

		assert_eq!(AuctionCounter::<Test>::get(), 0);
		assert_eq!(TestLeaser::deposit_held(0u32.into(), &1), 0);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::NotStarted
		);
	});
}

#[test]
fn can_start_auction() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);

		assert_noop!(Auctions::new_auction(RuntimeOrigin::signed(1), 5, 1), BadOrigin);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));

		assert_eq!(AuctionCounter::<Test>::get(), 1);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);
	});
}

#[test]
fn bidding_works() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 1, 4, 5));

		assert_eq!(Balances::reserved_balance(1), 5);
		assert_eq!(Balances::free_balance(1), 5);
		assert_eq!(
			Winning::<Test>::get(0).unwrap()[SlotRange::ZeroThree as u8 as usize],
			Some((1, 0.into(), 5))
		);
	});
}

#[test]
fn under_bidding_works() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));

		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 1, 4, 5));

		assert_storage_noop!({
			assert_ok!(Auctions::bid(RuntimeOrigin::signed(2), 0.into(), 1, 1, 4, 1));
		});
	});
}

#[test]
fn over_bidding_works() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 1, 4, 5));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(2), 0.into(), 1, 1, 4, 6));

		assert_eq!(Balances::reserved_balance(1), 0);
		assert_eq!(Balances::free_balance(1), 10);
		assert_eq!(Balances::reserved_balance(2), 6);
		assert_eq!(Balances::free_balance(2), 14);
		assert_eq!(
			Winning::<Test>::get(0).unwrap()[SlotRange::ZeroThree as u8 as usize],
			Some((2, 0.into(), 6))
		);
	});
}

#[test]
fn auction_proceeds_correctly() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);

		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));

		assert_eq!(AuctionCounter::<Test>::get(), 1);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);

		System::run_to_block::<AllPalletsWithSystem>(2);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);

		System::run_to_block::<AllPalletsWithSystem>(3);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);

		System::run_to_block::<AllPalletsWithSystem>(4);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);

		System::run_to_block::<AllPalletsWithSystem>(5);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);

		System::run_to_block::<AllPalletsWithSystem>(6);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(0, 0)
		);

		System::run_to_block::<AllPalletsWithSystem>(7);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(1, 0)
		);

		System::run_to_block::<AllPalletsWithSystem>(8);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(2, 0)
		);

		System::run_to_block::<AllPalletsWithSystem>(9);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::NotStarted
		);
	});
}

#[test]
fn can_win_auction() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 1, 4, 1));
		assert_eq!(Balances::reserved_balance(1), 1);
		assert_eq!(Balances::free_balance(1), 9);
		System::run_to_block::<AllPalletsWithSystem>(9);

		assert_eq!(
			leases(),
			vec![
				((0.into(), 1), LeaseData { leaser: 1, amount: 1 }),
				((0.into(), 2), LeaseData { leaser: 1, amount: 1 }),
				((0.into(), 3), LeaseData { leaser: 1, amount: 1 }),
				((0.into(), 4), LeaseData { leaser: 1, amount: 1 }),
			]
		);
		assert_eq!(TestLeaser::deposit_held(0.into(), &1), 1);
	});
}

#[test]
fn can_win_auction_with_late_randomness() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 1, 4, 1));
		assert_eq!(Balances::reserved_balance(1), 1);
		assert_eq!(Balances::free_balance(1), 9);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);
		System::run_to_block::<AllPalletsWithSystem>(8);
		// Auction has not yet ended.
		assert_eq!(leases(), vec![]);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(2, 0)
		);
		// This will prevent the auction's winner from being decided in the next block, since
		// the random seed was known before the final bids were made.
		set_last_random(H256::zero(), 8);
		// Auction definitely ended now, but we don't know exactly when in the last 3 blocks yet
		// since no randomness available yet.
		System::run_to_block::<AllPalletsWithSystem>(9);
		// Auction has now ended... But auction winner still not yet decided, so no leases yet.
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::VrfDelay(0)
		);
		assert_eq!(leases(), vec![]);

		// Random seed now updated to a value known at block 9, when the auction ended. This
		// means that the winner can now be chosen.
		set_last_random(H256::zero(), 9);
		System::run_to_block::<AllPalletsWithSystem>(10);
		// Auction ended and winner selected
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::NotStarted
		);
		assert_eq!(
			leases(),
			vec![
				((0.into(), 1), LeaseData { leaser: 1, amount: 1 }),
				((0.into(), 2), LeaseData { leaser: 1, amount: 1 }),
				((0.into(), 3), LeaseData { leaser: 1, amount: 1 }),
				((0.into(), 4), LeaseData { leaser: 1, amount: 1 }),
			]
		);
		assert_eq!(TestLeaser::deposit_held(0.into(), &1), 1);
	});
}

#[test]
fn can_win_incomplete_auction() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 4, 4, 5));
		System::run_to_block::<AllPalletsWithSystem>(9);

		assert_eq!(leases(), vec![((0.into(), 4), LeaseData { leaser: 1, amount: 5 }),]);
		assert_eq!(TestLeaser::deposit_held(0.into(), &1), 5);
	});
}

#[test]
fn should_choose_best_combination() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 1, 1, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(2), 0.into(), 1, 2, 3, 4));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(3), 0.into(), 1, 4, 4, 2));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 1.into(), 1, 1, 4, 2));
		System::run_to_block::<AllPalletsWithSystem>(9);

		assert_eq!(
			leases(),
			vec![
				((0.into(), 1), LeaseData { leaser: 1, amount: 1 }),
				((0.into(), 2), LeaseData { leaser: 2, amount: 4 }),
				((0.into(), 3), LeaseData { leaser: 2, amount: 4 }),
				((0.into(), 4), LeaseData { leaser: 3, amount: 2 }),
			]
		);
		assert_eq!(TestLeaser::deposit_held(0.into(), &1), 1);
		assert_eq!(TestLeaser::deposit_held(1.into(), &1), 0);
		assert_eq!(TestLeaser::deposit_held(0.into(), &2), 4);
		assert_eq!(TestLeaser::deposit_held(0.into(), &3), 2);
	});
}

#[test]
fn gap_bid_works() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));

		// User 1 will make a bid for period 1 and 4 for the same Para 0
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 1, 1, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 4, 4, 4));

		// User 2 and 3 will make a bid for para 1 on period 2 and 3 respectively
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(2), 1.into(), 1, 2, 2, 2));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(3), 1.into(), 1, 3, 3, 3));

		// Total reserved should be the max of the two
		assert_eq!(Balances::reserved_balance(1), 4);

		// Other people are reserved correctly too
		assert_eq!(Balances::reserved_balance(2), 2);
		assert_eq!(Balances::reserved_balance(3), 3);

		// End the auction.
		System::run_to_block::<AllPalletsWithSystem>(9);

		assert_eq!(
			leases(),
			vec![
				((0.into(), 1), LeaseData { leaser: 1, amount: 1 }),
				((0.into(), 4), LeaseData { leaser: 1, amount: 4 }),
				((1.into(), 2), LeaseData { leaser: 2, amount: 2 }),
				((1.into(), 3), LeaseData { leaser: 3, amount: 3 }),
			]
		);
		assert_eq!(TestLeaser::deposit_held(0.into(), &1), 4);
		assert_eq!(TestLeaser::deposit_held(1.into(), &2), 2);
		assert_eq!(TestLeaser::deposit_held(1.into(), &3), 3);
	});
}

#[test]
fn deposit_credit_should_work() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 1, 1, 5));
		assert_eq!(Balances::reserved_balance(1), 5);
		System::run_to_block::<AllPalletsWithSystem>(10);

		assert_eq!(leases(), vec![((0.into(), 1), LeaseData { leaser: 1, amount: 5 }),]);
		assert_eq!(TestLeaser::deposit_held(0.into(), &1), 5);

		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 2));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 2, 2, 2, 6));
		// Only 1 reserved since we have a deposit credit of 5.
		assert_eq!(Balances::reserved_balance(1), 1);
		System::run_to_block::<AllPalletsWithSystem>(20);

		assert_eq!(
			leases(),
			vec![
				((0.into(), 1), LeaseData { leaser: 1, amount: 5 }),
				((0.into(), 2), LeaseData { leaser: 1, amount: 6 }),
			]
		);
		assert_eq!(TestLeaser::deposit_held(0.into(), &1), 6);
	});
}

#[test]
fn deposit_credit_on_alt_para_should_not_count() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 1, 1, 5));
		assert_eq!(Balances::reserved_balance(1), 5);
		System::run_to_block::<AllPalletsWithSystem>(10);

		assert_eq!(leases(), vec![((0.into(), 1), LeaseData { leaser: 1, amount: 5 }),]);
		assert_eq!(TestLeaser::deposit_held(0.into(), &1), 5);

		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 2));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 1.into(), 2, 2, 2, 6));
		// 6 reserved since we are bidding on a new para; only works because we don't
		assert_eq!(Balances::reserved_balance(1), 6);
		System::run_to_block::<AllPalletsWithSystem>(20);

		assert_eq!(
			leases(),
			vec![
				((0.into(), 1), LeaseData { leaser: 1, amount: 5 }),
				((1.into(), 2), LeaseData { leaser: 1, amount: 6 }),
			]
		);
		assert_eq!(TestLeaser::deposit_held(0.into(), &1), 5);
		assert_eq!(TestLeaser::deposit_held(1.into(), &1), 6);
	});
}

#[test]
fn multiple_bids_work_pre_ending() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);

		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));

		for i in 1..6u64 {
			System::run_to_block::<AllPalletsWithSystem>(i as _);
			assert_ok!(Auctions::bid(RuntimeOrigin::signed(i), 0.into(), 1, 1, 4, i));
			for j in 1..6 {
				assert_eq!(Balances::reserved_balance(j), if j == i { j } else { 0 });
				assert_eq!(Balances::free_balance(j), if j == i { j * 9 } else { j * 10 });
			}
		}

		System::run_to_block::<AllPalletsWithSystem>(9);
		assert_eq!(
			leases(),
			vec![
				((0.into(), 1), LeaseData { leaser: 5, amount: 5 }),
				((0.into(), 2), LeaseData { leaser: 5, amount: 5 }),
				((0.into(), 3), LeaseData { leaser: 5, amount: 5 }),
				((0.into(), 4), LeaseData { leaser: 5, amount: 5 }),
			]
		);
	});
}

#[test]
fn multiple_bids_work_post_ending() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);

		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 0, 1));

		for i in 1..6u64 {
			System::run_to_block::<AllPalletsWithSystem>(((i - 1) / 2 + 1) as _);
			assert_ok!(Auctions::bid(RuntimeOrigin::signed(i), 0.into(), 1, 1, 4, i));
			for j in 1..6 {
				assert_eq!(Balances::reserved_balance(j), if j <= i { j } else { 0 });
				assert_eq!(Balances::free_balance(j), if j <= i { j * 9 } else { j * 10 });
			}
		}
		for i in 1..6u64 {
			assert_eq!(ReservedAmounts::<Test>::get((i, ParaId::from(0))).unwrap(), i);
		}

		System::run_to_block::<AllPalletsWithSystem>(5);
		assert_eq!(
			leases(),
			(1..=4)
				.map(|i| ((0.into(), i), LeaseData { leaser: 2, amount: 2 }))
				.collect::<Vec<_>>()
		);
	});
}

#[test]
fn incomplete_calculate_winners_works() {
	let mut winning = [None; SlotRange::SLOT_RANGE_COUNT];
	winning[SlotRange::ThreeThree as u8 as usize] = Some((1, 0.into(), 1));

	let winners = vec![(1, 0.into(), 1, SlotRange::ThreeThree)];

	assert_eq!(Auctions::calculate_winners(winning), winners);
}

#[test]
fn first_incomplete_calculate_winners_works() {
	let mut winning = [None; SlotRange::SLOT_RANGE_COUNT];
	winning[0] = Some((1, 0.into(), 1));

	let winners = vec![(1, 0.into(), 1, SlotRange::ZeroZero)];

	assert_eq!(Auctions::calculate_winners(winning), winners);
}

#[test]
fn calculate_winners_works() {
	let mut winning = [None; SlotRange::SLOT_RANGE_COUNT];
	winning[SlotRange::ZeroZero as u8 as usize] = Some((2, 0.into(), 2));
	winning[SlotRange::ZeroThree as u8 as usize] = Some((1, 100.into(), 1));
	winning[SlotRange::OneOne as u8 as usize] = Some((3, 1.into(), 1));
	winning[SlotRange::TwoTwo as u8 as usize] = Some((1, 2.into(), 53));
	winning[SlotRange::ThreeThree as u8 as usize] = Some((5, 3.into(), 1));

	let winners = vec![
		(2, 0.into(), 2, SlotRange::ZeroZero),
		(3, 1.into(), 1, SlotRange::OneOne),
		(1, 2.into(), 53, SlotRange::TwoTwo),
		(5, 3.into(), 1, SlotRange::ThreeThree),
	];
	assert_eq!(Auctions::calculate_winners(winning), winners);

	winning[SlotRange::ZeroOne as u8 as usize] = Some((4, 10.into(), 3));
	let winners = vec![
		(4, 10.into(), 3, SlotRange::ZeroOne),
		(1, 2.into(), 53, SlotRange::TwoTwo),
		(5, 3.into(), 1, SlotRange::ThreeThree),
	];
	assert_eq!(Auctions::calculate_winners(winning), winners);

	winning[SlotRange::ZeroThree as u8 as usize] = Some((1, 100.into(), 100));
	let winners = vec![(1, 100.into(), 100, SlotRange::ZeroThree)];
	assert_eq!(Auctions::calculate_winners(winning), winners);
}

#[test]
fn lower_bids_are_correctly_refunded() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 1, 1));
		let para_1 = ParaId::from(1_u32);
		let para_2 = ParaId::from(2_u32);

		// Make a bid and reserve a balance
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), para_1, 1, 1, 4, 9));
		assert_eq!(Balances::reserved_balance(1), 9);
		assert_eq!(ReservedAmounts::<Test>::get((1, para_1)), Some(9));
		assert_eq!(Balances::reserved_balance(2), 0);
		assert_eq!(ReservedAmounts::<Test>::get((2, para_2)), None);

		// Bigger bid, reserves new balance and returns funds
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(2), para_2, 1, 1, 4, 19));
		assert_eq!(Balances::reserved_balance(1), 0);
		assert_eq!(ReservedAmounts::<Test>::get((1, para_1)), None);
		assert_eq!(Balances::reserved_balance(2), 19);
		assert_eq!(ReservedAmounts::<Test>::get((2, para_2)), Some(19));
	});
}

#[test]
fn initialize_winners_in_ending_period_works() {
	new_test_ext().execute_with(|| {
		let ed: u64 = <Test as pallet_balances::Config>::ExistentialDeposit::get();
		assert_eq!(ed, 1);
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 9, 1));
		let para_1 = ParaId::from(1_u32);
		let para_2 = ParaId::from(2_u32);
		let para_3 = ParaId::from(3_u32);

		// Make bids
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), para_1, 1, 1, 4, 9));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(2), para_2, 1, 3, 4, 19));

		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);
		let mut winning = [None; SlotRange::SLOT_RANGE_COUNT];
		winning[SlotRange::ZeroThree as u8 as usize] = Some((1, para_1, 9));
		winning[SlotRange::TwoThree as u8 as usize] = Some((2, para_2, 19));
		assert_eq!(Winning::<Test>::get(0), Some(winning));

		System::run_to_block::<AllPalletsWithSystem>(9);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);

		System::run_to_block::<AllPalletsWithSystem>(10);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(0, 0)
		);
		assert_eq!(Winning::<Test>::get(0), Some(winning));

		System::run_to_block::<AllPalletsWithSystem>(11);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(1, 0)
		);
		assert_eq!(Winning::<Test>::get(1), Some(winning));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(3), para_3, 1, 3, 4, 29));

		System::run_to_block::<AllPalletsWithSystem>(12);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(2, 0)
		);
		winning[SlotRange::TwoThree as u8 as usize] = Some((3, para_3, 29));
		assert_eq!(Winning::<Test>::get(2), Some(winning));
	});
}

#[test]
fn handle_bid_requires_registered_para() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_noop!(
			Auctions::bid(RuntimeOrigin::signed(1), 1337.into(), 1, 1, 4, 1),
			Error::<Test>::ParaNotRegistered
		);
		assert_ok!(TestRegistrar::<Test>::register(
			1,
			1337.into(),
			dummy_head_data(),
			dummy_validation_code()
		));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 1337.into(), 1, 1, 4, 1));
	});
}

#[test]
fn handle_bid_checks_existing_lease_periods() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 2, 3, 1));
		assert_eq!(Balances::reserved_balance(1), 1);
		assert_eq!(Balances::free_balance(1), 9);
		System::run_to_block::<AllPalletsWithSystem>(9);

		assert_eq!(
			leases(),
			vec![
				((0.into(), 2), LeaseData { leaser: 1, amount: 1 }),
				((0.into(), 3), LeaseData { leaser: 1, amount: 1 }),
			]
		);
		assert_eq!(TestLeaser::deposit_held(0.into(), &1), 1);

		// Para 1 just won an auction above and won some lease periods.
		// No bids can work which overlap these periods.
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_noop!(
			Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 2, 1, 4, 1),
			Error::<Test>::AlreadyLeasedOut,
		);
		assert_noop!(
			Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 2, 1, 2, 1),
			Error::<Test>::AlreadyLeasedOut,
		);
		assert_noop!(
			Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 2, 3, 4, 1),
			Error::<Test>::AlreadyLeasedOut,
		);
		// This is okay, not an overlapping bid.
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 2, 1, 1, 1));
	});
}

// Here we will test that taking only 10 samples during the ending period works as expected.
#[test]
fn less_winning_samples_work() {
	new_test_ext().execute_with(|| {
		let ed: u64 = <Test as pallet_balances::Config>::ExistentialDeposit::get();
		assert_eq!(ed, 1);
		EndingPeriod::set(30);
		SampleLength::set(10);

		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 9, 11));
		let para_1 = ParaId::from(1_u32);
		let para_2 = ParaId::from(2_u32);
		let para_3 = ParaId::from(3_u32);

		// Make bids
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), para_1, 1, 11, 14, 9));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(2), para_2, 1, 13, 14, 19));

		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);
		let mut winning = [None; SlotRange::SLOT_RANGE_COUNT];
		winning[SlotRange::ZeroThree as u8 as usize] = Some((1, para_1, 9));
		winning[SlotRange::TwoThree as u8 as usize] = Some((2, para_2, 19));
		assert_eq!(Winning::<Test>::get(0), Some(winning));

		System::run_to_block::<AllPalletsWithSystem>(9);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);

		System::run_to_block::<AllPalletsWithSystem>(10);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(0, 0)
		);
		assert_eq!(Winning::<Test>::get(0), Some(winning));

		// New bids update the current winning
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(3), para_3, 1, 14, 14, 29));
		winning[SlotRange::ThreeThree as u8 as usize] = Some((3, para_3, 29));
		assert_eq!(Winning::<Test>::get(0), Some(winning));

		System::run_to_block::<AllPalletsWithSystem>(20);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(1, 0)
		);
		assert_eq!(Winning::<Test>::get(1), Some(winning));
		System::run_to_block::<AllPalletsWithSystem>(25);
		// Overbid mid sample
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(3), para_3, 1, 13, 14, 29));
		winning[SlotRange::TwoThree as u8 as usize] = Some((3, para_3, 29));
		assert_eq!(Winning::<Test>::get(1), Some(winning));

		System::run_to_block::<AllPalletsWithSystem>(30);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(2, 0)
		);
		assert_eq!(Winning::<Test>::get(2), Some(winning));

		set_last_random(H256::from([254; 32]), 40);
		System::run_to_block::<AllPalletsWithSystem>(40);
		// Auction ended and winner selected
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::NotStarted
		);
		assert_eq!(
			leases(),
			vec![
				((3.into(), 13), LeaseData { leaser: 3, amount: 29 }),
				((3.into(), 14), LeaseData { leaser: 3, amount: 29 }),
			]
		);
	});
}

#[test]
fn auction_status_works() {
	new_test_ext().execute_with(|| {
		EndingPeriod::set(30);
		SampleLength::set(10);
		set_last_random(dummy_hash(), 0);

		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::NotStarted
		);

		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 9, 11));

		System::run_to_block::<AllPalletsWithSystem>(9);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::StartingPeriod
		);

		System::run_to_block::<AllPalletsWithSystem>(10);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(0, 0)
		);

		System::run_to_block::<AllPalletsWithSystem>(11);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(0, 1)
		);

		System::run_to_block::<AllPalletsWithSystem>(19);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(0, 9)
		);

		System::run_to_block::<AllPalletsWithSystem>(20);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(1, 0)
		);

		System::run_to_block::<AllPalletsWithSystem>(25);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(1, 5)
		);

		System::run_to_block::<AllPalletsWithSystem>(30);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(2, 0)
		);

		System::run_to_block::<AllPalletsWithSystem>(39);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::EndingPeriod(2, 9)
		);

		System::run_to_block::<AllPalletsWithSystem>(40);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::VrfDelay(0)
		);

		System::run_to_block::<AllPalletsWithSystem>(44);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::VrfDelay(4)
		);

		set_last_random(dummy_hash(), 45);
		System::run_to_block::<AllPalletsWithSystem>(45);
		assert_eq!(
			Auctions::auction_status(System::block_number()),
			AuctionStatus::<u32>::NotStarted
		);
	});
}

#[test]
fn can_cancel_auction() {
	new_test_ext().execute_with(|| {
		System::run_to_block::<AllPalletsWithSystem>(1);
		assert_ok!(Auctions::new_auction(RuntimeOrigin::signed(6), 5, 1));
		assert_ok!(Auctions::bid(RuntimeOrigin::signed(1), 0.into(), 1, 1, 4, 1));
		assert_eq!(Balances::reserved_balance(1), 1);
		assert_eq!(Balances::free_balance(1), 9);

		assert_noop!(Auctions::cancel_auction(RuntimeOrigin::signed(6)), BadOrigin);
		assert_ok!(Auctions::cancel_auction(RuntimeOrigin::root()));

		assert!(AuctionInfo::<Test>::get().is_none());
		assert_eq!(Balances::reserved_balance(1), 0);
		assert_eq!(ReservedAmounts::<Test>::iter().count(), 0);
		assert_eq!(Winning::<Test>::iter().count(), 0);
	});
}
