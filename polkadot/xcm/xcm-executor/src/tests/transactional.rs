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

//! Unit tests for `transactional_process` and `transactional_process_with_custom_rollback`.

use xcm::prelude::*;

use super::mock::*;
use crate::{ExecutorError, XcmExecutor};

const SENDER: [u8; 32] = [0; 32];
const RECIPIENT: [u8; 32] = [1; 32];

/// On success, `transactional_process` does not roll back holding or fees.
#[test]
fn transactional_process_success_no_rollback() {
	add_asset(SENDER, (Here, 100u128));

	// Withdraw into holding, pay fees, then deposit — all should succeed.
	let xcm = Xcm::<TestCall>::builder()
		.withdraw_asset((Here, 100u128))
		.pay_fees((Here, 10u128))
		.deposit_asset(All, RECIPIENT)
		.build();

	let (mut vm, _weight) = instantiate_executor(SENDER, xcm.clone());
	assert!(vm.bench_process(xcm).is_ok());

	// Holding is empty (deposited to recipient).
	assert_eq!(get_first_fungible(vm.holding()), None);
	// Fees register has unspent fees.
	assert!(get_first_fungible(vm.fees()).is_some());
	// Recipient got their assets.
	assert_eq!(asset_list(RECIPIENT), [(Here, 90u128).into()]);
}

/// On error, `transactional_process` rolls back the holding register.
///
/// We trigger an error via `ExchangeAsset` which uses `transactional_process` internally.
/// The mock has no `AssetExchanger`, so the exchange always fails. The assets taken from
/// holding for the exchange should be restored on failure.
#[test]
fn transactional_process_error_rolls_back_holding() {
	add_asset(SENDER, (Here, 100u128));

	let xcm = Xcm::<TestCall>(vec![
		WithdrawAsset((Here, 100u128).into()),
		// ExchangeAsset takes from holding and tries to exchange — fails because no
		// AssetExchanger is configured, which triggers rollback.
		ExchangeAsset {
			give: Wild(All),
			want: Assets::from(vec![(Parent, 50u128).into()]),
			maximal: true,
		},
	]);

	let (mut vm, _weight) = instantiate_executor(SENDER, xcm.clone());
	assert!(vm.bench_process(xcm).is_err());

	// Holding should be restored after the failed ExchangeAsset.
	assert_eq!(get_first_fungible(vm.holding()), Some((Here, 100u128).into()));
}

/// On error, `transactional_process_with_custom_rollback` rolls back holding, fees, AND
/// invokes the custom rollback handler.
///
/// `PayFees` uses `transactional_process_with_custom_rollback` with a custom handler that
/// resets `already_paid_fees`. We verify this by running a failing `PayFees` first, then
/// running a second program with a valid `PayFees` on the same executor — if the custom
/// rollback worked, `already_paid_fees` was reset and the second `PayFees` actually
/// processes (populating the `fees` register). If it were stuck as `true`, the second
/// `PayFees` would be a no-op, leaving `fees` empty.
#[test]
fn custom_rollback_is_invoked_on_error() {
	add_asset(SENDER, (Here, 100u128));

	// First program: withdraw, then PayFees with an asset NOT in holding → fails.
	let xcm1 = Xcm::<TestCall>(vec![
		WithdrawAsset((Here, 100u128).into()),
		PayFees { asset: (Parent, 10u128).into() },
	]);

	let (mut vm, _weight) = instantiate_executor(SENDER, xcm1.clone());
	// PayFees fails because (Parent, 10) is not in holding.
	assert!(vm.bench_process(xcm1).is_err());

	// The custom rollback should have reset `already_paid_fees` to false.
	// Verify by running a second program: if the flag was properly rolled back,
	// PayFees will buy weight and populate the `fees` register.
	let xcm2 = Xcm::<TestCall>(vec![PayFees { asset: (Here, 10u128).into() }]);

	assert!(vm.bench_process(xcm2).is_ok());

	// If `already_paid_fees` was stuck as `true`, PayFees would have been a no-op and
	// the fees register would be empty. The custom rollback ensures it was reset.
	assert!(get_first_fungible(vm.fees()).is_some());
}

/// On error, `TransferReserveAsset` rolls back holding and fees, including the delivery fee
/// it already took from the fees register.
#[test]
fn transfer_reserve_asset_error_rolls_back_holding_and_fees() {
	add_asset(SENDER, (Here, 100u128));

	let setup = Xcm::<TestCall>(vec![
		WithdrawAsset((Here, 20u128).into()),
		PayFees { asset: (Here, 10u128).into() },
	]);
	let (mut vm, _weight) = instantiate_executor(SENDER, setup.clone());
	assert!(vm.bench_process(setup).is_ok());
	let holding_before = get_first_fungible(vm.holding());
	let fees_before = get_first_fungible(vm.fees());

	// Delivery fails after the delivery fee was taken from the fees register.
	let transfer = Xcm::<TestCall>(vec![TransferReserveAsset {
		assets: (Here, 5u128).into(),
		dest: undeliverable_destination(),
		xcm: Xcm(vec![]),
	}]);
	assert!(vm.bench_process(transfer).is_err());

	assert_eq!(get_first_fungible(vm.holding()), holding_before);
	assert_eq!(get_first_fungible(vm.fees()), fees_before);
}

/// A sender with `20` native in holding and the unspent part of a `PayFees { 10 }` in the fees
/// register.
fn vm_with_holding_and_fees() -> XcmExecutor<XcmConfig> {
	add_asset(SENDER, (Here, 100u128));
	let setup = Xcm::<TestCall>(vec![
		WithdrawAsset((Here, 20u128).into()),
		PayFees { asset: (Here, 10u128).into() },
	]);
	let (mut vm, _weight) = instantiate_executor(SENDER, setup.clone());
	assert!(vm.bench_process(setup).is_ok());
	vm
}

fn response_info(destination: Location) -> xcm::latest::QueryResponseInfo {
	xcm::latest::QueryResponseInfo { destination, query_id: 0, max_weight: Weight::zero() }
}

/// List of instructions that send a message through `send`.
fn sending_instructions(destination: Location) -> Vec<(&'static str, Instruction<TestCall>)> {
	vec![
		("ReportError", ReportError(response_info(destination.clone()))),
		(
			"ReportHolding",
			ReportHolding { response_info: response_info(destination.clone()), assets: All.into() },
		),
		("ReportTransactStatus", ReportTransactStatus(response_info(destination.clone()))),
		(
			"QueryPallet",
			QueryPallet {
				module_name: b"pallet".to_vec(),
				response_info: response_info(destination),
			},
		),
	]
}

/// Assumption behind the rollback tests below: each sending instruction charges the delivery
/// fee from the fees register and delivers when the destination is reachable.
#[test]
fn sending_instructions_charge_delivery_fee_from_fees_register() {
	for (name, instruction) in sending_instructions(Parent.into()) {
		let mut vm = vm_with_holding_and_fees();
		let holding_before = get_first_fungible(vm.holding());
		let fees_before = get_first_fungible(vm.fees()).unwrap();
		let sent_before = sent_xcm().len();

		assert!(vm.bench_process(Xcm(vec![instruction])).is_ok(), "{name}");

		assert_eq!(get_first_fungible(vm.holding()), holding_before, "{name}");
		// `PayFees { 10 }` bought the message weight for `4`; the test sender then charges `1`.
		assert_eq!(fees_before, (Here, 6u128).into(), "{name}");
		assert_eq!(get_first_fungible(vm.fees()), Some((Here, 5u128).into()), "{name}");
		assert_eq!(sent_xcm().len(), sent_before + 1, "{name}");
	}
}

/// When delivery fails, the delivery fee already taken from the fees register must be restored,
/// exactly as for the asset-transfer instructions.
#[test]
fn sending_instructions_delivery_failure_rolls_back_holding_and_fees() {
	for (name, instruction) in sending_instructions(undeliverable_destination()) {
		let mut vm = vm_with_holding_and_fees();
		let holding_before = get_first_fungible(vm.holding());
		let fees_before = get_first_fungible(vm.fees());
		let sent_before = sent_xcm().len();

		let result = vm.bench_process(Xcm(vec![instruction]));
		assert!(
			matches!(result, Err(ExecutorError { xcm_error: XcmError::Transport(_), .. })),
			"{name}: {result:?}"
		);

		assert_eq!(get_first_fungible(vm.holding()), holding_before, "{name}");
		assert_eq!(get_first_fungible(vm.fees()), fees_before, "{name}");
		assert_eq!(sent_xcm().len(), sent_before, "{name}");
	}
}

/// Failed `BuyExecution` should not inform which asset to use for delivery fee payments.
#[test]
fn failed_buy_execution_does_not_record_fee_asset() {
	add_asset(SENDER, (Here, 100u128));
	let weight_limit = Limited(Weight::from_parts(2, 2));

	let fails = Xcm::<TestCall>(vec![
		WithdrawAsset((Here, 100u128).into()),
		// Not in holding.
		BuyExecution { fees: (Parent, 10u128).into(), weight_limit: weight_limit.clone() },
	]);
	let (mut vm, _weight) = instantiate_executor(SENDER, fails.clone());
	assert!(matches!(
		vm.bench_process(fails),
		Err(ExecutorError { xcm_error: XcmError::NotHoldingFees, .. })
	));
	assert_eq!(vm.asset_used_in_buy_execution(), &None);
	assert_eq!(get_first_fungible(vm.holding()), Some((Here, 100u128).into()));

	// A successful purchase records the asset ...
	let succeeds = Xcm::<TestCall>(vec![BuyExecution {
		fees: (Here, 10u128).into(),
		weight_limit: weight_limit.clone(),
	}]);
	assert!(vm.bench_process(succeeds).is_ok());
	assert_eq!(vm.asset_used_in_buy_execution(), &Some(Here.into()));

	// ... and a later failed one keeps the recorded asset of the successful one.
	let fails_again = Xcm::<TestCall>(vec![BuyExecution {
		fees: (Parent, 10u128).into(),
		weight_limit: weight_limit.clone(),
	}]);
	assert!(vm.bench_process(fails_again).is_err());
	assert_eq!(vm.asset_used_in_buy_execution(), &Some(Here.into()));
}

// ========== `ExportMessage` must roll back the export fee when delivery fails ==========

fn export_message(destination: InteriorLocation) -> Instruction<TestCall> {
	ExportMessage { network: ByGenesis([1; 32]), destination, xcm: Xcm(vec![]) }
}

/// Assumption behind the rollback test below: `ExportMessage` charges the export fee from the
/// fees register and delivers when the destination is reachable.
#[test]
fn export_message_charges_export_fee_from_fees_register() {
	let mut vm = vm_with_holding_and_fees();
	let holding_before = get_first_fungible(vm.holding());
	let exported_before = exported_xcm().len();

	assert!(vm.bench_process(Xcm(vec![export_message([Parachain(1)].into())])).is_ok());

	assert_eq!(get_first_fungible(vm.holding()), holding_before);
	// `PayFees { 10 }` bought the message weight for `4`; the test exporter then charges `1`.
	assert_eq!(get_first_fungible(vm.fees()), Some((Here, 5u128).into()));
	assert_eq!(exported_xcm().len(), exported_before + 1);
}

/// A failing `deliver` must surface as the instruction's error and roll back the export fee,
/// rather than being swallowed and charging for a message that was never exported.
#[test]
fn export_message_delivery_failure_rolls_back_fee() {
	let mut vm = vm_with_holding_and_fees();
	let holding_before = get_first_fungible(vm.holding());
	let fees_before = get_first_fungible(vm.fees());
	let exported_before = exported_xcm().len();

	let result = vm.bench_process(Xcm(vec![export_message(undeliverable_export_destination())]));
	assert!(
		matches!(result, Err(ExecutorError { xcm_error: XcmError::Transport(_), .. })),
		"{result:?}"
	);

	assert_eq!(get_first_fungible(vm.holding()), holding_before);
	assert_eq!(get_first_fungible(vm.fees()), fees_before);
	assert_eq!(exported_xcm().len(), exported_before);
}

/// A rejected claim leaves holding and fees untouched.
#[test]
fn rejected_claim_leaves_registers_untouched() {
	let mut vm = vm_with_holding_and_fees();
	let holding_before = get_first_fungible(vm.holding());
	let fees_before = get_first_fungible(vm.fees());

	// Nothing was trapped, so there is nothing to claim.
	let claim =
		Xcm::<TestCall>(vec![ClaimAsset { assets: (Here, 5u128).into(), ticket: Here.into() }]);
	assert!(matches!(
		vm.bench_process(claim),
		Err(ExecutorError { xcm_error: XcmError::UnknownClaim, .. })
	));

	assert_eq!(get_first_fungible(vm.holding()), holding_before);
	assert_eq!(get_first_fungible(vm.fees()), fees_before);
}
