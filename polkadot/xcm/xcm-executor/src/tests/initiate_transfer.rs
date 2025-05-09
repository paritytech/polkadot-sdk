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

//! Unit tests related to the `InitiateTransfer` instruction.
//!
//! See [Fellowship RFC 100](https://github.com/polkadot-fellows/rfCs/pull/100),
//! [Fellowship RFC 122](https://github.com/polkadot-fellows/rfCs/pull/122), and the
//! [specification](https://github.com/polkadot-fellows/xcm-format) for more information.

use codec::Encode;
use frame_support::BoundedVec;
use xcm::{latest::AssetTransferFilter, prelude::*};

use super::mock::*;

// The sender and recipient we use across these tests.
const SENDER: [u8; 32] = [0; 32];
const RECIPIENT: [u8; 32] = [1; 32];
const RECIPIENT2: [u8; 32] = [2; 32];

#[test]
fn clears_origin() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER, (Here, 100u128));

	let xcm_on_dest =
		Xcm(vec![RefundSurplus, DepositAsset { assets: Wild(All), beneficiary: RECIPIENT.into() }]);
	let assets: Assets = (Here, 90u128).into();
	let xcm = Xcm::<TestCall>(vec![
		WithdrawAsset((Here, 100u128).into()),
		PayFees { asset: (Here, 10u128).into() },
		InitiateTransfer {
			destination: Parent.into(),
			remote_fees: Some(AssetTransferFilter::ReserveDeposit(assets.into())),
			preserve_origin: false,
			assets: BoundedVec::new(),
			remote_xcm: xcm_on_dest,
		},
	]);

	let (mut vm, _) = instantiate_executor(SENDER, xcm.clone());

	// Program runs successfully.
	let res = vm.bench_process(xcm);
	assert!(res.is_ok(), "execution error {:?}", res);

	let (dest, sent_message) = sent_xcm().pop().unwrap();
	assert_eq!(dest, Parent.into());
	assert_eq!(sent_message.len(), 5);
	let mut instr = sent_message.inner().iter();
	assert!(matches!(instr.next().unwrap(), ReserveAssetDeposited(..)));
	assert!(matches!(instr.next().unwrap(), PayFees { .. }));
	assert!(matches!(instr.next().unwrap(), ClearOrigin));
	assert!(matches!(instr.next().unwrap(), RefundSurplus));
	assert!(matches!(instr.next().unwrap(), DepositAsset { .. }));
}

#[test]
fn preserves_origin() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER, (Here, 100u128));

	let xcm_on_dest =
		Xcm(vec![RefundSurplus, DepositAsset { assets: Wild(All), beneficiary: RECIPIENT.into() }]);
	let assets: Assets = (Here, 90u128).into();
	let xcm = Xcm::<TestCall>(vec![
		WithdrawAsset((Here, 100u128).into()),
		PayFees { asset: (Here, 10u128).into() },
		InitiateTransfer {
			destination: Parent.into(),
			remote_fees: Some(AssetTransferFilter::ReserveDeposit(assets.into())),
			preserve_origin: true,
			assets: BoundedVec::new(),
			remote_xcm: xcm_on_dest,
		},
	]);

	let (mut vm, _) = instantiate_executor(SENDER, xcm.clone());

	// Program runs successfully.
	let res = vm.bench_process(xcm);
	assert!(res.is_ok(), "execution error {:?}", res);

	let (dest, sent_message) = sent_xcm().pop().unwrap();
	assert_eq!(dest, Parent.into());
	assert_eq!(sent_message.len(), 5);
	let mut instr = sent_message.inner().iter();
	assert!(matches!(instr.next().unwrap(), ReserveAssetDeposited(..)));
	assert!(matches!(instr.next().unwrap(), PayFees { .. }));
	assert!(matches!(
		instr.next().unwrap(),
		AliasOrigin(origin) if matches!(origin.unpack(), (0, [Parachain(1000), AccountId32 { id: SENDER, network: None }]))
	));
	assert!(matches!(instr.next().unwrap(), RefundSurplus));
	assert!(matches!(instr.next().unwrap(), DepositAsset { .. }));
}

#[test]
fn unpaid_execution_goes_after_origin_alteration() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(SENDER, (Here, 100u128));

	let xcm_on_destination =
		Xcm::builder_unsafe().refund_surplus().deposit_asset(All, RECIPIENT).build();
	let asset: Asset = (Here, 90u128).into();
	let xcm = Xcm::builder()
		.withdraw_asset((Here, 100u128))
		.pay_fees((Here, 10u128))
		.initiate_transfer(
			Parent,
			None, // We specify no remote fees.
			true, // Preserve origin, necessary for `UnpaidExecution`.
			vec![AssetTransferFilter::ReserveDeposit(asset.into())],
			xcm_on_destination,
		)
		.build();

	// We initialize the executor with the SENDER origin, which is not waived.
	let (mut vm, _) = instantiate_executor(SENDER, xcm.clone());

	// Program fails with `BadOrigin`.
	let result = vm.bench_process(xcm);
	assert!(result.is_ok(), "execution error {:?}", result);

	let (destination, sent_message) = sent_xcm().pop().unwrap();
	assert_eq!(destination, Parent.into());
	assert_eq!(sent_message.len(), 5);
	let mut instructions = sent_message.inner().iter();
	assert!(matches!(instructions.next().unwrap(), ReserveAssetDeposited(..)));
	assert!(matches!(
		instructions.next().unwrap(),
		AliasOrigin(origin) if matches!(origin.unpack(), (0, [Parachain(1000), AccountId32 { id: SENDER, network: None }]))
	));
	assert!(matches!(instructions.next().unwrap(), UnpaidExecution { .. }));
	assert!(matches!(instructions.next().unwrap(), RefundSurplus));
	assert!(matches!(instructions.next().unwrap(), DepositAsset { .. }));
}

#[test]
fn no_alias_origin_if_root() {
	// Make sure the sender has enough funds to withdraw.
	add_asset(Here, (Here, 100u128));

	let xcm_on_destination =
		Xcm::builder_unsafe().refund_surplus().deposit_asset(All, RECIPIENT).build();
	let asset: Asset = (Here, 90u128).into();
	let xcm = Xcm::builder()
		.withdraw_asset((Here, 100u128))
		.pay_fees((Here, 10u128))
		.initiate_transfer(
			Parent,
			None, // We specify no remote fees.
			true, // Preserve origin, necessary for `UnpaidExecution`.
			vec![AssetTransferFilter::ReserveDeposit(asset.into())],
			xcm_on_destination,
		)
		.build();

	// We initialize the executor with the SENDER origin, which is not waived.
	let (mut vm, _) = instantiate_executor(Here, xcm.clone());

	// Program fails with `BadOrigin`.
	let result = vm.bench_process(xcm);
	assert!(result.is_ok(), "execution error {:?}", result);

	let (destination, sent_message) = sent_xcm().pop().unwrap();
	assert_eq!(destination, Parent.into());
	assert_eq!(sent_message.len(), 4);
	let mut instructions = sent_message.inner().iter();
	assert!(matches!(instructions.next().unwrap(), ReserveAssetDeposited(..)));
	assert!(matches!(instructions.next().unwrap(), UnpaidExecution { .. }));
	assert!(matches!(instructions.next().unwrap(), RefundSurplus));
	assert!(matches!(instructions.next().unwrap(), DepositAsset { .. }));
}

// We simulate going from one system parachain to another without
// having to pay remote fees.
#[test]
fn unpaid_transact() {
	let to_another_system_para: Location = (Parent, Parachain(1001)).into();
	// We want to execute some call in the receiving chain.
	let xcm_on_destination = Xcm::builder_unsafe()
		.transact(OriginKind::Superuser, None, b"".encode())
		.build();
	let xcm = Xcm::builder_unsafe()
		.initiate_transfer(
			to_another_system_para.clone(),
			None,   // We specify no remote fees.
			true,   // Preserve necessary for `UnpaidExecution`.
			vec![], // No need for assets.
			xcm_on_destination,
		)
		.build();

	// We initialize the executor with the root origin, which is waived.
	let (mut vm, _) = instantiate_executor(Here, xcm.clone());

	// Program executes successfully.
	let result = vm.bench_process(xcm.clone());
	assert!(result.is_ok(), "execution error: {:?}", result);

	let (destination, sent_message) = sent_xcm().pop().unwrap();
	assert_eq!(destination, to_another_system_para);
	assert_eq!(sent_message.len(), 2);
	let mut instructions = sent_message.inner().iter();
	assert!(matches!(instructions.next().unwrap(), UnpaidExecution { .. }));
	assert!(matches!(instructions.next().unwrap(), Transact { .. }));
}

#[test]
fn deposit_assets_with_retry_burns_dust_and_deposits_rest() {
	// fund sender
	add_asset(SENDER, (Here, 200u128));

	// dust amount (< ED=2)
	let dust: Asset = (Here, 1u128).into();

	// non-dust amount (> ED=2)
	let legit: Asset = (Here, 100u128).into();

	let xcm = Xcm::<TestCall>(vec![
		WithdrawAsset((Here, 101u128).into()),
		DepositAsset {
			assets: Definite(Assets::from(vec![dust.clone()])),
			beneficiary: RECIPIENT.into(),
		},
		DepositAsset {
			assets: Definite(Assets::from(vec![legit.clone()])),
			beneficiary: RECIPIENT.into(),
		},
	]);

	let (mut vm, weight) = instantiate_executor(SENDER, xcm.clone());

	let result = vm.bench_process(xcm);

	assert!(result.is_ok(), "XCM execution must succeed even if one deposit is dust");
	let outcome = vm.bench_post_process(weight);
	assert!(matches!(outcome, Outcome::Complete { .. }), "Expected Complete, got {:?}", outcome);

	let here_assets = asset_list(RECIPIENT);
	assert_eq!(here_assets, vec![legit], "only the ≥ED asset (100) should end up in `Here`");

	// dust is burned, so nothing lands in the trap account
	let trapped = asset_list(TRAPPED_ASSETS);
	assert!(trapped.is_empty(), "dust assets should be silently burned, not trapped");
}

#[test]
fn deposit_assets_with_retry_all_dust_are_burned() {
	// fund sender
	add_asset(SENDER, (Here, 20u128));

	// two dust amounts, both < ED=2
	let d1: Asset = (Here, 1u128).into();
	let d2: Asset = (Here, 1u128).into();

	let xcm = Xcm::<TestCall>(vec![
		// withdraw 1+1 so it succeeds
		WithdrawAsset((Here, (1u128 + 1u128)).into()),
		DepositAsset {
			assets: Definite(Assets::from(vec![d1.clone()])),
			beneficiary: RECIPIENT.into(),
		},
		DepositAsset {
			assets: Definite(Assets::from(vec![d2.clone()])),
			beneficiary: RECIPIENT2.into(),
		},
	]);

	let (mut vm, weight) = instantiate_executor(SENDER, xcm.clone());
	let result = vm.bench_process(xcm);

	assert!(result.is_ok(), "all-dust deposit must not abort");
	let outcome = vm.bench_post_process(weight);
	assert!(matches!(outcome, Outcome::Complete { .. }));

	// none of the two dust deposits should land in either recipient
	let received = asset_list(RECIPIENT);
	assert!(received.is_empty(), "no ≥ED assets, so recipient must get nothing");

	// none of the two dust deposits should land in either recipient
	let received = asset_list(RECIPIENT2);
	assert!(received.is_empty(), "no ≥ED assets, so recipient must get nothing");

	// all dust is burned, trap account stays empty
	let trapped = asset_list(TRAPPED_ASSETS);
	assert!(trapped.is_empty(), "all dust assets must be burned, not trapped");
}
