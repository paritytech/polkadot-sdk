// Copyright Parity Technologies (UK) Ltd.
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

//! # Reserve asset transfers.
//!
//! These are the types of cross-chain transfers that require three parties:
//! the sender (A), the receiver (B), and the reserve (R).
//! The reserve is the chain that holds the real asset, while the sender and receiver hold only
//! derivatives of it.
//!
//! There are 3 cases of a reserve asset transfer.
//! 1. When the reserve is a third chain, so the transfer looks like A->R->B.
//! 2. When the reserve is the sender chain, so A=R->B.
//! 3. When the reserve is the receiver chain, so A->B=R.
//!
//! We'll be going through all of them and showing examples of how to write messages that accomplish
//! these transfers.
//!
//! ## 1. Remote reserve
//!
//! This is the case where the reserve is a third chain.
//!
#![doc = docify::embed!("src/cookbook/writing_messages/reserve_asset_transfers.rs", remote_reserve)]

use frame::testing_prelude::*;
use test_log::test;
use xcm::prelude::*;
use xcm_simulator::TestExt;

use super::{parachain, network::{UNITS, ALICE, BOB, ParaA, MockNet}};

#[docify::export]
#[test]
fn remote_reserve() {
	MockNet::reset();

    let one_token = 1 * UNITS;
    let fee_amount = one_token / 10; // We dedicate 10% of our assets for fee payment.
    let beneficiary = BOB;

    let xcm_in_receiver = Xcm::<()>::builder_unsafe()
        .buy_execution((Parent, fee_amount), Unlimited)
        .deposit_asset(AllCounted(1), beneficiary)
        .build();
    let receiver = Location::new(1, [Parachain(2001)]);
    let xcm_in_reserve = Xcm::<()>::builder_unsafe()
        .buy_execution((Parent, fee_amount), Unlimited)
        .deposit_reserve_asset(
            AllCounted(1),
            receiver,
            xcm_in_receiver,
        )
        .build();
    let reserve = Location::new(1, [Parachain(1000)]);
    let xcm = Xcm::<parachain::RuntimeCall>::builder()
        .withdraw_asset((Parent, one_token))
        .buy_execution((Parent, fee_amount), Unlimited)
        .initiate_reserve_withdraw(
            AllCounted(1),
            reserve,
            xcm_in_reserve,
        )
        .build();

    ParaA::execute_with(|| {
        assert_ok!(parachain::PolkadotXcm::execute(
            parachain::RuntimeOrigin::signed(ALICE),
            Box::new(VersionedXcm::from(xcm)),
            Weight::from_parts(10_000_000_000_000, 500_000), // A big enough value.
        ));
    });
}

#[docify::export]
#[test]
fn sender_reserve() {
    let one_token = 1 * UNITS;
    let fee_amount = one_token / 10;

    let xcm = Xcm::<parachain::RuntimeCall>::builder()
        .withdraw_asset((Here, one_token))
        .buy_execution((Here, fee_amount), Unlimited)
        .build();

    ParaA::execute_with(|| {
        assert_ok!(parachain::PolkadotXcm::execute(
            parachain::RuntimeOrigin::signed(ALICE),
            Box::new(VersionedXcm::from(xcm)),
            Weight::from_parts(10_000_000_000_000, 500_000), // A big enough value.
        ));
    });
}

#[docify::export]
#[test]
fn receiver_reserve() {
    
}
