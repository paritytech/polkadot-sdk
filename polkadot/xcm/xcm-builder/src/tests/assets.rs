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

use super::*;

#[test]
fn exchange_asset_should_work() {
	AllowUnpaidFrom::set(vec![Parent.into()]);
	add_asset(Parent, (Parent, 1000u128));
	set_exchange_assets(vec![(Here, 100u128).into()]);
	let message = Xcm(vec![
		WithdrawAsset((Parent, 100u128).into()),
		SetAppendix(
			vec![DepositAsset { assets: AllCounted(2).into(), beneficiary: Parent.into() }].into(),
		),
		ExchangeAsset {
			give: Definite((Parent, 50u128).into()),
			want: (Here, 50u128).into(),
			maximal: true,
		},
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parent,
		message,
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(40, 40) });
	assert_eq!(asset_list(Parent), vec![(Here, 100u128).into(), (Parent, 950u128).into()]);
	assert_eq!(exchange_assets(), vec![(Parent, 50u128).into()].into());
}

#[test]
fn exchange_asset_without_maximal_should_work() {
	AllowUnpaidFrom::set(vec![Parent.into()]);
	add_asset(Parent, (Parent, 1000u128));
	set_exchange_assets(vec![(Here, 100u128).into()]);
	let message = Xcm(vec![
		WithdrawAsset((Parent, 100u128).into()),
		SetAppendix(
			vec![DepositAsset { assets: AllCounted(2).into(), beneficiary: Parent.into() }].into(),
		),
		ExchangeAsset {
			give: Definite((Parent, 50).into()),
			want: (Here, 50u128).into(),
			maximal: false,
		},
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parent,
		message,
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(40, 40) });
	assert_eq!(asset_list(Parent), vec![(Here, 50u128).into(), (Parent, 950u128).into()]);
	assert_eq!(exchange_assets(), vec![(Here, 50u128).into(), (Parent, 50u128).into()].into());
}

#[test]
fn exchange_asset_should_fail_when_no_deal_possible() {
	AllowUnpaidFrom::set(vec![Parent.into()]);
	add_asset(Parent, (Parent, 1000u128));
	set_exchange_assets(vec![(Here, 100u128).into()]);
	let message = Xcm(vec![
		WithdrawAsset((Parent, 150u128).into()),
		SetAppendix(
			vec![DepositAsset { assets: AllCounted(2).into(), beneficiary: Parent.into() }].into(),
		),
		ExchangeAsset {
			give: Definite((Parent, 150u128).into()),
			want: (Here, 150u128).into(),
			maximal: false,
		},
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parent,
		message,
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(40, 40), error: XcmError::NoDeal }
	);
	assert_eq!(asset_list(Parent), vec![(Parent, 1000u128).into()]);
	assert_eq!(exchange_assets(), vec![(Here, 100u128).into()].into());
}

#[test]
fn paying_reserve_deposit_should_work() {
	AllowPaidFrom::set(vec![Parent.into()]);
	add_reserve(Parent.into(), (Parent, WildFungible).into());
	WeightPrice::set((Parent.into(), 1_000_000_000_000, 1024 * 1024));

	let fees = (Parent, 60u128).into();
	let message = Xcm(vec![
		ReserveAssetDeposited((Parent, 100u128).into()),
		BuyExecution { fees, weight_limit: Limited(Weight::from_parts(30, 30)) },
		DepositAsset { assets: AllCounted(1).into(), beneficiary: Here.into() },
	]);
	let mut hash = fake_message_hash(&message);
	let weight_limit = Weight::from_parts(50, 50);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parent,
		message,
		&mut hash,
		weight_limit,
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(30, 30) });
	assert_eq!(asset_list(Here), vec![(Parent, 40u128).into()]);
}

#[test]
fn transfer_should_work() {
	// we'll let them have message execution for free.
	AllowUnpaidFrom::set(vec![[Parachain(1)].into()]);
	// Child parachain #1 owns 1000 tokens held by us in reserve.
	add_asset(Parachain(1), (Here, 1000));
	// They want to transfer 100 of them to their sibling parachain #2
	let message = Xcm(vec![TransferAsset {
		assets: (Here, 100u128).into(),
		beneficiary: [AccountIndex64 { index: 3, network: None }].into(),
	}]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(10, 10) });
	assert_eq!(
		asset_list(AccountIndex64 { index: 3, network: None }),
		vec![(Here, 100u128).into()]
	);
	assert_eq!(asset_list(Parachain(1)), vec![(Here, 900u128).into()]);
	assert_eq!(sent_xcm(), vec![]);
}

#[test]
fn reserve_transfer_should_work() {
	AllowUnpaidFrom::set(vec![[Parachain(1)].into()]);
	// Child parachain #1 owns 1000 tokens held by us in reserve.
	add_asset(Parachain(1), (Here, 1000));
	// The remote account owned by gav.
	let three: Location = [AccountIndex64 { index: 3, network: None }].into();

	// They want to transfer 100 of our native asset from sovereign account of parachain #1 into #2
	// and let them know to hand it to account #3.
	let message = Xcm(vec![TransferReserveAsset {
		assets: (Here, 100u128).into(),
		dest: Parachain(2).into(),
		xcm: Xcm::<()>(vec![DepositAsset {
			assets: AllCounted(1).into(),
			beneficiary: three.clone(),
		}]),
	}]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(10, 10) });

	let expected_msg = Xcm::<()>(vec![
		ReserveAssetDeposited((Parent, 100u128).into()),
		ClearOrigin,
		DepositAsset { assets: AllCounted(1).into(), beneficiary: three },
	]);
	let expected_hash = fake_message_hash(&expected_msg);
	assert_eq!(asset_list(Parachain(2)), vec![(Here, 100).into()]);
	assert_eq!(sent_xcm(), vec![(Parachain(2).into(), expected_msg, expected_hash)]);
}

#[test]
fn burn_should_work() {
	// we'll let them have message execution for free.
	AllowUnpaidFrom::set(vec![[Parachain(1)].into()]);
	// Child parachain #1 owns 1000 tokens held by us in reserve.
	add_asset(Parachain(1), (Here, 1000));
	// They want to burn 100 of them
	let message = Xcm(vec![
		WithdrawAsset((Here, 1000u128).into()),
		BurnAsset((Here, 100u128).into()),
		DepositAsset { assets: Wild(AllCounted(1)), beneficiary: Parachain(1).into() },
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(30, 30) });
	assert_eq!(asset_list(Parachain(1)), vec![(Here, 900u128).into()]);
	assert_eq!(sent_xcm(), vec![]);

	// Now they want to burn 1000 of them, which will actually only burn 900.
	let message = Xcm(vec![
		WithdrawAsset((Here, 900u128).into()),
		BurnAsset((Here, 1000u128).into()),
		DepositAsset { assets: Wild(AllCounted(1)), beneficiary: Parachain(1).into() },
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(30, 30) });
	assert_eq!(asset_list(Parachain(1)), vec![]);
	assert_eq!(sent_xcm(), vec![]);
}

#[test]
fn basic_asset_trap_should_work() {
	// we'll let them have message execution for free.
	AllowUnpaidFrom::set(vec![[Parachain(1)].into(), [Parachain(2)].into()]);

	// Child parachain #1 owns 1000 tokens held by us in reserve.
	add_asset(Parachain(1), (Here, 1000));
	// They want to transfer 100 of them to their sibling parachain #2 but have a problem
	let message = Xcm(vec![
		WithdrawAsset((Here, 100u128).into()),
		DepositAsset {
			assets: Wild(AllCounted(0)), // <<< 0 is an error.
			beneficiary: AccountIndex64 { index: 3, network: None }.into(),
		},
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(20, 20),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(25, 25) });
	assert_eq!(asset_list(Parachain(1)), vec![(Here, 900u128).into()]);
	assert_eq!(asset_list(AccountIndex64 { index: 3, network: None }), vec![]);

	// Incorrect ticket doesn't work.
	let message = Xcm(vec![
		ClaimAsset { assets: (Here, 100u128).into(), ticket: GeneralIndex(1).into() },
		DepositAsset {
			assets: Wild(AllCounted(1)),
			beneficiary: AccountIndex64 { index: 3, network: None }.into(),
		},
	]);
	let mut hash = fake_message_hash(&message);
	let old_trapped_assets = TrappedAssets::get();
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(20, 20),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(10, 10), error: XcmError::UnknownClaim }
	);
	assert_eq!(asset_list(Parachain(1)), vec![(Here, 900u128).into()]);
	assert_eq!(asset_list(AccountIndex64 { index: 3, network: None }), vec![]);
	assert_eq!(old_trapped_assets, TrappedAssets::get());

	// Incorrect origin doesn't work.
	let message = Xcm(vec![
		ClaimAsset { assets: (Here, 100u128).into(), ticket: GeneralIndex(0).into() },
		DepositAsset {
			assets: Wild(AllCounted(1)),
			beneficiary: AccountIndex64 { index: 3, network: None }.into(),
		},
	]);
	let mut hash = fake_message_hash(&message);
	let old_trapped_assets = TrappedAssets::get();
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(2),
		message,
		&mut hash,
		Weight::from_parts(20, 20),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(10, 10), error: XcmError::UnknownClaim }
	);
	assert_eq!(asset_list(Parachain(1)), vec![(Here, 900u128).into()]);
	assert_eq!(asset_list(AccountIndex64 { index: 3, network: None }), vec![]);
	assert_eq!(old_trapped_assets, TrappedAssets::get());

	// Incorrect assets doesn't work.
	let message = Xcm(vec![
		ClaimAsset { assets: (Here, 101u128).into(), ticket: GeneralIndex(0).into() },
		DepositAsset {
			assets: Wild(AllCounted(1)),
			beneficiary: AccountIndex64 { index: 3, network: None }.into(),
		},
	]);
	let mut hash = fake_message_hash(&message);
	let old_trapped_assets = TrappedAssets::get();
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(20, 20),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(10, 10), error: XcmError::UnknownClaim }
	);
	assert_eq!(asset_list(Parachain(1)), vec![(Here, 900u128).into()]);
	assert_eq!(asset_list(AccountIndex64 { index: 3, network: None }), vec![]);
	assert_eq!(old_trapped_assets, TrappedAssets::get());

	let message = Xcm(vec![
		ClaimAsset { assets: (Here, 100u128).into(), ticket: GeneralIndex(0).into() },
		DepositAsset {
			assets: Wild(AllCounted(1)),
			beneficiary: AccountIndex64 { index: 3, network: None }.into(),
		},
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(20, 20),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(20, 20) });
	assert_eq!(asset_list(Parachain(1)), vec![(Here, 900u128).into()]);
	assert_eq!(
		asset_list(AccountIndex64 { index: 3, network: None }),
		vec![(Here, 100u128).into()]
	);

	// Same again doesn't work :-)
	let message = Xcm(vec![
		ClaimAsset { assets: (Here, 100u128).into(), ticket: GeneralIndex(0).into() },
		DepositAsset {
			assets: Wild(AllCounted(1)),
			beneficiary: AccountIndex64 { index: 3, network: None }.into(),
		},
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(20, 20),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(10, 10), error: XcmError::UnknownClaim }
	);
}

#[test]
fn max_assets_limit_should_work() {
	// we'll let them have message execution for free.
	AllowUnpaidFrom::set(vec![[Parachain(1)].into()]);
	// Child parachain #1 owns 1000 tokens held by us in reserve.
	add_asset(Parachain(1), (Junctions::from([GeneralIndex(0)]), 1000u128));
	add_asset(Parachain(1), (Junctions::from([GeneralIndex(1)]), 1000u128));
	add_asset(Parachain(1), (Junctions::from([GeneralIndex(2)]), 1000u128));
	add_asset(Parachain(1), (Junctions::from([GeneralIndex(3)]), 1000u128));
	add_asset(Parachain(1), (Junctions::from([GeneralIndex(4)]), 1000u128));
	add_asset(Parachain(1), (Junctions::from([GeneralIndex(5)]), 1000u128));
	add_asset(Parachain(1), (Junctions::from([GeneralIndex(6)]), 1000u128));
	add_asset(Parachain(1), (Junctions::from([GeneralIndex(7)]), 1000u128));
	add_asset(Parachain(1), (Junctions::from([GeneralIndex(8)]), 1000u128));

	// Attempt to withdraw 8 (=2x4)different assets. This will succeed.
	let message = Xcm(vec![
		WithdrawAsset((Junctions::from([GeneralIndex(0)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(1)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(2)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(3)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(4)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(5)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(6)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(7)]), 100u128).into()),
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(100, 100),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(85, 85) });

	// Attempt to withdraw 9 different assets will fail.
	let message = Xcm(vec![
		WithdrawAsset((Junctions::from([GeneralIndex(0)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(1)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(2)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(3)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(4)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(5)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(6)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(7)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(8)]), 100u128).into()),
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(100, 100),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete {
			used: Weight::from_parts(95, 95),
			error: XcmError::HoldingWouldOverflow
		}
	);

	// Attempt to withdraw 4 different assets and then the same 4 and then a different 4 will
	// succeed.
	let message = Xcm(vec![
		WithdrawAsset((Junctions::from([GeneralIndex(0)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(1)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(2)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(3)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(0)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(1)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(2)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(3)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(4)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(5)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(6)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(7)]), 100u128).into()),
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(200, 200),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(125, 125) });

	// Attempt to withdraw 4 different assets and then a different 4 and then the same 4 will fail.
	let message = Xcm(vec![
		WithdrawAsset((Junctions::from([GeneralIndex(0)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(1)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(2)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(3)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(4)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(5)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(6)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(7)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(0)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(1)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(2)]), 100u128).into()),
		WithdrawAsset((Junctions::from([GeneralIndex(3)]), 100u128).into()),
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(200, 200),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete {
			used: Weight::from_parts(95, 95),
			error: XcmError::HoldingWouldOverflow
		}
	);

	// Attempt to withdraw 4 different assets and then a different 4 and then the same 4 will fail.
	let message = Xcm(vec![
		WithdrawAsset(Assets::from(vec![
			(Junctions::from([GeneralIndex(0)]), 100u128).into(),
			(Junctions::from([GeneralIndex(1)]), 100u128).into(),
			(Junctions::from([GeneralIndex(2)]), 100u128).into(),
			(Junctions::from([GeneralIndex(3)]), 100u128).into(),
			(Junctions::from([GeneralIndex(4)]), 100u128).into(),
			(Junctions::from([GeneralIndex(5)]), 100u128).into(),
			(Junctions::from([GeneralIndex(6)]), 100u128).into(),
			(Junctions::from([GeneralIndex(7)]), 100u128).into(),
		])),
		WithdrawAsset((Junctions::from([GeneralIndex(0)]), 100u128).into()),
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(200, 200),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete {
			used: Weight::from_parts(25, 25),
			error: XcmError::HoldingWouldOverflow
		}
	);
}
