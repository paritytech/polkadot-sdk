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

//! Unit tests for refunding surplus weight, through `RefundSurplus` and at the end of a message.

use xcm::prelude::*;

use super::mock::*;
use crate::XcmExecutor;

const SENDER: [u8; 32] = [0; 32];
const RECIPIENT: [u8; 32] = [1; 32];

/// Runs a message that pays `10` of `Here` into the `fees` register. The message weighs `(2, 2)`
/// and costs `4`, so `6` is left in `fees` and holding is empty.
fn executor_with_leftover_fees() -> XcmExecutor<XcmConfig> {
	add_asset(SENDER, (Here, 100u128));
	let xcm = Xcm::<TestCall>::builder()
		.withdraw_asset((Here, 100u128))
		.pay_fees((Here, 10u128))
		.deposit_asset(All, RECIPIENT)
		.build();
	let (mut vm, _) = instantiate_executor(SENDER, xcm.clone());
	assert!(vm.bench_process(xcm).is_ok());
	assert_eq!(get_first_fungible(vm.holding()), None);
	assert_eq!(get_first_fungible(vm.fees()).unwrap(), (Here, 6u128).into());
	vm
}

#[test]
fn implicit_refund_is_charged_the_refund_surplus_weight() {
	let mut vm = executor_with_leftover_fees();
	// Twice the `(2, 2)` a `RefundSurplus` weighs.
	vm.set_total_surplus(Weight::from_parts(4, 4));

	let outcome = vm.bench_post_process(Weight::from_parts(10, 10));
	assert!(matches!(outcome, Outcome::Complete { .. }));
	let used = outcome.weight_used();

	// The refund work is charged, so `(2, 2)` of the surplus is refunded: `4` of `Here` on top of
	// the `6` leftover fees, all trapped.
	assert_eq!(used, Weight::from_parts(8, 8));
	assert_eq!(asset_list(TRAPPED_ASSETS), [(Here, 10u128).into()]);
}

#[test]
fn implicit_refund_is_skipped_when_the_surplus_does_not_cover_it() {
	let mut vm = executor_with_leftover_fees();
	vm.set_total_surplus(Weight::from_parts(1, 1));

	let outcome = vm.bench_post_process(Weight::from_parts(10, 10));
	assert!(matches!(outcome, Outcome::Complete { .. }));
	let used = outcome.weight_used();

	// Nothing is refunded and nothing is charged.
	assert_eq!(used, Weight::from_parts(9, 9));
	assert_eq!(asset_list(TRAPPED_ASSETS), [(Here, 6u128).into()]);
}

#[test]
fn implicit_refund_is_skipped_when_the_surplus_equals_its_weight() {
	let mut vm = executor_with_leftover_fees();
	vm.set_total_surplus(Weight::from_parts(2, 2));

	let outcome = vm.bench_post_process(Weight::from_parts(10, 10));
	assert!(matches!(outcome, Outcome::Complete { .. }));
	let used = outcome.weight_used();

	assert_eq!(used, Weight::from_parts(8, 8));
	assert_eq!(asset_list(TRAPPED_ASSETS), [(Here, 6u128).into()]);
}

#[test]
fn refund_lands_in_holding_when_it_fits() {
	let mut vm = executor_with_leftover_fees();
	vm.set_total_surplus(Weight::from_parts(2, 2));

	assert!(vm.bench_process(Xcm(vec![RefundSurplus])).is_ok());

	assert_eq!(vm.total_refunded(), &Weight::from_parts(2, 2));
	// The refund of `4` and the leftover fees of `6` are both in holding.
	assert_eq!(get_first_fungible(vm.holding()).unwrap(), (Here, 10u128).into());
	assert_eq!(get_first_fungible(vm.fees()), None);
	assert_eq!(asset_list(TRAPPED_ASSETS), []);
}

#[test]
fn refund_holding_cannot_take_is_trapped_and_fails() {
	let mut vm = executor_with_leftover_fees();
	vm.set_total_surplus(Weight::from_parts(2, 2));
	// Holding is empty and its limit zero, so a refund in `Here` cannot be added to it.
	vm.set_holding_limit(0);

	let error = vm.bench_process(Xcm(vec![RefundSurplus, RefundSurplus])).unwrap_err();

	// The first `RefundSurplus` fails, so the second never runs and only one refund is trapped.
	assert_eq!(error.index, 0);
	assert_eq!(error.xcm_error, XcmError::HoldingWouldOverflow);
	assert_eq!(vm.total_refunded(), &Weight::from_parts(2, 2));
	// The refund of `4` and the leftover fees of `6` are trapped together; holding stays empty.
	assert_eq!(asset_list(TRAPPED_ASSETS), [(Here, 10u128).into()]);
	assert_eq!(get_first_fungible(vm.holding()), None);
	assert_eq!(get_first_fungible(vm.fees()), None);
}

#[test]
fn implicit_refund_holding_cannot_take_is_trapped() {
	let mut vm = executor_with_leftover_fees();
	vm.set_total_surplus(Weight::from_parts(4, 4));
	vm.set_holding_limit(0);

	let outcome = vm.bench_post_process(Weight::from_parts(10, 10));
	assert!(matches!(outcome, Outcome::Complete { .. }));

	assert_eq!(outcome.weight_used(), Weight::from_parts(8, 8));
	assert_eq!(asset_list(TRAPPED_ASSETS), [(Here, 10u128).into()]);
}
