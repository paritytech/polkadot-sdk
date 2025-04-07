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

use frame_support::assert_ok;
use xcm_executor::traits::WeightFee;

use super::*;

#[test]
fn fixed_rate_of_fungible_should_work() {
	parameter_types! {
		pub static WeightPrice: (AssetId, u128, u128) =
			(Here.into(), WEIGHT_REF_TIME_PER_SECOND.into(), WEIGHT_PROOF_SIZE_PER_MB.into());
	}

	type Trader = FixedRateOfFungible<WeightPrice, ()>;
	let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

	// Correctly computes the fee
	assert_ok!(
		Trader::weight_fee(&Weight::from_parts(10, 10), &Here.into(), Some(&ctx),),
		WeightFee::Desired(20),
	);

	assert_ok!(
		Trader::weight_fee(&Weight::from_parts(5, 5), &Here.into(), Some(&ctx),),
		WeightFee::Desired(10),
	);

	assert_ok!(
		Trader::weight_fee(&Weight::from_parts(5, 0), &Here.into(), Some(&ctx),),
		WeightFee::Desired(5),
	);

	// Won't accept unknown token
	assert_err!(
		Trader::weight_fee(&Weight::from_parts(10, 10), &Parachain(1).into(), Some(&ctx)),
		XcmError::FeesNotMet,
	);
}

#[test]
fn errors_should_return_unused_weight() {
	// we'll let them have message execution for free.
	AllowUnpaidFrom::set(vec![Here.into()]);
	// We own 1000 of our tokens.
	add_asset(Here, (Here, 11u128));
	let mut message = Xcm(vec![
		// First xfer results in an error on the last message only
		TransferAsset {
			assets: (Here, 1u128).into(),
			beneficiary: [AccountIndex64 { index: 3, network: None }].into(),
		},
		// Second xfer results in error third message and after
		TransferAsset {
			assets: (Here, 2u128).into(),
			beneficiary: [AccountIndex64 { index: 3, network: None }].into(),
		},
		// Third xfer results in error second message and after
		TransferAsset {
			assets: (Here, 4u128).into(),
			beneficiary: [AccountIndex64 { index: 3, network: None }].into(),
		},
	]);
	// Weight limit of 70 is needed.
	let limit = <TestConfig as Config>::Weigher::weight(&mut message).unwrap();
	assert_eq!(limit, Weight::from_parts(30, 30));

	let mut hash = fake_message_hash(&message);

	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Here,
		message.clone(),
		&mut hash,
		limit,
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(30, 30) });
	assert_eq!(asset_list(AccountIndex64 { index: 3, network: None }), vec![(Here, 7u128).into()]);
	assert_eq!(asset_list(Here), vec![(Here, 4u128).into()]);
	assert_eq!(sent_xcm(), vec![]);

	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Here,
		message.clone(),
		&mut hash,
		limit,
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(30, 30), error: XcmError::NotWithdrawable }
	);
	assert_eq!(asset_list(AccountIndex64 { index: 3, network: None }), vec![(Here, 10u128).into()]);
	assert_eq!(asset_list(Here), vec![(Here, 1u128).into()]);
	assert_eq!(sent_xcm(), vec![]);

	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Here,
		message.clone(),
		&mut hash,
		limit,
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(20, 20), error: XcmError::NotWithdrawable }
	);
	assert_eq!(asset_list(AccountIndex64 { index: 3, network: None }), vec![(Here, 11u128).into()]);
	assert_eq!(asset_list(Here), vec![]);
	assert_eq!(sent_xcm(), vec![]);

	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Here,
		message,
		&mut hash,
		limit,
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(10, 10), error: XcmError::NotWithdrawable }
	);
	assert_eq!(asset_list(AccountIndex64 { index: 3, network: None }), vec![(Here, 11u128).into()]);
	assert_eq!(asset_list(Here), vec![]);
	assert_eq!(sent_xcm(), vec![]);
}

#[test]
fn weight_bounds_should_respect_instructions_limit() {
	MaxInstructions::set(3);
	let mut message = Xcm(vec![ClearOrigin; 4]);
	// 4 instructions are too many.
	assert_eq!(<TestConfig as Config>::Weigher::weight(&mut message), Err(()));

	let mut message =
		Xcm(vec![SetErrorHandler(Xcm(vec![ClearOrigin])), SetAppendix(Xcm(vec![ClearOrigin]))]);
	// 4 instructions are too many, even when hidden within 2.
	assert_eq!(<TestConfig as Config>::Weigher::weight(&mut message), Err(()));

	let mut message =
		Xcm(vec![SetErrorHandler(Xcm(vec![SetErrorHandler(Xcm(vec![SetErrorHandler(Xcm(
			vec![ClearOrigin],
		))]))]))]);
	// 4 instructions are too many, even when it's just one that's 3 levels deep.
	assert_eq!(<TestConfig as Config>::Weigher::weight(&mut message), Err(()));

	let mut message =
		Xcm(vec![SetErrorHandler(Xcm(vec![SetErrorHandler(Xcm(vec![ClearOrigin]))]))]);
	// 3 instructions are OK.
	assert_eq!(
		<TestConfig as Config>::Weigher::weight(&mut message),
		Ok(Weight::from_parts(30, 30))
	);
}

#[test]
fn weight_trader_tuple_should_work() {
	let para_1: Location = Parachain(1).into();
	let para_2: Location = Parachain(2).into();

	parameter_types! {
		pub static HereWeightPrice: (AssetId, u128, u128) =
			(Here.into(), WEIGHT_REF_TIME_PER_SECOND.into(), WEIGHT_PROOF_SIZE_PER_MB.into());
		pub static Para1WeightPrice: (AssetId, u128, u128) =
			(Parachain(1).into(), (5 * WEIGHT_REF_TIME_PER_SECOND).into(), (5 * WEIGHT_PROOF_SIZE_PER_MB).into());
	}

	type Traders = (
		// trader one
		FixedRateOfFungible<HereWeightPrice, ()>,
		// trader two
		FixedRateOfFungible<Para1WeightPrice, ()>,
	);

	let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };

	// the first trader computes the weight fee
	assert_ok!(
		Traders::weight_fee(&Weight::from_parts(5, 5), &Here.into(), Some(&ctx)),
		WeightFee::Desired(10),
	);

	// the second trader computes the weight fee
	assert_ok!(
		Traders::weight_fee(&Weight::from_parts(5, 5), &para_1.into(), Some(&ctx)),
		WeightFee::Desired(50),
	);

	// unknown asset, all traders fail to compute the weight fee
	assert_err!(
		Traders::weight_fee(&Weight::from_parts(5, 5), &para_2.into(), Some(&ctx)),
		XcmError::TooExpensive,
	);
}
