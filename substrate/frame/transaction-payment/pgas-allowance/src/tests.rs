// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{ChargeFeeWithPgas, mock::*};

use frame_support::{assert_ok, weights::Weight};
use pallet_asset_conversion_tx_payment::Event as AssetTxPaymentEvent;
use pallet_balances::Call as BalancesCall;
use sp_runtime::traits::{DispatchTransaction, TransactionExtension, TxBaseImplication};

type Ext = ChargeFeeWithPgas<Runtime>;

fn new_ext() -> Ext {
	ChargeFeeWithPgas::from(0, None)
}

fn pgas_call() -> RuntimeCall {
	RuntimeCall::System(frame_system::Call::remark { remark: Default::default() })
}

fn non_pgas_call() -> RuntimeCall {
	RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest: BOB, value: 1 })
}

/// Expect an `AssetTxFeePaid` event for `who` paid in PGAS; returns the actual fee.
fn pgas_fee_paid_event(who: &AccountId) -> Option<Balance> {
	System::events().into_iter().find_map(|e| match e.event {
		RuntimeEvent::AssetTxPayment(AssetTxPaymentEvent::AssetTxFeePaid {
			who: w,
			actual_fee,
			asset_id,
			..
		}) if &w == who && asset_id == PGAS_ASSET_ID => Some(actual_fee),
		_ => None,
	})
}

/// Alice holds no native but enough PGAS. A filter-matching call with `asset_id=None` auto-routes
/// to PGAS; her native balance stays zero and the fee is burned from PGAS.
#[test]
fn none_asset_filter_match_routes_to_pgas() {
	let pgas_initial = 1_000;
	ExtBuilder::default()
		.with_pgas(vec![(ALICE, pgas_initial)])
		.build()
		.execute_with(|| {
			let call = pgas_call();
			let len = 10;
			let info = info_from_weight(Weight::from_parts(7, 0));

			let fee =
				pallet_transaction_payment::Pallet::<Runtime>::compute_fee(len as u32, &info, 0);
			assert!(fee > 0);

			assert_eq!(Balances::free_balance(ALICE), 0);
			assert_eq!(Assets::balance(PGAS_ASSET_ID, ALICE), pgas_initial);

			let (pre, _) = new_ext()
				.validate_and_prepare(Some(ALICE).into(), &call, &info, len, 0)
				.unwrap();
			assert_eq!(Assets::balance(PGAS_ASSET_ID, ALICE), pgas_initial - fee);

			assert_ok!(<Ext as TransactionExtension<RuntimeCall>>::post_dispatch_details(
				pre,
				&info,
				&default_post_info(),
				len,
				&Ok(()),
			));
			assert_eq!(Balances::free_balance(ALICE), 0);
			assert_eq!(Assets::balance(PGAS_ASSET_ID, ALICE), pgas_initial - fee);
			assert_eq!(pgas_fee_paid_event(&ALICE), Some(fee));
		});
}

/// When the user explicitly specifies PGAS as the fee asset, PGAS is used regardless of whether
/// the call passes `CallFilter`.
#[test]
fn explicit_pgas_routes_to_pgas_even_on_filter_miss() {
	let pgas_initial = 1_000;
	ExtBuilder::default()
		.with_pgas(vec![(CHARLIE, pgas_initial)])
		.with_native(vec![(CHARLIE, 10)])
		.build()
		.execute_with(|| {
			let call = non_pgas_call();
			let len = 10;
			let info = info_from_weight(Weight::from_parts(7, 0));

			let ext = ChargeFeeWithPgas::<Runtime>::from(0, Some(PGAS_ASSET_ID));
			let (_pre, _) =
				ext.validate_and_prepare(Some(CHARLIE).into(), &call, &info, len, 0).unwrap();

			assert_eq!(Balances::free_balance(CHARLIE), 10, "native untouched on explicit PGAS");
			assert!(Assets::balance(PGAS_ASSET_ID, CHARLIE) < pgas_initial);
		});
}

/// Bob holds native but no PGAS. A filter-matching call falls back to native because PGAS balance
/// is insufficient.
#[test]
fn falls_back_to_native_when_no_pgas() {
	let native_initial = 1_000;
	ExtBuilder::default()
		.with_native(vec![(BOB, native_initial)])
		.build()
		.execute_with(|| {
			let call = pgas_call();
			let len = 10;
			let info = info_from_weight(Weight::from_parts(7, 0));

			let fee =
				pallet_transaction_payment::Pallet::<Runtime>::compute_fee(len as u32, &info, 0);
			assert!(fee > 0);

			assert_eq!(Assets::balance(PGAS_ASSET_ID, BOB), 0);

			let (_pre, _) =
				new_ext().validate_and_prepare(Some(BOB).into(), &call, &info, len, 0).unwrap();

			assert_eq!(Balances::free_balance(BOB), native_initial - fee);
			assert_eq!(Assets::balance(PGAS_ASSET_ID, BOB), 0);
			assert_eq!(pgas_fee_paid_event(&BOB), None);
		});
}

/// Charlie holds both native and PGAS but dispatches a call the filter rejects. The extension
/// stays on the native path; PGAS stays untouched.
#[test]
fn filter_miss_uses_native_even_with_pgas() {
	let native_initial = 1_000;
	let pgas_initial = 1_000;
	ExtBuilder::default()
		.with_native(vec![(CHARLIE, native_initial), (BOB, 10)])
		.with_pgas(vec![(CHARLIE, pgas_initial)])
		.build()
		.execute_with(|| {
			let call = non_pgas_call();
			let len = 10;
			let info = info_from_weight(Weight::from_parts(7, 0));

			let fee =
				pallet_transaction_payment::Pallet::<Runtime>::compute_fee(len as u32, &info, 0);
			assert!(fee > 0);

			let (_pre, _) = new_ext()
				.validate_and_prepare(Some(CHARLIE).into(), &call, &info, len, 0)
				.unwrap();

			assert_eq!(Balances::free_balance(CHARLIE), native_initial - fee);
			assert_eq!(Assets::balance(PGAS_ASSET_ID, CHARLIE), pgas_initial);
		});
}

/// Unused weight must refund PGAS back to the payer.
#[test]
fn pgas_refund_on_unused_weight() {
	let pgas_initial = 1_000;
	ExtBuilder::default()
		.with_pgas(vec![(ALICE, pgas_initial)])
		.build()
		.execute_with(|| {
			let call = pgas_call();
			let len = 10;
			let claimed = Weight::from_parts(100, 0);
			let actual = Weight::from_parts(40, 0);
			let info = info_from_weight(claimed);

			let reserved =
				pallet_transaction_payment::Pallet::<Runtime>::compute_fee(len as u32, &info, 0);
			let actual_fee = pallet_transaction_payment::Pallet::<Runtime>::compute_actual_fee(
				len as u32,
				&info,
				&post_info_from_weight(actual),
				0,
			);
			assert!(reserved > actual_fee);

			let (pre, _) = new_ext()
				.validate_and_prepare(Some(ALICE).into(), &call, &info, len, 0)
				.unwrap();
			assert_eq!(Assets::balance(PGAS_ASSET_ID, ALICE), pgas_initial - reserved);

			assert_ok!(<Ext as TransactionExtension<RuntimeCall>>::post_dispatch_details(
				pre,
				&info,
				&post_info_from_weight(actual),
				len,
				&Ok(()),
			));
			assert_eq!(Assets::balance(PGAS_ASSET_ID, ALICE), pgas_initial - actual_fee);
		});
}

/// Unsigned origins skip the PGAS routing by default (no signer; `NoCharge` path in the inner
/// extension).
#[test]
fn unsigned_delegates_to_no_charge() {
	ExtBuilder::default().with_pgas(vec![(ALICE, 1_000)]).build().execute_with(|| {
		let call = pgas_call();
		let len = 10;
		let info = info_from_weight(Weight::from_parts(7, 0));

		let (_validity, _val, _origin) = <Ext as TransactionExtension<RuntimeCall>>::validate(
			&new_ext(),
			frame_system::RawOrigin::None.into(),
			&call,
			&info,
			len,
			(),
			&TxBaseImplication((0u8, &call)),
			sp_runtime::transaction_validity::TransactionSource::External,
		)
		.unwrap();
		assert_eq!(Assets::balance(PGAS_ASSET_ID, ALICE), 1_000);
	});
}

/// An extension built with `new_skip_pgas` never auto-routes to PGAS: native pays the fee even
/// with enough PGAS and a matching filter.
#[test]
fn skip_pgas_forces_native() {
	let native_initial = 1_000;
	let pgas_initial = 1_000;
	ExtBuilder::default()
		.with_native(vec![(ALICE, native_initial)])
		.with_pgas(vec![(ALICE, pgas_initial)])
		.build()
		.execute_with(|| {
			let call = pgas_call();
			let len = 10;
			let info = info_from_weight(Weight::from_parts(7, 0));

			let fee =
				pallet_transaction_payment::Pallet::<Runtime>::compute_fee(len as u32, &info, 0);
			assert!(fee > 0);

			let ext = ChargeFeeWithPgas::<Runtime>::new_skip_pgas(0, None);
			let (_pre, _) =
				ext.validate_and_prepare(Some(ALICE).into(), &call, &info, len, 0).unwrap();

			assert_eq!(Balances::free_balance(ALICE), native_initial - fee);
			assert_eq!(Assets::balance(PGAS_ASSET_ID, ALICE), pgas_initial);
		});
}
