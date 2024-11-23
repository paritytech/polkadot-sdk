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

//! Unit tests for the `ExecuteWithOrigin` instruction.
//!
//! See the [XCM RFC](https://github.com/polkadot-fellows/xcm-format/pull/38)
//! and the [specification](https://github.com/polkadot-fellows/xcm-format/tree/8cef08e375c6f6d3966909ccf773ed46ac703917) for more information.
//!
//! The XCM RFCs were moved to the fellowship RFCs but this one was approved and merged before that.

use xcm::prelude::*;

use super::mock::*;
use crate::ExecutorError;

// The sender and recipient we use across these tests.
const SENDER_1: [u8; 32] = [0; 32];
const SENDER_2: [u8; 32] = [1; 32];
const RECIPIENT: [u8; 32] = [2; 32];

// ===== Happy path =====

// In this test, root descends into one account to pay fees, pops that origin
// and descends into a second account to withdraw funds.
// These assets can now be used to perform actions as root.
#[test]
fn root_can_descend_into_more_than_one_account() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER_1, (Here, 10u128));
	add_asset(SENDER_2, (Here, 100u128));

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder_unsafe()
		.execute_with_origin(
			Some(SENDER_1.into()),
			Xcm::<TestCall>::builder_unsafe()
				.withdraw_asset((Here, 10u128))
				.pay_fees((Here, 10u128))
				.build(),
		)
		.execute_with_origin(
			Some(SENDER_2.into()),
			Xcm::<TestCall>::builder_unsafe().withdraw_asset((Here, 100u128)).build(),
		)
		.expect_origin(Some(Here.into()))
		.deposit_asset(All, RECIPIENT)
		.build();

	let (mut vm, weight) = instantiate_executor(Here, xcm.clone());

	// Program runs successfully.
	assert!(vm.bench_process(xcm).is_ok());
	assert!(vm.bench_post_process(weight).ensure_complete().is_ok());

	// RECIPIENT gets the funds.
	assert_eq!(asset_list(RECIPIENT), [(Here, 100u128).into()]);
}

// ExecuteWithOrigin works for clearing the origin as well.
#[test]
fn works_for_clearing_origin() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER_1, (Here, 100u128));

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder_unsafe()
		// Root code.
		.expect_origin(Some(Here.into()))
		.execute_with_origin(
			None,
			// User code, we run it with no origin.
			Xcm::<TestCall>::builder_unsafe().expect_origin(None).build(),
		)
		// We go back to root code.
		.build();

	let (mut vm, weight) = instantiate_executor(Here, xcm.clone());

	// Program runs successfully.
	assert!(vm.bench_process(xcm).is_ok());
	assert!(vm.bench_post_process(weight).ensure_complete().is_ok());
}

// Setting the error handler or appendix inside of `ExecuteWithOrigin`
// will work as expected.
#[test]
fn set_error_handler_and_appendix_work() {
	add_asset(SENDER_1, (Here, 110u128));

	let xcm = Xcm::<TestCall>::builder_unsafe()
		.execute_with_origin(
			Some(SENDER_1.into()),
			Xcm::<TestCall>::builder_unsafe()
				.withdraw_asset((Here, 110u128))
				.pay_fees((Here, 10u128))
				.set_error_handler(
					Xcm::<TestCall>::builder_unsafe()
						.deposit_asset(vec![(Here, 10u128).into()], SENDER_2)
						.build(),
				)
				.set_appendix(
					Xcm::<TestCall>::builder_unsafe().deposit_asset(All, RECIPIENT).build(),
				)
				.build(),
		)
		.build();

	let (mut vm, weight) = instantiate_executor(Here, xcm.clone());

	// Program runs successfully.
	assert!(vm.bench_process(xcm).is_ok());

	assert_eq!(
		vm.error_handler(),
		&Xcm::<TestCall>(vec![DepositAsset {
			assets: vec![Asset { id: AssetId(Location::new(0, [])), fun: Fungible(10) }].into(),
			beneficiary: Location::new(0, [AccountId32 { id: SENDER_2, network: None }]),
		},])
	);
	assert_eq!(
		vm.appendix(),
		&Xcm::<TestCall>(vec![DepositAsset {
			assets: All.into(),
			beneficiary: Location::new(0, [AccountId32 { id: RECIPIENT, network: None }]),
		},])
	);

	assert!(vm.bench_post_process(weight).ensure_complete().is_ok());
}

// ===== Unhappy path =====

// Processing still can't be called recursively more than the limit.
#[test]
fn recursion_exceeds_limit() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER_1, (Here, 10u128));
	add_asset(SENDER_2, (Here, 100u128));

	let mut xcm = Xcm::<TestCall>::builder_unsafe()
		.execute_with_origin(None, Xcm::<TestCall>::builder_unsafe().clear_origin().build())
		.build();

	// 10 is the RECURSION_LIMIT.
	for _ in 0..10 {
		let clone_of_xcm = xcm.clone();
		if let ExecuteWithOrigin { xcm: ref mut inner, .. } = xcm.inner_mut()[0] {
			*inner = clone_of_xcm;
		}
	}

	let (mut vm, weight) = instantiate_executor(Here, xcm.clone());

	// Program errors with `ExceedsStackLimit`.
	assert_eq!(
		vm.bench_process(xcm),
		Err(ExecutorError {
			index: 0,
			xcm_error: XcmError::ExceedsStackLimit,
			weight: Weight::zero(),
		})
	);
	assert!(vm.bench_post_process(weight).ensure_complete().is_ok());
}
