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

//! Tests for making sure `PayOverXcm::pay` generates the correct message and sends it to the
//! correct destination

use super::{mock::*, *};
use frame_support::{assert_ok, traits::tokens::Pay};

parameter_types! {
	pub SenderAccount: AccountId = AccountId::new([3u8; 32]);
	pub InteriorAccount: InteriorLocation = AccountId32 { id: SenderAccount::get().into(), network: None }.into();
	pub InteriorBody: InteriorLocation = Plurality { id: BodyId::Treasury, part: BodyPart::Voice }.into();
	pub Timeout: BlockNumber = 5; // 5 blocks
}

/// Scenario:
/// Account #3 on the local chain, parachain 42, controls an amount of funds on parachain 2.
/// [`PayOverXcm::pay`] creates the correct message for account #3 to pay another account, account
/// #5, on parachain 2, remotely, in its native token.
#[test]
fn pay_over_xcm_works() {
	let recipient = AccountId::new([5u8; 32]);
	let asset_kind =
		AssetKind { destination: (Parent, Parachain(2)).into(), asset_id: Here.into() };
	let amount = 10 * UNITS;

	new_test_ext().execute_with(|| {
		// Check starting balance
		assert_eq!(mock::Assets::balance(0, &recipient), 0);

		assert_ok!(PayOverXcm::<
			InteriorAccount,
			XcmConfig,
			TestQueryHandler<TestConfig, BlockNumber>,
			Timeout,
			AccountId,
			AssetKind,
			LocatableAssetKindConverter,
			AliasesIntoAccountId32<AnyNetwork, AccountId>,
		>::pay(&recipient, asset_kind, amount));

		let expected_message = Xcm(vec![
			DescendOrigin(AccountId32 { id: SenderAccount::get().into(), network: None }.into()),
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			SetAppendix(Xcm(vec![
				SetFeesMode { jit_withdraw: true },
				ReportError(QueryResponseInfo {
					destination: (Parent, Parachain(42)).into(),
					query_id: 1,
					max_weight: Weight::zero(),
				}),
			])),
			TransferAsset {
				assets: (Here, amount).into(),
				beneficiary: AccountId32 { id: recipient.clone().into(), network: None }.into(),
			},
		]);
		let expected_hash = fake_message_hash(&expected_message);

		assert_eq!(
			sent_xcm(),
			vec![((Parent, Parachain(2)).into(), expected_message, expected_hash)]
		);

		let (_, message, mut hash) = sent_xcm()[0].clone();
		let message =
			Xcm::<<XcmConfig as xcm_executor::Config>::RuntimeCall>::from(message.clone());

		// Execute message in parachain 2 with parachain 42's origin
		let origin = (Parent, Parachain(42));
		XcmExecutor::<XcmConfig>::prepare_and_execute(
			origin,
			message,
			&mut hash,
			Weight::MAX,
			Weight::zero(),
		);
		assert_eq!(mock::Assets::balance(0, &recipient), amount);
	});
}

/// Scenario:
/// A pluralistic body, a Treasury, on the local chain, parachain 42, controls an amount of funds
/// on parachain 2. [`PayOverXcm::pay`] creates the correct message for the treasury to pay
/// another account, account #7, on parachain 2, remotely, in the relay's token.
#[test]
fn pay_over_xcm_governance_body() {
	let recipient = AccountId::new([7u8; 32]);
	let asset_kind =
		AssetKind { destination: (Parent, Parachain(2)).into(), asset_id: Parent.into() };
	let amount = 10 * UNITS;

	let relay_asset_index = 1;

	new_test_ext().execute_with(|| {
		// Check starting balance
		assert_eq!(mock::Assets::balance(relay_asset_index, &recipient), 0);

		assert_ok!(PayOverXcm::<
			InteriorBody,
			XcmConfig,
			TestQueryHandler<TestConfig, BlockNumber>,
			Timeout,
			AccountId,
			AssetKind,
			LocatableAssetKindConverter,
			AliasesIntoAccountId32<AnyNetwork, AccountId>,
		>::pay(&recipient, asset_kind, amount));

		let expected_message = Xcm(vec![
			DescendOrigin(Plurality { id: BodyId::Treasury, part: BodyPart::Voice }.into()),
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			SetAppendix(Xcm(vec![
				SetFeesMode { jit_withdraw: true },
				ReportError(QueryResponseInfo {
					destination: (Parent, Parachain(42)).into(),
					query_id: 1,
					max_weight: Weight::zero(),
				}),
			])),
			TransferAsset {
				assets: (Parent, amount).into(),
				beneficiary: AccountId32 { id: recipient.clone().into(), network: None }.into(),
			},
		]);
		let expected_hash = fake_message_hash(&expected_message);
		assert_eq!(
			sent_xcm(),
			vec![((Parent, Parachain(2)).into(), expected_message, expected_hash)]
		);

		let (_, message, mut hash) = sent_xcm()[0].clone();
		let message =
			Xcm::<<XcmConfig as xcm_executor::Config>::RuntimeCall>::from(message.clone());

		// Execute message in parachain 2 with parachain 42's origin
		let origin = (Parent, Parachain(42));
		XcmExecutor::<XcmConfig>::prepare_and_execute(
			origin,
			message,
			&mut hash,
			Weight::MAX,
			Weight::zero(),
		);
		assert_eq!(mock::Assets::balance(relay_asset_index, &recipient), amount);
	});
}

/// Regression test: Verifies that when delivery fees are required but the sender cannot pay them,
/// the pay operation fails and no message is delivered.
///
/// This test covers a bug fix where the old implementation would either:
/// - Allow free delivery for any origin (incorrect)
/// - Burn non-existent tokens or mint fees out of thin air
///
/// The fix ensures that delivery fees are properly charged BEFORE the message is delivered.
#[test]
fn pay_over_xcm_fails_when_delivery_fees_cannot_be_paid() {
	let recipient = AccountId::new([5u8; 32]);
	let asset_kind =
		AssetKind { destination: (Parent, Parachain(2)).into(), asset_id: Here.into() };
	let amount = 10 * UNITS;
	let delivery_fee_amount = 1 * UNITS;

	new_test_ext().execute_with(|| {
		// Set delivery fee - the sender account doesn't have this asset
		set_send_price((Here, delivery_fee_amount));

		// Verify sender doesn't have funds to pay delivery fees
		// SenderAccount is AccountId::new([3u8; 32]) which is different from
		// sibling_chain_account_id(42, [3u8; 32]) that has funds in genesis
		assert_eq!(mock::Assets::balance(0, &SenderAccount::get()), 0);

		// Pay should fail because delivery fees cannot be charged
		// The error is FailedToTransactAsset because the asset transactor cannot withdraw fees
		let result = PayOverXcm::<
			InteriorAccount,
			XcmConfig,
			TestQueryHandler<TestConfig, BlockNumber>,
			Timeout,
			AccountId,
			AssetKind,
			LocatableAssetKindConverter,
			AliasesIntoAccountId32<AnyNetwork, AccountId>,
		>::pay(&recipient, asset_kind, amount);
		assert!(
			matches!(result, Err(xcm::latest::Error::FailedToTransactAsset(_))),
			"Expected FailedToTransactAsset error, got {:?}",
			result
		);

		// Verify no message was delivered - this is the key regression check
		// The old buggy code would have delivered the message despite not being able to pay fees
		assert!(sent_xcm().is_empty(), "No message should be sent when delivery fees cannot be paid");
	});
}

/// Regression test: Verifies that when delivery fees are required and the sender has sufficient
/// funds, the fees are properly charged and the message is delivered.
#[test]
fn pay_over_xcm_charges_delivery_fees_before_sending() {
	use frame_support::traits::fungibles::Mutate;

	let recipient = AccountId::new([5u8; 32]);
	let asset_kind =
		AssetKind { destination: (Parent, Parachain(2)).into(), asset_id: Here.into() };
	let amount = 10 * UNITS;
	let delivery_fee_amount = 1 * UNITS;
	let sender_initial_balance = 5 * UNITS;

	new_test_ext().execute_with(|| {
		// Fund the sender account with the delivery fee asset
		mock::Assets::mint_into(0, &SenderAccount::get(), sender_initial_balance).unwrap();
		assert_eq!(mock::Assets::balance(0, &SenderAccount::get()), sender_initial_balance);

		// Set delivery fee
		set_send_price((Here, delivery_fee_amount));

		// Pay should succeed
		assert_ok!(PayOverXcm::<
			InteriorAccount,
			XcmConfig,
			TestQueryHandler<TestConfig, BlockNumber>,
			Timeout,
			AccountId,
			AssetKind,
			LocatableAssetKindConverter,
			AliasesIntoAccountId32<AnyNetwork, AccountId>,
		>::pay(&recipient, asset_kind, amount));

		// Verify delivery fees were charged from sender
		assert_eq!(
			mock::Assets::balance(0, &SenderAccount::get()),
			sender_initial_balance - delivery_fee_amount,
			"Delivery fees should be deducted from sender's balance"
		);

		// Verify message was sent
		assert_eq!(sent_xcm().len(), 1, "Message should be delivered after fees are paid");
	});
}
