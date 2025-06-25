// This file is part of Substrate.

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

//! bounties pallet tests.

#![cfg(test)]

use super::{Event as BountiesEvent, *};
use crate as pallet_bounties;
use crate::mock::{Bounties, *};

use frame_support::{assert_err_ignore_postinfo, assert_noop, assert_ok, traits::Currency};
use sp_runtime::traits::Dispatchable;

type UtilityCall = pallet_utility::Call<Test>;
type BountiesCall = crate::Call<Test>;

#[test]
fn fund_bounty_works() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let value = 50;

		// When
		assert_ok!(Bounties::fund_bounty(
			RuntimeOrigin::root(),
			Box::new(asset_kind),
			value,
			b"1234567890".to_vec()
		));

		// Then
		let bounty_id = 0;
		let payment_id = get_payment_id(bounty_id, None).expect("no payment attempt");
		expect_events(vec![
			BountiesEvent::Paid { index: bounty_id, payment_id },
			BountiesEvent::BountyFunded { index: bounty_id },
		]);
		assert_eq!(
			pallet_bounties::Bounties::<Test>::get(bounty_id).unwrap(),
			Bounty {
				fee: 0,
				curator_deposit: 0,
				asset_kind,
				value,
				status: BountyStatus::FundingAttempted {
					payment_status: PaymentState::Attempted { id: payment_id }
				},
			}
		);
		assert_eq!(pallet_bounties::BountyCount::<Test>::get(), 1);
		assert_eq!(
			pallet_bounties::BountyDescriptions::<Test>::get(bounty_id).unwrap(),
			b"1234567890".to_vec()
		);
	});
}

#[test]
fn fund_bounty_fails() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;

		// When/Then
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::signed(0),
				Box::new(asset_kind),
				50,
				b"1234567890".to_vec()
			),
			BadOrigin
		);

		// When/Then
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::root(),
				Box::new(asset_kind),
				0,
				b"1234567890".to_vec()
			),
			Error::<Test>::InvalidValue
		);

		// When/Then
		assert_noop!(
			Bounties::fund_bounty(
				RuntimeOrigin::signed(10), // max spending of 10
				Box::new(asset_kind),
				11,
				b"1234567890".to_vec()
			),
			Error::<Test>::InsufficientPermission
		);
	});
}

#[test]
fn fund_bounty_in_batch_respects_max_total() {
	ExtBuilder::default().build_and_execute(|| {
		// Given
		let asset_kind = 1;
		let spend_origin = 10; // max spending of 10
		let value = 1; // `native_amount` is 1

		// When/Then
		// Respect the `max_total` for the given origin.
		assert_ok!(RuntimeCall::from(UtilityCall::batch_all {
			calls: vec![
				RuntimeCall::from(BountiesCall::fund_bounty {
					asset_kind: Box::new(asset_kind),
					value,
					description: b"1234567890".to_vec()
				}),
				RuntimeCall::from(BountiesCall::fund_bounty {
					asset_kind: Box::new(asset_kind),
					value,
					description: b"1234567890".to_vec()
				})
			]
		})
		.dispatch(RuntimeOrigin::signed(spend_origin)));

		// Given
		let value = 5; // `native_amount` is 5

		// When/Then
		// `spend` of 10 surpasses `max_total` for the given origin.
		assert_err_ignore_postinfo!(
			RuntimeCall::from(UtilityCall::batch_all {
				calls: vec![
					RuntimeCall::from(BountiesCall::fund_bounty {
						asset_kind: Box::new(asset_kind),
						value,
						description: b"1234567890".to_vec()
					}),
					RuntimeCall::from(BountiesCall::fund_bounty {
						asset_kind: Box::new(asset_kind),
						value,
						description: b"1234567890".to_vec()
					})
				]
			})
			.dispatch(RuntimeOrigin::signed(spend_origin)),
			Error::<Test>::InsufficientPermission
		);
	});
}
