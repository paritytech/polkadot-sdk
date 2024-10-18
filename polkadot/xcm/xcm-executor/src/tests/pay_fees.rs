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

use xcm::prelude::*;

use super::mock::*;

// The sender and recipient we use across these tests.
const SENDER: [u8; 32] = [0; 32];
const RECIPIENT: [u8; 32] = [1; 32];

// ===== Happy path =====

// This is a sort of backwards compatibility test.
// Since `PayFees` is a replacement for `BuyExecution`, we need to make sure it at least
// manages to do the same thing, paying for execution fees.
#[test]
fn works_for_execution_fees() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER, (Here, 100u128));

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder()
		.withdraw_asset((Here, 100u128))
		.pay_fees((Here, 10u128)) // 10% destined for fees, not more.
		.deposit_asset(All, RECIPIENT)
		.build();

	let (mut vm, weight) = instantiate_executor(SENDER, xcm.clone());

	// Program runs successfully.
	assert!(vm.bench_process(xcm).is_ok());

	// Nothing is left in the `holding` register.
	assert_eq!(get_first_fungible(vm.holding()), None);
	// Execution fees were 4, so we still have 6 left in the `fees` register.
	assert_eq!(get_first_fungible(vm.fees()).unwrap(), (Here, 6u128).into());

	// The recipient received all the assets in the holding register, so `100` that
	// were withdrawn, minus the `10` that were destinated for fee payment.
	assert_eq!(asset_list(RECIPIENT), [(Here, 90u128).into()]);

	// Leftover fees get trapped.
	assert!(vm.bench_post_process(weight).ensure_complete().is_ok());
	assert_eq!(asset_list(TRAPPED_ASSETS), [(Here, 6u128).into()])
}

// This tests the new functionality provided by `PayFees`, being able to pay for
// delivery fees from the `fees` register.
#[test]
fn works_for_delivery_fees() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER, (Here, 100u128));

	// Information to send messages.
	// We don't care about the specifics since we're not actually sending them.
	let query_response_info =
		QueryResponseInfo { destination: Parent.into(), query_id: 0, max_weight: Weight::zero() };

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder()
		.withdraw_asset((Here, 100u128))
		.pay_fees((Here, 10u128))
		// Send a bunch of messages, each charging delivery fees.
		.report_error(query_response_info.clone())
		.report_error(query_response_info.clone())
		.report_error(query_response_info)
		.deposit_asset(All, RECIPIENT)
		.build();

	let (mut vm, _) = instantiate_executor(SENDER, xcm.clone());

	// Program runs successfully.
	assert!(vm.bench_process(xcm).is_ok());

	// Nothing is left in the `holding` register.
	assert_eq!(get_first_fungible(vm.holding()), None);
	// Execution fees were 4, delivery were 3, so we are left with only 3 in the `fees` register.
	assert_eq!(get_first_fungible(vm.fees()).unwrap(), (Here, 3u128).into());

	// The recipient received all the assets in the holding register, so `100` that
	// were withdrawn, minus the `10` that were destinated for fee payment.
	assert_eq!(asset_list(RECIPIENT), [(Here, 90u128).into()]);

	let querier: Location = (
		UniversalLocation::get().take_first().unwrap(),
		AccountId32 { id: SENDER.into(), network: None },
	)
		.into();
	let sent_message = Xcm(vec![QueryResponse {
		query_id: 0,
		response: Response::ExecutionResult(None),
		max_weight: Weight::zero(),
		querier: Some(querier),
	}]);

	// The messages were "sent" successfully.
	assert_eq!(
		sent_xcm(),
		vec![
			(Parent.into(), sent_message.clone()),
			(Parent.into(), sent_message.clone()),
			(Parent.into(), sent_message.clone())
		]
	);
}

// Tests the support for `BuyExecution` while the ecosystem transitions to `PayFees`.
#[test]
fn buy_execution_works_as_before() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER, (Here, 100u128));

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder()
		.withdraw_asset((Here, 100u128))
		// We can put everything here, since excess will be returned to holding.
		// We have to specify `Limited` here to actually work, it's normally
		// set in the `AllowTopLevelPaidExecutionFrom` barrier.
		.buy_execution((Here, 100u128), Limited(Weight::from_parts(2, 2)))
		.deposit_asset(All, RECIPIENT)
		.build();

	let (mut vm, _) = instantiate_executor(SENDER, xcm.clone());

	// Program runs successfully.
	assert!(vm.bench_process(xcm).is_ok());

	// Nothing is left in the `holding` register.
	assert_eq!(get_first_fungible(vm.holding()), None);
	// `BuyExecution` does not interact with the `fees` register.
	assert_eq!(get_first_fungible(vm.fees()), None);

	// The recipient received all the assets in the holding register, so `100` that
	// were withdrawn, minus the `4` from paying the execution fees.
	assert_eq!(asset_list(RECIPIENT), [(Here, 96u128).into()]);
}

// Tests the interaction between `PayFees` and `RefundSurplus`.
#[test]
fn fees_can_be_refunded() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER, (Here, 100u128));

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder()
		.withdraw_asset((Here, 100u128))
		.pay_fees((Here, 10u128)) // 10% destined for fees, not more.
		.deposit_asset(All, RECIPIENT)
		.refund_surplus()
		.deposit_asset(All, SENDER)
		.build();

	let (mut vm, _) = instantiate_executor(SENDER, xcm.clone());

	// Program runs successfully.
	assert!(vm.bench_process(xcm).is_ok());

	// Nothing is left in the `holding` register.
	assert_eq!(get_first_fungible(vm.holding()), None);
	// Nothing was left in the `fees` register since it was refunded.
	assert_eq!(get_first_fungible(vm.fees()), None);

	// The recipient received all the assets in the holding register, so `100` that
	// were withdrawn, minus the `10` that were destinated for fee payment.
	assert_eq!(asset_list(RECIPIENT), [(Here, 90u128).into()]);

	// The sender got back `6` from unused assets.
	assert_eq!(asset_list(SENDER), [(Here, 6u128).into()]);
}

// ===== Unhappy path =====

#[test]
fn putting_all_assets_in_pay_fees() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER, (Here, 100u128));

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder()
		.withdraw_asset((Here, 100u128))
		.pay_fees((Here, 100u128)) // 100% destined for fees, this is not going to end well...
		.deposit_asset(All, RECIPIENT)
		.build();

	let (mut vm, _) = instantiate_executor(SENDER, xcm.clone());

	// Program runs successfully.
	assert!(vm.bench_process(xcm).is_ok());

	// Nothing is left in the `holding` register.
	assert_eq!(get_first_fungible(vm.holding()), None);
	// We destined `100` for fee payment, after `4` for execution fees, we are left with `96`.
	assert_eq!(get_first_fungible(vm.fees()).unwrap(), (Here, 96u128).into());

	// The recipient received no assets since they were all destined for fee payment.
	assert_eq!(asset_list(RECIPIENT), []);
}

#[test]
fn refunding_too_early() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER, (Here, 100u128));

	// Information to send messages.
	// We don't care about the specifics since we're not actually sending them.
	let query_response_info =
		QueryResponseInfo { destination: Parent.into(), query_id: 0, max_weight: Weight::zero() };

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder()
		.withdraw_asset((Here, 100u128))
		.pay_fees((Here, 10u128)) // 10% destined for fees, not more.
		.deposit_asset(All, RECIPIENT)
		.refund_surplus()
		.deposit_asset(All, SENDER)
		// `refund_surplus` cleared the `fees` register.
		// `holding` is used as a fallback, but we also cleared that.
		// The instruction will error and the message won't be sent :(.
		.report_error(query_response_info)
		.build();

	let (mut vm, _) = instantiate_executor(SENDER, xcm.clone());

	// Program fails to run.
	assert!(vm.bench_process(xcm).is_err());

	// Nothing is left in the `holding` register.
	assert_eq!(get_first_fungible(vm.holding()), None);
	// Nothing was left in the `fees` register since it was refunded.
	assert_eq!(get_first_fungible(vm.fees()), None);

	// The recipient received all the assets in the holding register, so `100` that
	// were withdrawn, minus the `10` that were destinated for fee payment.
	assert_eq!(asset_list(RECIPIENT), [(Here, 90u128).into()]);

	// The sender got back `6` from unused assets.
	assert_eq!(asset_list(SENDER), [(Here, 6u128).into()]);

	// No messages were "sent".
	assert_eq!(sent_xcm(), Vec::new());
}
