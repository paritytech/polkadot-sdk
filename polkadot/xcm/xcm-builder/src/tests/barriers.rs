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

use xcm_executor::traits::Properties;

use super::*;

fn props(weight_credit: Weight) -> Properties {
	Properties { weight_credit, message_id: None }
}

#[test]
fn take_weight_credit_barrier_should_work() {
	let mut message =
		Xcm::<()>(vec![TransferAsset { assets: (Parent, 100).into(), beneficiary: Here.into() }]);
	let mut properties = props(Weight::from_parts(10, 10));
	let r = TakeWeightCredit::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(10, 10),
		&mut properties,
	);
	assert_eq!(r, Ok(()));
	assert_eq!(properties.weight_credit, Weight::zero());

	let r = TakeWeightCredit::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(10, 10),
		&mut properties,
	);
	assert_eq!(r, Err(ProcessMessageError::Overweight(Weight::from_parts(10, 10))));
	assert_eq!(properties.weight_credit, Weight::zero());
}

#[test]
fn computed_origin_should_work() {
	let mut message = Xcm::<()>(vec![
		UniversalOrigin(GlobalConsensus(Kusama)),
		DescendOrigin(Parachain(100).into()),
		DescendOrigin(PalletInstance(69).into()),
		WithdrawAsset((Parent, 100).into()),
		BuyExecution {
			fees: (Parent, 100).into(),
			weight_limit: Limited(Weight::from_parts(100, 100)),
		},
		TransferAsset { assets: (Parent, 100).into(), beneficiary: Here.into() },
	]);

	AllowPaidFrom::set(vec![(
		Parent,
		Parent,
		GlobalConsensus(Kusama),
		Parachain(100),
		PalletInstance(69),
	)
		.into()]);

	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(100, 100),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Unsupported));

	let r = WithComputedOrigin::<
		AllowTopLevelPaidExecutionFrom<IsInVec<AllowPaidFrom>>,
		ExecutorUniversalLocation,
		ConstU32<2>,
	>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(100, 100),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Unsupported));

	let r = WithComputedOrigin::<
		AllowTopLevelPaidExecutionFrom<IsInVec<AllowPaidFrom>>,
		ExecutorUniversalLocation,
		ConstU32<5>,
	>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(100, 100),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()));
}

#[test]
fn allow_unpaid_should_work() {
	let mut message =
		Xcm::<()>(vec![TransferAsset { assets: (Parent, 100).into(), beneficiary: Here.into() }]);

	AllowUnpaidFrom::set(vec![Parent.into()]);

	let r = AllowUnpaidExecutionFrom::<IsInVec<AllowUnpaidFrom>>::should_execute(
		&Parachain(1).into(),
		message.inner_mut(),
		Weight::from_parts(10, 10),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Unsupported));

	let r = AllowUnpaidExecutionFrom::<IsInVec<AllowUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(10, 10),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()));
}

#[test]
fn allow_explicit_unpaid_should_work() {
	let mut bad_message1 =
		Xcm::<()>(vec![TransferAsset { assets: (Parent, 100).into(), beneficiary: Here.into() }]);

	let mut bad_message2 = Xcm::<()>(vec![
		UnpaidExecution {
			weight_limit: Limited(Weight::from_parts(10, 10)),
			check_origin: Some(Parent.into()),
		},
		TransferAsset { assets: (Parent, 100).into(), beneficiary: Here.into() },
	]);

	let mut good_message = Xcm::<()>(vec![
		UnpaidExecution {
			weight_limit: Limited(Weight::from_parts(20, 20)),
			check_origin: Some(Parent.into()),
		},
		TransferAsset { assets: (Parent, 100).into(), beneficiary: Here.into() },
	]);

	AllowExplicitUnpaidFrom::set(vec![Parent.into(), (Parent, Parachain(1000)).into()]);
	type ExplicitUnpaidBarrier<T> = AllowExplicitUnpaidExecutionFrom<T, mock::Aliasers>;

	let r = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parachain(1).into(),
		good_message.inner_mut(),
		Weight::from_parts(20, 20),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Unsupported));

	let r = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		bad_message1.inner_mut(),
		Weight::from_parts(20, 20),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Overweight(Weight::from_parts(20, 20))));

	let r = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		bad_message2.inner_mut(),
		Weight::from_parts(20, 20),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Overweight(Weight::from_parts(20, 20))));

	let r = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		good_message.inner_mut(),
		Weight::from_parts(20, 20),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()));

	let mut message_with_different_weight_parts = Xcm::<()>(vec![
		UnpaidExecution {
			weight_limit: Limited(Weight::from_parts(20, 10)),
			check_origin: Some(Parent.into()),
		},
		TransferAsset { assets: (Parent, 100).into(), beneficiary: Here.into() },
	]);

	let r = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message_with_different_weight_parts.inner_mut(),
		Weight::from_parts(20, 20),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Overweight(Weight::from_parts(20, 20))));

	let r = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message_with_different_weight_parts.inner_mut(),
		Weight::from_parts(10, 10),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()));

	// Invalid since location to alias is not allowed.
	let mut message = Xcm::<()>::builder_unsafe()
		.receive_teleported_asset((Here, 100u128))
		.alias_origin(Parachain(1000))
		.unpaid_execution(Unlimited, None)
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Err(ProcessMessageError::Unsupported));

	// Valid because all parachains are children of the relay chain.
	let mut message = Xcm::<()>::builder_unsafe()
		.receive_teleported_asset((Here, 100u128))
		.alias_origin((Parent, Parachain(1000)))
		.unpaid_execution(Unlimited, None)
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Ok(()));

	// Valid.
	let mut message = Xcm::<()>::builder_unsafe()
		.alias_origin((Parent, Parachain(1000)))
		.unpaid_execution(Unlimited, None)
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Ok(()));

	// Invalid because `ClearOrigin` clears origin and `UnpaidExecution`
	// can't know if there are enough permissions.
	let mut message = Xcm::<()>::builder_unsafe()
		.receive_teleported_asset((Here, 100u128))
		.clear_origin()
		.unpaid_execution(Unlimited, None)
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Err(ProcessMessageError::Unsupported));

	// Valid.
	let mut message = Xcm::<()>::builder_unsafe()
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited((Parent, 100u128))
		.descend_origin(Parachain(1000))
		.unpaid_execution(Unlimited, None)
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(40, 40),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Ok(()));

	// Invalid because of `ClearOrigin`.
	let mut message = Xcm::<()>::builder_unsafe()
		.receive_teleported_asset((Here, 100u128))
		.clear_origin()
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Err(ProcessMessageError::Unsupported));

	// Invalid because there is no `UnpaidExecution`.
	let mut message = Xcm::<()>::builder_unsafe()
		.receive_teleported_asset((Here, 100u128))
		.alias_origin((Parent, Parachain(1000)))
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Err(ProcessMessageError::BadFormat));

	// Invalid because even though alias is valid, it can't use `UnpaidExecution`.
	let assets: Vec<Asset> = vec![
		(Parent, 100u128).into(),
		((Parent, PalletInstance(10), GeneralIndex(1000)), 100u128).into(),
	];
	let mut message = Xcm::<()>::builder_unsafe()
		.set_hints(vec![AssetClaimer {
			location: AccountId32 { id: [100u8; 32], network: None }.into(),
		}])
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited(assets)
		.withdraw_asset((GeneralIndex(1), 100u128))
		.alias_origin((Parent, AccountId32 { id: [128u8; 32], network: None }))
		.unpaid_execution(Unlimited, None)
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(60, 60),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Err(ProcessMessageError::Unsupported));

	// Invalid because `UnpaidExecution` specifies less weight than needed.
	let assets: Vec<Asset> = vec![
		(Parent, 100u128).into(),
		((Parent, PalletInstance(10), GeneralIndex(1000)), 100u128).into(),
	];
	let mut message = Xcm::<()>::builder_unsafe()
		.set_hints(vec![AssetClaimer {
			location: AccountId32 { id: [100u8; 32], network: None }.into(),
		}])
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited(assets)
		.withdraw_asset((GeneralIndex(1), 100u128))
		.alias_origin((Parent, Parachain(1000)))
		.unpaid_execution(Limited(Weight::from_parts(50, 50)), None)
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(60, 60),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Err(ProcessMessageError::Overweight(Weight::from_parts(60, 60))));

	// Invalid because of too many instructions before `UnpaidExecution`.
	let assets: Vec<Asset> = vec![
		(Parent, 100u128).into(),
		((Parent, PalletInstance(10), GeneralIndex(1000)), 100u128).into(),
	];
	let mut message = Xcm::<()>::builder_unsafe()
		.set_hints(vec![AssetClaimer {
			location: AccountId32 { id: [100u8; 32], network: None }.into(),
		}])
		.receive_teleported_asset((Here, 100u128))
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited(assets)
		.withdraw_asset((GeneralIndex(1), 100u128))
		.alias_origin((Parent, AccountId32 { id: [128u8; 32], network: None }))
		.unpaid_execution(Limited(Weight::from_parts(50, 50)), None)
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(70, 70),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Err(ProcessMessageError::Overweight(Weight::from_parts(70, 70))));

	// Valid.
	let assets: Vec<Asset> = vec![
		(Parent, 100u128).into(),
		((Parent, PalletInstance(10), GeneralIndex(1000)), 100u128).into(),
	];
	let mut message = Xcm::<()>::builder_unsafe()
		.set_hints(vec![AssetClaimer {
			location: AccountId32 { id: [100u8; 32], network: None }.into(),
		}])
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited(assets)
		.withdraw_asset((GeneralIndex(1), 100u128))
		.alias_origin((Parent, Parachain(1000)))
		.unpaid_execution(Unlimited, None)
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(60, 60),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Ok(()));

	// Valid.
	let assets: Vec<Asset> = vec![
		(Parent, 100u128).into(),
		((Parent, PalletInstance(10), GeneralIndex(1000)), 100u128).into(),
	];
	let mut message = Xcm::<()>::builder_unsafe()
		.set_hints(vec![AssetClaimer {
			location: AccountId32 { id: [100u8; 32], network: None }.into(),
		}])
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited(assets)
		.withdraw_asset((GeneralIndex(1), 100u128))
		.descend_origin(Parachain(1000))
		.unpaid_execution(Unlimited, None)
		.build();
	let result = ExplicitUnpaidBarrier::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(60, 60),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Ok(()));
}

#[test]
fn allow_explicit_unpaid_fails_with_alias_origin_if_no_aliasers() {
	AllowExplicitUnpaidFrom::set(vec![(Parent, Parachain(1000)).into()]);

	let assets: Vec<Asset> = vec![
		(Parent, 100u128).into(),
		((Parent, PalletInstance(10), GeneralIndex(1000)), 100u128).into(),
	];
	let mut good_message = Xcm::<()>::builder_unsafe()
		.set_hints(vec![AssetClaimer {
			location: AccountId32 { id: [100u8; 32], network: None }.into(),
		}])
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited(assets)
		.withdraw_asset((GeneralIndex(1), 100u128))
		.descend_origin(Parachain(1000))
		.unpaid_execution(Unlimited, None)
		.build();
	let result =
		AllowExplicitUnpaidExecutionFrom::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
			&Parent.into(),
			good_message.inner_mut(),
			Weight::from_parts(100, 100),
			&mut props(Weight::zero()),
		);
	assert_eq!(result, Ok(()));

	let assets: Vec<Asset> = vec![
		(Parent, 100u128).into(),
		((Parent, PalletInstance(10), GeneralIndex(1000)), 100u128).into(),
	];
	let mut bad_message = Xcm::<()>::builder_unsafe()
		.set_hints(vec![AssetClaimer {
			location: AccountId32 { id: [100u8; 32], network: None }.into(),
		}])
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited(assets)
		.withdraw_asset((GeneralIndex(1), 100u128))
		.alias_origin((Parent, Parachain(1000)))
		.unpaid_execution(Unlimited, None)
		.build();
	// Barrier has `Aliasers` set as `Nothing` by default, rejecting message if it
	// has an `AliasOrigin` instruction.
	let result =
		AllowExplicitUnpaidExecutionFrom::<IsInVec<AllowExplicitUnpaidFrom>>::should_execute(
			&Parent.into(),
			bad_message.inner_mut(),
			Weight::from_parts(100, 100),
			&mut props(Weight::zero()),
		);
	assert_eq!(result, Err(ProcessMessageError::Unsupported));
}

#[test]
fn allow_explicit_unpaid_with_computed_origin() {
	AllowExplicitUnpaidFrom::set(vec![
		(Parent, Parachain(1000)).into(),
		(Parent, Parent, GlobalConsensus(Polkadot), Parachain(1000)).into(),
	]);
	type ExplicitUnpaidBarrier<T> = AllowExplicitUnpaidExecutionFrom<T, mock::Aliasers>;

	// Message that passes without `WithComputedOrigin` should also pass with it.
	let assets: Vec<Asset> = vec![
		(Parent, 100u128).into(),
		((Parent, PalletInstance(10), GeneralIndex(1000)), 100u128).into(),
	];
	let mut message = Xcm::<()>::builder_unsafe()
		.set_hints(vec![AssetClaimer {
			location: AccountId32 { id: [100u8; 32], network: None }.into(),
		}])
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited(assets)
		.withdraw_asset((GeneralIndex(1), 100u128))
		.alias_origin((Parent, Parachain(1000)))
		.unpaid_execution(Unlimited, None)
		.build();
	let result = WithComputedOrigin::<
		ExplicitUnpaidBarrier<IsInVec<AllowExplicitUnpaidFrom>>,
		ExecutorUniversalLocation,
		ConstU32<2>,
	>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(100, 100),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Ok(()));

	// Can manipulate origin before the inner barrier.
	// For example, to act as another network.
	let assets: Vec<Asset> = vec![
		(Parent, 100u128).into(),
		((Parent, PalletInstance(10), GeneralIndex(1000)), 100u128).into(),
	];
	let mut message = Xcm::<()>::builder_unsafe()
		.universal_origin(Polkadot)
		.set_hints(vec![AssetClaimer {
			location: AccountId32 { id: [100u8; 32], network: None }.into(),
		}])
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited(assets)
		.withdraw_asset((GeneralIndex(1), 100u128))
		.alias_origin((Parent, Parent, GlobalConsensus(Polkadot), Parachain(1000)))
		.unpaid_execution(Unlimited, None)
		.build();
	let result = WithComputedOrigin::<
		ExplicitUnpaidBarrier<IsInVec<AllowExplicitUnpaidFrom>>,
		ExecutorUniversalLocation,
		ConstU32<2>,
	>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(100, 100),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Ok(()));

	// Any invalid conversions from the new origin fail.
	let assets: Vec<Asset> = vec![
		(Parent, 100u128).into(),
		((Parent, PalletInstance(10), GeneralIndex(1000)), 100u128).into(),
	];
	let mut message = Xcm::<()>::builder_unsafe()
		.universal_origin(Polkadot)
		.set_hints(vec![AssetClaimer {
			location: AccountId32 { id: [100u8; 32], network: None }.into(),
		}])
		.receive_teleported_asset((Here, 100u128))
		.reserve_asset_deposited(assets)
		.withdraw_asset((GeneralIndex(1), 100u128))
		.alias_origin((Parent, Parachain(1000)))
		.unpaid_execution(Unlimited, None)
		.build();
	let result = WithComputedOrigin::<
		ExplicitUnpaidBarrier<IsInVec<AllowExplicitUnpaidFrom>>,
		ExecutorUniversalLocation,
		ConstU32<2>,
	>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(100, 100),
		&mut props(Weight::zero()),
	);
	assert_eq!(result, Err(ProcessMessageError::Unsupported));
}

#[test]
fn allow_paid_should_work() {
	AllowPaidFrom::set(vec![Parent.into()]);

	let mut message =
		Xcm::<()>(vec![TransferAsset { assets: (Parent, 100).into(), beneficiary: Here.into() }]);

	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parachain(1).into(),
		message.inner_mut(),
		Weight::from_parts(10, 10),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Unsupported));

	let fees = (Parent, 1).into();
	let mut underpaying_message = Xcm::<()>(vec![
		ReserveAssetDeposited((Parent, 100).into()),
		BuyExecution { fees, weight_limit: Limited(Weight::from_parts(20, 20)) },
		DepositAsset { assets: AllCounted(1).into(), beneficiary: Here.into() },
	]);

	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parent.into(),
		underpaying_message.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Overweight(Weight::from_parts(30, 30))));

	let fees = (Parent, 1).into();
	let mut paying_message = Xcm::<()>(vec![
		ReserveAssetDeposited((Parent, 100).into()),
		BuyExecution { fees, weight_limit: Limited(Weight::from_parts(30, 30)) },
		DepositAsset { assets: AllCounted(1).into(), beneficiary: Here.into() },
	]);

	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parachain(1).into(),
		paying_message.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Unsupported));

	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parent.into(),
		paying_message.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()));

	let fees = (Parent, 1).into();
	let mut paying_message_with_different_weight_parts = Xcm::<()>(vec![
		WithdrawAsset((Parent, 100).into()),
		BuyExecution { fees, weight_limit: Limited(Weight::from_parts(20, 10)) },
		DepositAsset { assets: AllCounted(1).into(), beneficiary: Here.into() },
	]);

	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parent.into(),
		paying_message_with_different_weight_parts.inner_mut(),
		Weight::from_parts(20, 20),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Overweight(Weight::from_parts(20, 20))));

	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parent.into(),
		paying_message_with_different_weight_parts.inner_mut(),
		Weight::from_parts(10, 10),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()))
}

#[test]
fn allow_paid_should_deprivilege_origin() {
	AllowPaidFrom::set(vec![Parent.into()]);
	let fees = (Parent, 1).into();

	let mut paying_message_clears_origin = Xcm::<()>(vec![
		ReserveAssetDeposited((Parent, 100).into()),
		ClearOrigin,
		BuyExecution { fees, weight_limit: Limited(Weight::from_parts(30, 30)) },
		DepositAsset { assets: AllCounted(1).into(), beneficiary: Here.into() },
	]);
	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parent.into(),
		paying_message_clears_origin.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()));

	let mut paying_message_aliases_origin = paying_message_clears_origin.clone();
	paying_message_aliases_origin.0[1] = AliasOrigin(Parachain(1).into());
	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parent.into(),
		paying_message_aliases_origin.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()));

	let mut paying_message_descends_origin = paying_message_clears_origin.clone();
	paying_message_descends_origin.0[1] = DescendOrigin(Parachain(1).into());
	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parent.into(),
		paying_message_descends_origin.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()));

	let mut paying_message_fake_descends_origin = paying_message_clears_origin.clone();
	paying_message_fake_descends_origin.0[1] = DescendOrigin(Here.into());
	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parent.into(),
		paying_message_fake_descends_origin.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Overweight(Weight::from_parts(30, 30))));
}

#[test]
fn allow_paid_should_allow_hints() {
	AllowPaidFrom::set(vec![Parent.into()]);
	let fees = (Parent, 1).into();

	let mut paying_message_with_hints = Xcm::<()>(vec![
		ReserveAssetDeposited((Parent, 100).into()),
		SetHints { hints: vec![AssetClaimer { location: Location::here() }].try_into().unwrap() },
		BuyExecution { fees, weight_limit: Limited(Weight::from_parts(30, 30)) },
		DepositAsset { assets: AllCounted(1).into(), beneficiary: Here.into() },
	]);
	let r = AllowTopLevelPaidExecutionFrom::<IsInVec<AllowPaidFrom>>::should_execute(
		&Parent.into(),
		paying_message_with_hints.inner_mut(),
		Weight::from_parts(30, 30),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()));
}

#[test]
fn suspension_should_work() {
	TestSuspender::set_suspended(true);
	AllowUnpaidFrom::set(vec![Parent.into()]);

	let mut message =
		Xcm::<()>(vec![TransferAsset { assets: (Parent, 100).into(), beneficiary: Here.into() }]);
	let r = RespectSuspension::<AllowUnpaidExecutionFrom::<IsInVec<AllowUnpaidFrom>>, TestSuspender>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(10, 10),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Err(ProcessMessageError::Yield));

	TestSuspender::set_suspended(false);
	let mut message =
		Xcm::<()>(vec![TransferAsset { assets: (Parent, 100).into(), beneficiary: Here.into() }]);
	let r = RespectSuspension::<AllowUnpaidExecutionFrom::<IsInVec<AllowUnpaidFrom>>, TestSuspender>::should_execute(
		&Parent.into(),
		message.inner_mut(),
		Weight::from_parts(10, 10),
		&mut props(Weight::zero()),
	);
	assert_eq!(r, Ok(()));
}

#[test]
fn allow_subscriptions_from_should_work() {
	// allow only parent
	AllowSubsFrom::set(vec![Location::parent()]);

	// closure for (xcm, origin) testing with `AllowSubscriptionsFrom`
	let assert_should_execute = |mut xcm: Vec<Instruction<()>>, origin, expected_result| {
		assert_eq!(
			AllowSubscriptionsFrom::<IsInVec<AllowSubsFrom>>::should_execute(
				&origin,
				&mut xcm,
				Weight::from_parts(10, 10),
				&mut props(Weight::zero()),
			),
			expected_result
		);
	};

	// invalid origin
	assert_should_execute(
		vec![SubscribeVersion {
			query_id: Default::default(),
			max_response_weight: Default::default(),
		}],
		Parachain(1).into_location(),
		Err(ProcessMessageError::Unsupported),
	);
	assert_should_execute(
		vec![UnsubscribeVersion],
		Parachain(1).into_location(),
		Err(ProcessMessageError::Unsupported),
	);

	// invalid XCM (unexpected instruction before)
	assert_should_execute(
		vec![
			SetAppendix(Xcm(vec![])),
			SubscribeVersion {
				query_id: Default::default(),
				max_response_weight: Default::default(),
			},
		],
		Location::parent(),
		Err(ProcessMessageError::BadFormat),
	);
	assert_should_execute(
		vec![SetAppendix(Xcm(vec![])), UnsubscribeVersion],
		Location::parent(),
		Err(ProcessMessageError::BadFormat),
	);
	// invalid XCM (unexpected instruction after)
	assert_should_execute(
		vec![
			SubscribeVersion {
				query_id: Default::default(),
				max_response_weight: Default::default(),
			},
			SetTopic([0; 32]),
		],
		Location::parent(),
		Err(ProcessMessageError::BadFormat),
	);
	assert_should_execute(
		vec![UnsubscribeVersion, SetTopic([0; 32])],
		Location::parent(),
		Err(ProcessMessageError::BadFormat),
	);
	// invalid XCM (unexpected instruction)
	assert_should_execute(
		vec![SetAppendix(Xcm(vec![]))],
		Location::parent(),
		Err(ProcessMessageError::BadFormat),
	);

	// ok
	assert_should_execute(
		vec![SubscribeVersion {
			query_id: Default::default(),
			max_response_weight: Default::default(),
		}],
		Location::parent(),
		Ok(()),
	);
	assert_should_execute(vec![UnsubscribeVersion], Location::parent(), Ok(()));
}

#[test]
fn allow_hrmp_notifications_from_relay_chain_should_work() {
	// closure for (xcm, origin) testing with `AllowHrmpNotificationsFromRelayChain`
	let assert_should_execute = |mut xcm: Vec<Instruction<()>>, origin, expected_result| {
		assert_eq!(
			AllowHrmpNotificationsFromRelayChain::should_execute(
				&origin,
				&mut xcm,
				Weight::from_parts(10, 10),
				&mut props(Weight::zero()),
			),
			expected_result
		);
	};

	// invalid origin
	assert_should_execute(
		vec![HrmpChannelAccepted { recipient: Default::default() }],
		Location::new(1, [Parachain(1)]),
		Err(ProcessMessageError::Unsupported),
	);

	// invalid XCM (unexpected instruction before)
	assert_should_execute(
		vec![SetAppendix(Xcm(vec![])), HrmpChannelAccepted { recipient: Default::default() }],
		Location::parent(),
		Err(ProcessMessageError::BadFormat),
	);
	// invalid XCM (unexpected instruction after)
	assert_should_execute(
		vec![HrmpChannelAccepted { recipient: Default::default() }, SetTopic([0; 32])],
		Location::parent(),
		Err(ProcessMessageError::BadFormat),
	);
	// invalid XCM (unexpected instruction)
	assert_should_execute(
		vec![SetAppendix(Xcm(vec![]))],
		Location::parent(),
		Err(ProcessMessageError::BadFormat),
	);

	// ok
	assert_should_execute(
		vec![HrmpChannelAccepted { recipient: Default::default() }],
		Location::parent(),
		Ok(()),
	);
	assert_should_execute(
		vec![HrmpNewChannelOpenRequest {
			max_capacity: Default::default(),
			sender: Default::default(),
			max_message_size: Default::default(),
		}],
		Location::parent(),
		Ok(()),
	);
	assert_should_execute(
		vec![HrmpChannelClosing {
			recipient: Default::default(),
			sender: Default::default(),
			initiator: Default::default(),
		}],
		Location::parent(),
		Ok(()),
	);
}

#[test]
fn deny_then_try_works() {
	/// A dummy `DenyExecution` impl which returns `ProcessMessageError::Yield` when XCM contains
	/// `ClearTransactStatus`
	struct DenyClearTransactStatusAsYield;
	impl DenyExecution for DenyClearTransactStatusAsYield {
		fn deny_execution<RuntimeCall>(
			_origin: &Location,
			instructions: &mut [Instruction<RuntimeCall>],
			_max_weight: Weight,
			_properties: &mut Properties,
		) -> Result<(), ProcessMessageError> {
			instructions.matcher().match_next_inst_while(
				|_| true,
				|inst| match inst {
					ClearTransactStatus { .. } => Err(ProcessMessageError::Yield),
					_ => Ok(ControlFlow::Continue(())),
				},
			)?;
			Ok(())
		}
	}

	/// A dummy `DenyExecution` impl which returns `ProcessMessageError::BadFormat` when XCM
	/// contains `ClearOrigin` with origin location from `Here`
	struct DenyClearOriginFromHereAsBadFormat;
	impl DenyExecution for DenyClearOriginFromHereAsBadFormat {
		fn deny_execution<RuntimeCall>(
			origin: &Location,
			instructions: &mut [Instruction<RuntimeCall>],
			_max_weight: Weight,
			_properties: &mut Properties,
		) -> Result<(), ProcessMessageError> {
			instructions.matcher().match_next_inst_while(
				|_| true,
				|inst| match inst {
					ClearOrigin { .. } =>
						if origin.clone() == Here.into_location() {
							Err(ProcessMessageError::BadFormat)
						} else {
							Ok(ControlFlow::Continue(()))
						},
					_ => Ok(ControlFlow::Continue(())),
				},
			)?;
			Ok(())
		}
	}

	/// A dummy `DenyExecution` impl which returns `ProcessMessageError::StackLimitReached` when XCM
	/// contains a single `UnsubscribeVersion`
	struct DenyUnsubscribeVersionAsStackLimitReached;
	impl DenyExecution for DenyUnsubscribeVersionAsStackLimitReached {
		fn deny_execution<RuntimeCall>(
			_origin: &Location,
			instructions: &mut [Instruction<RuntimeCall>],
			_max_weight: Weight,
			_properties: &mut Properties,
		) -> Result<(), ProcessMessageError> {
			if instructions.len() != 1 {
				return Ok(())
			}
			match instructions.get(0).unwrap() {
				UnsubscribeVersion { .. } => Err(ProcessMessageError::StackLimitReached),
				_ => Ok(()),
			}
		}
	}

	/// A dummy `ShouldExecute` impl which returns `Ok(())` when XCM contains a single `ClearError`,
	/// else return `ProcessMessageError::Yield`
	struct AllowSingleClearErrorOrYield;
	impl ShouldExecute for AllowSingleClearErrorOrYield {
		fn should_execute<Call>(
			_origin: &Location,
			instructions: &mut [Instruction<Call>],
			_max_weight: Weight,
			_properties: &mut Properties,
		) -> Result<(), ProcessMessageError> {
			instructions.matcher().assert_remaining_insts(1)?.match_next_inst(
				|inst| match inst {
					ClearError { .. } => Ok(()),
					_ => Err(ProcessMessageError::Yield),
				},
			)?;
			Ok(())
		}
	}

	/// A dummy `ShouldExecute` impl which returns `Ok(())` when XCM contains `ClearTopic` and
	/// origin from `Here`, else return `ProcessMessageError::Unsupported`
	struct AllowClearTopicFromHere;
	impl ShouldExecute for AllowClearTopicFromHere {
		fn should_execute<Call>(
			origin: &Location,
			instructions: &mut [Instruction<Call>],
			_max_weight: Weight,
			_properties: &mut Properties,
		) -> Result<(), ProcessMessageError> {
			ensure!(origin.clone() == Here.into_location(), ProcessMessageError::Unsupported);
			let mut found = false;
			instructions.matcher().match_next_inst_while(
				|_| true,
				|inst| match inst {
					ClearTopic { .. } => {
						found = true;
						Ok(ControlFlow::Break(()))
					},
					_ => Ok(ControlFlow::Continue(())),
				},
			)?;
			ensure!(found, ProcessMessageError::Unsupported);
			Ok(())
		}
	}
	// closure for (xcm, origin) testing with `DenyThenTry`
	let assert_should_execute = |mut xcm: Vec<Instruction<()>>, origin, expected_result| {
		pub type Barrier = DenyThenTry<
			(
				DenyClearTransactStatusAsYield,
				DenyClearOriginFromHereAsBadFormat,
				DenyUnsubscribeVersionAsStackLimitReached,
			),
			(AllowSingleClearErrorOrYield, AllowClearTopicFromHere),
		>;
		assert_eq!(
			Barrier::should_execute(
				&origin,
				&mut xcm,
				Weight::from_parts(10, 10),
				&mut props(Weight::zero()),
			),
			expected_result
		);
	};

	// Deny cases:
	// trigger DenyClearTransactStatusAsYield
	assert_should_execute(
		vec![ClearTransactStatus],
		Parachain(1).into_location(),
		Err(ProcessMessageError::Yield),
	);
	// DenyClearTransactStatusAsYield wins against AllowSingleClearErrorOrYield
	assert_should_execute(
		vec![ClearError, ClearTransactStatus],
		Parachain(1).into_location(),
		Err(ProcessMessageError::Yield),
	);
	// trigger DenyClearOriginFromHereAsBadFormat
	assert_should_execute(
		vec![ClearOrigin],
		Here.into_location(),
		Err(ProcessMessageError::BadFormat),
	);
	// trigger DenyUnsubscribeVersionAsStackLimitReached
	assert_should_execute(
		vec![UnsubscribeVersion],
		Here.into_location(),
		Err(ProcessMessageError::StackLimitReached),
	);

	// deny because none of the allow items match
	assert_should_execute(
		vec![ClearError, ClearTopic],
		Parachain(1).into_location(),
		Err(ProcessMessageError::Unsupported),
	);

	// ok
	assert_should_execute(vec![ClearError], Parachain(1).into_location(), Ok(()));
	assert_should_execute(vec![ClearTopic], Here.into(), Ok(()));
	assert_should_execute(vec![ClearError, ClearTopic], Here.into_location(), Ok(()));
}

#[test]
fn deny_reserve_transfer_to_relaychain_should_work() {
	let assert_deny_execution = |mut xcm: Vec<Instruction<()>>, origin, expected_result| {
		assert_eq!(
			DenyReserveTransferToRelayChain::deny_execution(
				&origin,
				&mut xcm,
				Weight::from_parts(10, 10),
				&mut props(Weight::zero()),
			),
			expected_result
		);
	};
	// deny DepositReserveAsset to RelayChain
	assert_deny_execution(
		vec![DepositReserveAsset {
			assets: Wild(All),
			dest: Location::parent(),
			xcm: vec![].into(),
		}],
		Here.into_location(),
		Err(ProcessMessageError::Unsupported),
	);
	// deny InitiateReserveWithdraw to RelayChain
	assert_deny_execution(
		vec![InitiateReserveWithdraw {
			assets: Wild(All),
			reserve: Location::parent(),
			xcm: vec![].into(),
		}],
		Here.into_location(),
		Err(ProcessMessageError::Unsupported),
	);
	// deny TransferReserveAsset to RelayChain
	assert_deny_execution(
		vec![TransferReserveAsset {
			assets: vec![].into(),
			dest: Location::parent(),
			xcm: vec![].into(),
		}],
		Here.into_location(),
		Err(ProcessMessageError::Unsupported),
	);
	// accept DepositReserveAsset to destination other than RelayChain
	assert_deny_execution(
		vec![DepositReserveAsset {
			assets: Wild(All),
			dest: Here.into_location(),
			xcm: vec![].into(),
		}],
		Here.into_location(),
		Ok(()),
	);
	// others instructions should pass
	assert_deny_execution(vec![ClearOrigin], Here.into_location(), Ok(()));
}
