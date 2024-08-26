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

//! Unit tests related to the `fees` register and `PayFees` instruction.
//!
//! See [Fellowship RFC 105](https://github.com/polkadot-fellows/rfCs/pull/105)
//! and the [specification](https://github.com/polkadot-fellows/xcm-format) for more information.

use codec::Encode;
use xcm::prelude::*;

use super::mock::*;
use crate::XcmExecutor;

#[test]
fn works_with_set_asset_claimer() {
    let sender = Location::new(0, [AccountId32 { id: [0; 32], network: None }]);
    let recipient = Location::new(0, [AccountId32 { id: [1; 32], network: None }]);

    // Make sure the user has enough funds to withdraw.
    add_asset(sender.clone(), (Here, 100u128));

    // Build xcm.
    let xcm = Xcm::<TestCall>::builder()
        // .set_asset_claimer(sender)
        .withdraw_asset((Here, 100u128))
        .pay_fees((Here, 10u128)) // 10% destined for fees, not more.
        .clear_origin()
        .build();

    // We create an XCVM instance instead of calling `XcmExecutor::<_>::prepare_and_execute` so we
    // can inspect its fields.
    let mut vm =
        XcmExecutor::<XcmConfig>::new(sender, xcm.using_encoded(sp_io::hashing::blake2_256));
    vm.message_weight = XcmExecutor::<XcmConfig>::prepare(xcm.clone()).unwrap().weight_of();

    let result = vm.bench_process(xcm);
    assert!(result.is_ok());

    assert_eq!(get_first_fungible(vm.holding()), None);
    // Execution fees were 4, so we still have 6 left in the `fees` register.
    assert_eq!(get_first_fungible(vm.fees()).unwrap(), (Here, 6u128).into());

    // The recipient received all the assets in the holding register, so `100` that
    // were withdrawn minus the `10` that were destinated for fee payment.
    assert_eq!(asset_list(recipient), [(Here, 90u128).into()]);
}

#[test]
fn works_without_set_asset_claimer() {
}