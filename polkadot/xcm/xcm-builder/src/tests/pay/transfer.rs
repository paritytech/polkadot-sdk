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

//! Tests for making sure `TransferOverXcm::transfer` generates the correct message and sends it to
//! the correct destination

use super::{mock::*, *};
use crate::AliasesIntoAccountId32;
use frame_support::{
	assert_ok, parameter_types,
	traits::{fungible::Mutate, fungibles::Mutate as FungiblesMutate},
};
use xcm::{
	latest::{InteriorLocation, Junctions::X2, Xcm},
	v5::{AssetId, Location, Parent},
};
use xcm_executor::{traits::ConvertLocation, XcmExecutor};

parameter_types! {
	pub SenderAccount: AccountId = AccountId::new([3u8; 32]);
	pub SenderLocationOnTarget: Location = Location::new(
		1,
		X2([Parachain(MockRuntimeParachainId::get().into()), AccountId32 { network: None, id: SenderAccount::get().into() }].into()),
	);
	pub SenderAccountOnTarget: AccountId = SovereignAccountOf::convert_location(&SenderLocationOnTarget::get()).expect("can convert");
	pub InteriorAccount: InteriorLocation = AccountId32 { id: SenderAccount::get().into(), network: None }.into();
	pub Timeout: BlockNumber = 5; // 5 blocks
}

type TestTransferOverXcm =
	TransferOverXcm<AliasesIntoAccountId32<AnyNetwork, AccountId>, TestTransferOverXcmHelper>;

type TestTransferOverXcmHelper = TransferOverXcmHelper<
	TestMessageSender,
	TestQueryHandler<TestConfig, BlockNumber>,
	TestFeeManager,
	Timeout,
	AccountId,
	AssetKind,
	LocatableAssetKindConverter,
	AliasesIntoAccountId32<AnyNetwork, AccountId>,
>;

fn fungible_amount(asset: Asset) -> u128 {
	let Asset { id: _, ref fun } = asset;
	match fun {
		Fungible(fee) => *fee,
		NonFungible(_) => panic!("not fungible"),
	}
}

/// Scenario:
/// Account #3 on the local chain, parachain 42, controls an amount of funds on parachain 2.
/// [`TransferOverXcm::transfer`] creates the correct message for account #3 to pay another account,
/// account #5, on parachain 1000, remotely, in the relay chains native token.
#[test]
fn transfer_over_xcm_works() {
	let recipient = AccountId::new([5u8; 32]);

	// transact the parents native asset on parachain 1000.
	let asset_kind = AssetKind {
		destination: (Parent, Parachain(1000)).into(),
		asset_id: RelayLocation::get().into(),
	};
	let transfer_amount = INITIAL_BALANCE / 10;

	new_test_ext().execute_with(|| {
		// The parachain's native token
		mock::Assets::set_balance(0, &SenderAccountOnTarget::get(), INITIAL_BALANCE);
		// The relaychain's native token
		mock::Assets::set_balance(1, &SenderAccountOnTarget::get(), INITIAL_BALANCE);
		mock::Balances::set_balance(&SenderAccountOnTarget::get(), INITIAL_BALANCE);

		// Check starting balance
		assert_eq!(mock::Assets::balance(0, &recipient), 0);
		assert_eq!(mock::Assets::balance(1, &recipient), 0);

		let fee_asset =
			Asset { id: AssetId(RelayLocation::get()), fun: Fungible(1_000_000_000_000_u128) };

		assert_ok!(TestTransferOverXcm::transfer(
			&SenderAccount::get(),
			&recipient,
			asset_kind.clone(),
			transfer_amount,
			Some(fee_asset.clone())
		));

		let expected_message = remote_transfer_xcm(
			recipient.clone(),
			(asset_kind.asset_id, transfer_amount).into(),
			fee_asset.clone().into(),
		);
		assert_send_and_execute_msg(expected_message);

		assert_eq!(mock::Assets::balance(1, &recipient), transfer_amount);

		// The mock trader does not refund any weight. Hence, the balance is exactly the
		// initial amount minus what we withdrew for transferring and paying the remote fees.
		assert_eq!(
			mock::Assets::balance(1, &SenderAccountOnTarget::get()),
			INITIAL_BALANCE - transfer_amount - fungible_amount(fee_asset.into())
		);
	});
}

#[test]
fn sender_on_relative_to_asset_location_works() {
	let asset_kind = AssetKind {
		destination: (Parent, Parachain(1000)).into(),
		asset_id: RelayLocation::get().into(),
	};

	let sender_on_remote = TestTransferOverXcmHelper::from_relative_to_asset_location(
		&SenderAccount::get(),
		asset_kind.clone(),
	)
	.unwrap();

	assert_eq!(sender_on_remote, SenderLocationOnTarget::get());
}

fn assert_send_and_execute_msg(expected_message: Xcm<()>) {
	let expected_hash = fake_message_hash(&expected_message);

	assert_eq!(
		sent_xcm(),
		vec![((Parent, Parachain(1000)).into(), expected_message, expected_hash)]
	);

	let (_, message, mut hash) = sent_xcm()[0].clone();
	let message = Xcm::<<XcmConfig as xcm_executor::Config>::RuntimeCall>::from(message.clone());

	// Execute message in parachain 1000 with our parachains's origin
	let origin = (Parent, Parachain(MockRuntimeParachainId::get().into()));
	let _result = XcmExecutor::<XcmConfig>::prepare_and_execute(
		origin,
		message,
		&mut hash,
		Weight::MAX,
		Weight::zero(),
	);
}

fn remote_transfer_xcm<Call>(
	recipient: AccountId,
	transfer_asset: Asset,
	fee_asset: Asset,
) -> Xcm<Call> {
	Xcm(vec![
		// Change the origin to the local account on the target chain
		DescendOrigin(AccountId32 { id: SenderAccount::get().into(), network: None }.into()),
		WithdrawAsset(fee_asset.clone().into()),
		PayFees { asset: fee_asset.clone() },
		SetAppendix(Xcm(vec![
			ReportError(QueryResponseInfo {
				destination: (Parent, Parachain(MockRuntimeParachainId::get().into())).into(),
				query_id: 1,
				max_weight: Weight::MAX,
			}),
			RefundSurplus,
			DepositAsset {
				assets: AssetFilter::Wild(WildAsset::All),
				beneficiary: SenderLocationOnTarget::get(),
			},
		])),
		TransferAsset {
			beneficiary: AccountId32 { network: None, id: recipient.clone().into() }.into(),
			assets: transfer_asset.into(),
		},
	])
}
