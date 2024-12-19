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

//! Tests for the crowdloan pallet.

#[cfg(test)]
use super::*;

use crate::crowdloan::mock::*;
use frame_support::{assert_noop, assert_ok};
use polkadot_primitives::Id as ParaId;
// The testing primitives are very useful for avoiding having to work with signatures
// or public keys. `u64` is used as the `AccountId` and no `Signature`s are required.
use crate::{
	crowdloan,
	mock::TestRegistrar,
	traits::{AuctionStatus, OnSwap},
};
use pallet_balances::Error as BalancesError;
use polkadot_primitives_test_helpers::{dummy_head_data, dummy_validation_code};
use sp_runtime::traits::TrailingZeroInput;

#[test]
fn basic_setup_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(System::block_number(), 0);
		assert_eq!(crowdloan::Funds::<Test>::get(ParaId::from(0)), None);
		let empty: Vec<ParaId> = Vec::new();
		assert_eq!(crowdloan::NewRaise::<Test>::get(), empty);
		assert_eq!(Crowdloan::contribution_get(0u32, &1).0, 0);
		assert_eq!(crowdloan::EndingsCount::<Test>::get(), 0);

		assert_ok!(TestAuctioneer::new_auction(5, 0));

		assert_eq!(bids(), vec![]);
		assert_ok!(TestAuctioneer::place_bid(1, 2.into(), 0, 3, 6));
		let b = BidPlaced {
			height: 0,
			bidder: 1,
			para: 2.into(),
			first_period: 0,
			last_period: 3,
			amount: 6,
		};
		assert_eq!(bids(), vec![b]);
		assert_eq!(TestAuctioneer::auction_status(4), AuctionStatus::<u64>::StartingPeriod);
		assert_eq!(TestAuctioneer::auction_status(5), AuctionStatus::<u64>::EndingPeriod(0, 0));
		assert_eq!(TestAuctioneer::auction_status(9), AuctionStatus::<u64>::EndingPeriod(4, 0));
		assert_eq!(TestAuctioneer::auction_status(11), AuctionStatus::<u64>::NotStarted);
	});
}

#[test]
fn create_works() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		// Now try to create a crowdloan campaign
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 4, 9, None));
		// This is what the initial `fund_info` should look like
		let fund_info = FundInfo {
			depositor: 1,
			verifier: None,
			deposit: 1,
			raised: 0,
			// 5 blocks length + 3 block ending period + 1 starting block
			end: 9,
			cap: 1000,
			last_contribution: LastContribution::Never,
			first_period: 1,
			last_period: 4,
			fund_index: 0,
		};
		assert_eq!(crowdloan::Funds::<Test>::get(para), Some(fund_info));
		// User has deposit removed from their free balance
		assert_eq!(Balances::free_balance(1), 999);
		// Deposit is placed in reserved
		assert_eq!(Balances::reserved_balance(1), 1);
		// No new raise until first contribution
		let empty: Vec<ParaId> = Vec::new();
		assert_eq!(crowdloan::NewRaise::<Test>::get(), empty);
	});
}

#[test]
fn create_with_verifier_works() {
	new_test_ext().execute_with(|| {
		let pubkey = crypto::create_ed25519_pubkey(b"//verifier".to_vec());
		let para = new_para();
		// Now try to create a crowdloan campaign
		assert_ok!(Crowdloan::create(
			RuntimeOrigin::signed(1),
			para,
			1000,
			1,
			4,
			9,
			Some(pubkey.clone())
		));
		// This is what the initial `fund_info` should look like
		let fund_info = FundInfo {
			depositor: 1,
			verifier: Some(pubkey),
			deposit: 1,
			raised: 0,
			// 5 blocks length + 3 block ending period + 1 starting block
			end: 9,
			cap: 1000,
			last_contribution: LastContribution::Never,
			first_period: 1,
			last_period: 4,
			fund_index: 0,
		};
		assert_eq!(crowdloan::Funds::<Test>::get(ParaId::from(0)), Some(fund_info));
		// User has deposit removed from their free balance
		assert_eq!(Balances::free_balance(1), 999);
		// Deposit is placed in reserved
		assert_eq!(Balances::reserved_balance(1), 1);
		// No new raise until first contribution
		let empty: Vec<ParaId> = Vec::new();
		assert_eq!(crowdloan::NewRaise::<Test>::get(), empty);
	});
}

#[test]
fn create_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		// Now try to create a crowdloan campaign
		let para = new_para();

		let e = Error::<Test>::InvalidParaId;
		assert_noop!(Crowdloan::create(RuntimeOrigin::signed(1), 1.into(), 1000, 1, 4, 9, None), e);
		// Cannot create a crowdloan with bad lease periods
		let e = Error::<Test>::LastPeriodBeforeFirstPeriod;
		assert_noop!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 4, 1, 9, None), e);
		let e = Error::<Test>::LastPeriodTooFarInFuture;
		assert_noop!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 9, 9, None), e);

		// Cannot create a crowdloan without some deposit funds
		assert_ok!(TestRegistrar::<Test>::register(
			1337,
			ParaId::from(1234),
			dummy_head_data(),
			dummy_validation_code()
		));
		let e = BalancesError::<Test, _>::InsufficientBalance;
		assert_noop!(
			Crowdloan::create(RuntimeOrigin::signed(1337), ParaId::from(1234), 1000, 1, 3, 9, None),
			e
		);

		// Cannot create a crowdloan with nonsense end date
		// This crowdloan would end in lease period 2, but is bidding for some slot that starts
		// in lease period 1.
		assert_noop!(
			Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 4, 41, None),
			Error::<Test>::EndTooFarInFuture
		);
	});
}

#[test]
fn contribute_works() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		let index = NextFundIndex::<Test>::get();

		// Set up a crowdloan
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 4, 9, None));

		// No contributions yet
		assert_eq!(Crowdloan::contribution_get(u32::from(para), &1).0, 0);

		// User 1 contributes to their own crowdloan
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(1), para, 49, None));
		// User 1 has spent some funds to do this, transfer fees **are** taken
		assert_eq!(Balances::free_balance(1), 950);
		// Contributions are stored in the trie
		assert_eq!(Crowdloan::contribution_get(u32::from(para), &1).0, 49);
		// Contributions appear in free balance of crowdloan
		assert_eq!(Balances::free_balance(Crowdloan::fund_account_id(index)), 49);
		// Crowdloan is added to NewRaise
		assert_eq!(crowdloan::NewRaise::<Test>::get(), vec![para]);

		let fund = crowdloan::Funds::<Test>::get(para).unwrap();

		// Last contribution time recorded
		assert_eq!(fund.last_contribution, LastContribution::PreEnding(0));
		assert_eq!(fund.raised, 49);
	});
}

#[test]
fn contribute_with_verifier_works() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		let index = NextFundIndex::<Test>::get();
		let pubkey = crypto::create_ed25519_pubkey(b"//verifier".to_vec());
		// Set up a crowdloan
		assert_ok!(Crowdloan::create(
			RuntimeOrigin::signed(1),
			para,
			1000,
			1,
			4,
			9,
			Some(pubkey.clone())
		));

		// No contributions yet
		assert_eq!(Crowdloan::contribution_get(u32::from(para), &1).0, 0);

		// Missing signature
		assert_noop!(
			Crowdloan::contribute(RuntimeOrigin::signed(1), para, 49, None),
			Error::<Test>::InvalidSignature
		);

		let payload = (0u32, 1u64, 0u64, 49u64);
		let valid_signature = crypto::create_ed25519_signature(&payload.encode(), pubkey.clone());
		let invalid_signature = MultiSignature::decode(&mut TrailingZeroInput::zeroes()).unwrap();

		// Invalid signature
		assert_noop!(
			Crowdloan::contribute(RuntimeOrigin::signed(1), para, 49, Some(invalid_signature)),
			Error::<Test>::InvalidSignature
		);

		// Valid signature wrong parameter
		assert_noop!(
			Crowdloan::contribute(
				RuntimeOrigin::signed(1),
				para,
				50,
				Some(valid_signature.clone())
			),
			Error::<Test>::InvalidSignature
		);
		assert_noop!(
			Crowdloan::contribute(
				RuntimeOrigin::signed(2),
				para,
				49,
				Some(valid_signature.clone())
			),
			Error::<Test>::InvalidSignature
		);

		// Valid signature
		assert_ok!(Crowdloan::contribute(
			RuntimeOrigin::signed(1),
			para,
			49,
			Some(valid_signature.clone())
		));

		// Reuse valid signature
		assert_noop!(
			Crowdloan::contribute(RuntimeOrigin::signed(1), para, 49, Some(valid_signature)),
			Error::<Test>::InvalidSignature
		);

		let payload_2 = (0u32, 1u64, 49u64, 10u64);
		let valid_signature_2 = crypto::create_ed25519_signature(&payload_2.encode(), pubkey);

		// New valid signature
		assert_ok!(Crowdloan::contribute(
			RuntimeOrigin::signed(1),
			para,
			10,
			Some(valid_signature_2)
		));

		// Contributions appear in free balance of crowdloan
		assert_eq!(Balances::free_balance(Crowdloan::fund_account_id(index)), 59);

		// Contribution amount is correct
		let fund = crowdloan::Funds::<Test>::get(para).unwrap();
		assert_eq!(fund.raised, 59);
	});
}

#[test]
fn contribute_handles_basic_errors() {
	new_test_ext().execute_with(|| {
		let para = new_para();

		// Cannot contribute to non-existing fund
		assert_noop!(
			Crowdloan::contribute(RuntimeOrigin::signed(1), para, 49, None),
			Error::<Test>::InvalidParaId
		);
		// Cannot contribute below minimum contribution
		assert_noop!(
			Crowdloan::contribute(RuntimeOrigin::signed(1), para, 9, None),
			Error::<Test>::ContributionTooSmall
		);

		// Set up a crowdloan
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 4, 9, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(1), para, 101, None));

		// Cannot contribute past the limit
		assert_noop!(
			Crowdloan::contribute(RuntimeOrigin::signed(2), para, 900, None),
			Error::<Test>::CapExceeded
		);

		// Move past end date
		run_to_block(10);

		// Cannot contribute to ended fund
		assert_noop!(
			Crowdloan::contribute(RuntimeOrigin::signed(1), para, 49, None),
			Error::<Test>::ContributionPeriodOver
		);

		// If a crowdloan has already won, it should not allow contributions.
		let para_2 = new_para();
		let index = NextFundIndex::<Test>::get();
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para_2, 1000, 1, 4, 40, None));
		// Emulate a win by leasing out and putting a deposit. Slots pallet would normally do
		// this.
		let crowdloan_account = Crowdloan::fund_account_id(index);
		set_winner(para_2, crowdloan_account, true);
		assert_noop!(
			Crowdloan::contribute(RuntimeOrigin::signed(1), para_2, 49, None),
			Error::<Test>::BidOrLeaseActive
		);

		// Move past lease period 1, should not be allowed to have further contributions with a
		// crowdloan that has starting period 1.
		let para_3 = new_para();
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para_3, 1000, 1, 4, 40, None));
		run_to_block(40);
		let now = System::block_number();
		assert_eq!(TestAuctioneer::lease_period_index(now).unwrap().0, 2);
		assert_noop!(
			Crowdloan::contribute(RuntimeOrigin::signed(1), para_3, 49, None),
			Error::<Test>::ContributionPeriodOver
		);
	});
}

#[test]
fn cannot_contribute_during_vrf() {
	new_test_ext().execute_with(|| {
		set_vrf_delay(5);

		let para = new_para();
		let first_period = 1;
		let last_period = 4;

		assert_ok!(TestAuctioneer::new_auction(5, 0));

		// Set up a crowdloan
		assert_ok!(Crowdloan::create(
			RuntimeOrigin::signed(1),
			para,
			1000,
			first_period,
			last_period,
			20,
			None
		));

		run_to_block(8);
		// Can def contribute when auction is running.
		assert!(TestAuctioneer::auction_status(System::block_number()).is_ending().is_some());
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 250, None));

		run_to_block(10);
		// Can't contribute when auction is in the VRF delay period.
		assert!(TestAuctioneer::auction_status(System::block_number()).is_vrf());
		assert_noop!(
			Crowdloan::contribute(RuntimeOrigin::signed(2), para, 250, None),
			Error::<Test>::VrfDelayInProgress
		);

		run_to_block(15);
		// Its fine to contribute when no auction is running.
		assert!(!TestAuctioneer::auction_status(System::block_number()).is_in_progress());
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 250, None));
	})
}

#[test]
fn bidding_works() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		let index = NextFundIndex::<Test>::get();
		let first_period = 1;
		let last_period = 4;

		assert_ok!(TestAuctioneer::new_auction(5, 0));

		// Set up a crowdloan
		assert_ok!(Crowdloan::create(
			RuntimeOrigin::signed(1),
			para,
			1000,
			first_period,
			last_period,
			9,
			None
		));
		let bidder = Crowdloan::fund_account_id(index);

		// Fund crowdloan
		run_to_block(1);
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 100, None));
		run_to_block(3);
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(3), para, 150, None));
		run_to_block(5);
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(4), para, 200, None));
		run_to_block(8);
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 250, None));
		run_to_block(10);

		assert_eq!(
			bids(),
			vec![
				BidPlaced { height: 5, amount: 250, bidder, para, first_period, last_period },
				BidPlaced { height: 6, amount: 450, bidder, para, first_period, last_period },
				BidPlaced { height: 9, amount: 700, bidder, para, first_period, last_period },
			]
		);

		// Endings count incremented
		assert_eq!(crowdloan::EndingsCount::<Test>::get(), 1);
	});
}

#[test]
fn withdraw_from_failed_works() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		let index = NextFundIndex::<Test>::get();

		// Set up a crowdloan
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 1, 9, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 100, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(3), para, 50, None));

		run_to_block(10);
		let account_id = Crowdloan::fund_account_id(index);
		// para has no reserved funds, indicating it did not win the auction.
		assert_eq!(Balances::reserved_balance(&account_id), 0);
		// but there's still the funds in its balance.
		assert_eq!(Balances::free_balance(&account_id), 150);
		assert_eq!(Balances::free_balance(2), 1900);
		assert_eq!(Balances::free_balance(3), 2950);

		assert_ok!(Crowdloan::withdraw(RuntimeOrigin::signed(2), 2, para));
		assert_eq!(Balances::free_balance(&account_id), 50);
		assert_eq!(Balances::free_balance(2), 2000);

		assert_ok!(Crowdloan::withdraw(RuntimeOrigin::signed(2), 3, para));
		assert_eq!(Balances::free_balance(&account_id), 0);
		assert_eq!(Balances::free_balance(3), 3000);
	});
}

#[test]
fn withdraw_cannot_be_griefed() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		let index = NextFundIndex::<Test>::get();
		let issuance = Balances::total_issuance();

		// Set up a crowdloan
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 1, 9, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 100, None));

		run_to_block(10);
		let account_id = Crowdloan::fund_account_id(index);

		// user sends the crowdloan funds trying to make an accounting error
		assert_ok!(Balances::transfer_allow_death(RuntimeOrigin::signed(1), account_id, 10));

		// overfunded now
		assert_eq!(Balances::free_balance(&account_id), 110);
		assert_eq!(Balances::free_balance(2), 1900);

		assert_ok!(Crowdloan::withdraw(RuntimeOrigin::signed(2), 2, para));
		assert_eq!(Balances::free_balance(2), 2000);

		// Some funds are left over
		assert_eq!(Balances::free_balance(&account_id), 10);
		// Remaining funds will be burned
		assert_ok!(Crowdloan::dissolve(RuntimeOrigin::signed(1), para));
		assert_eq!(Balances::free_balance(&account_id), 0);
		assert_eq!(Balances::total_issuance(), issuance - 10);
	});
}

#[test]
fn refund_works() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		let index = NextFundIndex::<Test>::get();
		let account_id = Crowdloan::fund_account_id(index);

		// Set up a crowdloan ending on 9
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 1, 9, None));
		// Make some contributions
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(1), para, 100, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 200, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(3), para, 300, None));

		assert_eq!(Balances::free_balance(account_id), 600);

		// Can't refund before the crowdloan it has ended
		assert_noop!(
			Crowdloan::refund(RuntimeOrigin::signed(1337), para),
			Error::<Test>::FundNotEnded,
		);

		// Move to the end of the crowdloan
		run_to_block(10);
		assert_ok!(Crowdloan::refund(RuntimeOrigin::signed(1337), para));

		// Funds are returned
		assert_eq!(Balances::free_balance(account_id), 0);
		// 1 deposit for the crowdloan which hasn't dissolved yet.
		assert_eq!(Balances::free_balance(1), 1000 - 1);
		assert_eq!(Balances::free_balance(2), 2000);
		assert_eq!(Balances::free_balance(3), 3000);
	});
}

#[test]
fn multiple_refund_works() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		let index = NextFundIndex::<Test>::get();
		let account_id = Crowdloan::fund_account_id(index);

		// Set up a crowdloan ending on 9
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 100000, 1, 1, 9, None));
		// Make more contributions than our limit
		for i in 1..=RemoveKeysLimit::get() * 2 {
			Balances::make_free_balance_be(&i.into(), (1000 * i).into());
			assert_ok!(Crowdloan::contribute(
				RuntimeOrigin::signed(i.into()),
				para,
				(i * 100).into(),
				None
			));
		}

		assert_eq!(Balances::free_balance(account_id), 21000);

		// Move to the end of the crowdloan
		run_to_block(10);
		assert_ok!(Crowdloan::refund(RuntimeOrigin::signed(1337), para));
		assert_eq!(last_event(), super::Event::<Test>::PartiallyRefunded { para_id: para }.into());

		// Funds still left over
		assert!(!Balances::free_balance(account_id).is_zero());

		// Call again
		assert_ok!(Crowdloan::refund(RuntimeOrigin::signed(1337), para));
		assert_eq!(last_event(), super::Event::<Test>::AllRefunded { para_id: para }.into());

		// Funds are returned
		assert_eq!(Balances::free_balance(account_id), 0);
		// 1 deposit for the crowdloan which hasn't dissolved yet.
		for i in 1..=RemoveKeysLimit::get() * 2 {
			assert_eq!(Balances::free_balance(&i.into()), i as u64 * 1000);
		}
	});
}

#[test]
fn refund_and_dissolve_works() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		let issuance = Balances::total_issuance();

		// Set up a crowdloan
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 1, 9, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 100, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(3), para, 50, None));

		run_to_block(10);
		// All funds are refunded
		assert_ok!(Crowdloan::refund(RuntimeOrigin::signed(2), para));

		// Now that `fund.raised` is zero, it can be dissolved.
		assert_ok!(Crowdloan::dissolve(RuntimeOrigin::signed(1), para));
		assert_eq!(Balances::free_balance(1), 1000);
		assert_eq!(Balances::free_balance(2), 2000);
		assert_eq!(Balances::free_balance(3), 3000);
		assert_eq!(Balances::total_issuance(), issuance);
	});
}

// Regression test to check that a pot account with just one provider can be dissolved.
#[test]
fn dissolve_provider_refs_total_issuance_works() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		let issuance = Balances::total_issuance();

		// Set up a crowdloan
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 1, 9, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 100, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(3), para, 50, None));

		run_to_block(10);

		// We test the historic case where crowdloan accounts only have one provider:
		{
			let fund = crowdloan::Funds::<Test>::get(para).unwrap();
			let pot = Crowdloan::fund_account_id(fund.fund_index);
			System::dec_providers(&pot).unwrap();
			assert_eq!(System::providers(&pot), 1);
		}

		// All funds are refunded
		assert_ok!(Crowdloan::refund(RuntimeOrigin::signed(2), para));

		// Now that `fund.raised` is zero, it can be dissolved.
		assert_ok!(Crowdloan::dissolve(RuntimeOrigin::signed(1), para));

		assert_eq!(Balances::free_balance(1), 1000);
		assert_eq!(Balances::free_balance(2), 2000);
		assert_eq!(Balances::free_balance(3), 3000);
		assert_eq!(Balances::total_issuance(), issuance);
	});
}

#[test]
fn dissolve_works() {
	new_test_ext().execute_with(|| {
		let para = new_para();
		let issuance = Balances::total_issuance();

		// Set up a crowdloan
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 1, 9, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 100, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(3), para, 50, None));

		// Can't dissolve before it ends
		assert_noop!(
			Crowdloan::dissolve(RuntimeOrigin::signed(1), para),
			Error::<Test>::NotReadyToDissolve
		);

		run_to_block(10);
		set_winner(para, 1, true);
		// Can't dissolve when it won.
		assert_noop!(
			Crowdloan::dissolve(RuntimeOrigin::signed(1), para),
			Error::<Test>::NotReadyToDissolve
		);
		set_winner(para, 1, false);

		// Can't dissolve while it still has user funds
		assert_noop!(
			Crowdloan::dissolve(RuntimeOrigin::signed(1), para),
			Error::<Test>::NotReadyToDissolve
		);

		// All funds are refunded
		assert_ok!(Crowdloan::refund(RuntimeOrigin::signed(2), para));

		// Now that `fund.raised` is zero, it can be dissolved.
		assert_ok!(Crowdloan::dissolve(RuntimeOrigin::signed(1), para));
		assert_eq!(Balances::free_balance(1), 1000);
		assert_eq!(Balances::free_balance(2), 2000);
		assert_eq!(Balances::free_balance(3), 3000);
		assert_eq!(Balances::total_issuance(), issuance);
	});
}

#[test]
fn withdraw_from_finished_works() {
	new_test_ext().execute_with(|| {
		let ed: u64 = <Test as pallet_balances::Config>::ExistentialDeposit::get();
		assert_eq!(ed, 1);
		let para = new_para();
		let index = NextFundIndex::<Test>::get();
		let account_id = Crowdloan::fund_account_id(index);

		// Set up a crowdloan
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para, 1000, 1, 1, 9, None));

		// Fund crowdloans.
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para, 100, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(3), para, 50, None));
		// simulate the reserving of para's funds. this actually happens in the Slots pallet.
		assert_ok!(Balances::reserve(&account_id, 149));

		run_to_block(19);
		assert_noop!(
			Crowdloan::withdraw(RuntimeOrigin::signed(2), 2, para),
			Error::<Test>::BidOrLeaseActive
		);

		run_to_block(20);
		// simulate the unreserving of para's funds, now that the lease expired. this actually
		// happens in the Slots pallet.
		Balances::unreserve(&account_id, 150);

		// para has no reserved funds, indicating it did ot win the auction.
		assert_eq!(Balances::reserved_balance(&account_id), 0);
		// but there's still the funds in its balance.
		assert_eq!(Balances::free_balance(&account_id), 150);
		assert_eq!(Balances::free_balance(2), 1900);
		assert_eq!(Balances::free_balance(3), 2950);

		assert_ok!(Crowdloan::withdraw(RuntimeOrigin::signed(2), 2, para));
		assert_eq!(Balances::free_balance(&account_id), 50);
		assert_eq!(Balances::free_balance(2), 2000);

		assert_ok!(Crowdloan::withdraw(RuntimeOrigin::signed(2), 3, para));
		assert_eq!(Balances::free_balance(&account_id), 0);
		assert_eq!(Balances::free_balance(3), 3000);
	});
}

#[test]
fn on_swap_works() {
	new_test_ext().execute_with(|| {
		let para_1 = new_para();
		let para_2 = new_para();

		// Set up crowdloans
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para_1, 1000, 1, 1, 9, None));
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para_2, 1000, 1, 1, 9, None));
		// Different contributions
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para_1, 100, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(3), para_2, 50, None));
		// Original state
		assert_eq!(Funds::<Test>::get(para_1).unwrap().raised, 100);
		assert_eq!(Funds::<Test>::get(para_2).unwrap().raised, 50);
		// Swap
		Crowdloan::on_swap(para_1, para_2);
		// Final state
		assert_eq!(Funds::<Test>::get(para_2).unwrap().raised, 100);
		assert_eq!(Funds::<Test>::get(para_1).unwrap().raised, 50);
	});
}

#[test]
fn cannot_create_fund_when_already_active() {
	new_test_ext().execute_with(|| {
		let para_1 = new_para();

		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para_1, 1000, 1, 1, 9, None));
		// Cannot create a fund again
		assert_noop!(
			Crowdloan::create(RuntimeOrigin::signed(1), para_1, 1000, 1, 1, 9, None),
			Error::<Test>::FundNotEnded,
		);
	});
}

#[test]
fn edit_works() {
	new_test_ext().execute_with(|| {
		let para_1 = new_para();

		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para_1, 1000, 1, 1, 9, None));
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para_1, 100, None));
		let old_crowdloan = crowdloan::Funds::<Test>::get(para_1).unwrap();

		assert_ok!(Crowdloan::edit(RuntimeOrigin::root(), para_1, 1234, 2, 3, 4, None));
		let new_crowdloan = crowdloan::Funds::<Test>::get(para_1).unwrap();

		// Some things stay the same
		assert_eq!(old_crowdloan.depositor, new_crowdloan.depositor);
		assert_eq!(old_crowdloan.deposit, new_crowdloan.deposit);
		assert_eq!(old_crowdloan.raised, new_crowdloan.raised);

		// Some things change
		assert!(old_crowdloan.cap != new_crowdloan.cap);
		assert!(old_crowdloan.first_period != new_crowdloan.first_period);
		assert!(old_crowdloan.last_period != new_crowdloan.last_period);
	});
}

#[test]
fn add_memo_works() {
	new_test_ext().execute_with(|| {
		let para_1 = new_para();

		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para_1, 1000, 1, 1, 9, None));
		// Cant add a memo before you have contributed.
		assert_noop!(
			Crowdloan::add_memo(RuntimeOrigin::signed(1), para_1, b"hello, world".to_vec()),
			Error::<Test>::NoContributions,
		);
		// Make a contribution. Initially no memo.
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(1), para_1, 100, None));
		assert_eq!(Crowdloan::contribution_get(0u32, &1), (100, vec![]));
		// Can't place a memo that is too large.
		assert_noop!(
			Crowdloan::add_memo(RuntimeOrigin::signed(1), para_1, vec![123; 123]),
			Error::<Test>::MemoTooLarge,
		);
		// Adding a memo to an existing contribution works
		assert_ok!(Crowdloan::add_memo(RuntimeOrigin::signed(1), para_1, b"hello, world".to_vec()));
		assert_eq!(Crowdloan::contribution_get(0u32, &1), (100, b"hello, world".to_vec()));
		// Can contribute again and data persists
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(1), para_1, 100, None));
		assert_eq!(Crowdloan::contribution_get(0u32, &1), (200, b"hello, world".to_vec()));
	});
}

#[test]
fn poke_works() {
	new_test_ext().execute_with(|| {
		let para_1 = new_para();

		assert_ok!(TestAuctioneer::new_auction(5, 0));
		assert_ok!(Crowdloan::create(RuntimeOrigin::signed(1), para_1, 1000, 1, 1, 9, None));
		// Should fail when no contributions.
		assert_noop!(
			Crowdloan::poke(RuntimeOrigin::signed(1), para_1),
			Error::<Test>::NoContributions
		);
		assert_ok!(Crowdloan::contribute(RuntimeOrigin::signed(2), para_1, 100, None));
		run_to_block(6);
		assert_ok!(Crowdloan::poke(RuntimeOrigin::signed(1), para_1));
		assert_eq!(crowdloan::NewRaise::<Test>::get(), vec![para_1]);
		assert_noop!(
			Crowdloan::poke(RuntimeOrigin::signed(1), para_1),
			Error::<Test>::AlreadyInNewRaise
		);
	});
}
