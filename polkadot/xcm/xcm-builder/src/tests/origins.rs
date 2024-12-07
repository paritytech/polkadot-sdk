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
fn universal_origin_should_work() {
	AllowUnpaidFrom::set(vec![[Parachain(1)].into(), [Parachain(2)].into()]);
	clear_universal_aliases();
	// Parachain 1 may represent Kusama to us
	add_universal_alias(Parachain(1), Kusama);
	// Parachain 2 may represent Polkadot to us
	add_universal_alias(Parachain(2), Polkadot);

	let message = Xcm(vec![
		UniversalOrigin(GlobalConsensus(Kusama)),
		TransferAsset { assets: (Parent, 100u128).into(), beneficiary: Here.into() },
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(2),
		message,
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(10, 10), error: XcmError::InvalidLocation }
	);

	let message = Xcm(vec![
		UniversalOrigin(GlobalConsensus(Kusama)),
		TransferAsset { assets: (Parent, 100u128).into(), beneficiary: Here.into() },
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(20, 20), error: XcmError::NotWithdrawable }
	);

	add_asset((Ancestor(2), GlobalConsensus(Kusama)), (Parent, 100));
	let message = Xcm(vec![
		UniversalOrigin(GlobalConsensus(Kusama)),
		TransferAsset { assets: (Parent, 100u128).into(), beneficiary: Here.into() },
	]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message,
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(20, 20) });
	assert_eq!(asset_list((Ancestor(2), GlobalConsensus(Kusama))), vec![]);
}

#[test]
fn export_message_should_work() {
	// Bridge chain (assumed to be Relay) lets Parachain #1 have message execution for free.
	AllowUnpaidFrom::set(vec![[Parachain(1)].into()]);
	// Local parachain #1 issues a transfer asset on Polkadot Relay-chain, transferring 100 Planck
	// to Polkadot parachain #2.
	let expected_message = Xcm(vec![TransferAsset {
		assets: (Here, 100u128).into(),
		beneficiary: Parachain(2).into(),
	}]);
	let expected_hash = fake_message_hash(&expected_message);
	let message = Xcm(vec![ExportMessage {
		network: Polkadot,
		destination: Here,
		xcm: expected_message.clone(),
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
	let uni_src = (ByGenesis([0; 32]), Parachain(42), Parachain(1)).into();
	assert_eq!(
		exported_xcm(),
		vec![(Polkadot, 403611790, uni_src, Here, expected_message, expected_hash)]
	);
}

#[test]
fn unpaid_execution_should_work() {
	// Bridge chain (assumed to be Relay) lets Parachain #1 have message execution for free.
	AllowUnpaidFrom::set(vec![[Parachain(1)].into()]);
	// Bridge chain (assumed to be Relay) lets Parachain #2 have message execution for free if it
	// asks.
	AllowExplicitUnpaidFrom::set(vec![[Parachain(2)].into()]);
	// Asking for unpaid execution of up to 9 weight on the assumption it is origin of #2.
	let message = Xcm(vec![UnpaidExecution {
		weight_limit: Limited(Weight::from_parts(9, 9)),
		check_origin: Some(Parachain(2).into()),
	}]);
	let mut hash = fake_message_hash(&message);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(1),
		message.clone(),
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(
		r,
		Outcome::Incomplete { used: Weight::from_parts(10, 10), error: XcmError::BadOrigin }
	);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(2),
		message.clone(),
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Error { error: XcmError::Barrier });

	let message = Xcm(vec![UnpaidExecution {
		weight_limit: Limited(Weight::from_parts(10, 10)),
		check_origin: Some(Parachain(2).into()),
	}]);
	let r = XcmExecutor::<TestConfig>::prepare_and_execute(
		Parachain(2),
		message.clone(),
		&mut hash,
		Weight::from_parts(50, 50),
		Weight::zero(),
	);
	assert_eq!(r, Outcome::Complete { used: Weight::from_parts(10, 10) });
}
