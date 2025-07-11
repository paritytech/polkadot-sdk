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

//! Tests for making sure `TransferOverXcm::transfer` generates the correct message and sends it to the
//! correct destination

use super::{mock::*, xcm_mock::*, *};
use crate::{
    treasuries_xcm_payout::{ConstantKsmFee, GetRemoteFee, TransferOverXcm},
    xcm_config::KsmLocation,
};
use codec::{Decode, Encode};
use frame_support::{
    assert_ok, parameter_types,
    traits::{fungible::Mutate, fungibles::Mutate as FungiblesMutate},
};
use pallet_encointer_treasuries::Transfer;
use parachains_common::{AccountId, BlockNumber};
use xcm::{
    latest::{BodyId, BodyPart, InteriorLocation, Junctions::X2, Xcm},
    v5::{AssetId, Location, Parent},
};
use xcm_builder::{AliasesIntoAccountId32, LocatableAssetId};
use xcm_executor::{traits::ConvertLocation, XcmExecutor};

/// Type representing both a location and an asset that is held at that location.
/// The id of the held asset is relative to the location where it is being held.
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug)]
pub struct AssetKind {
    destination: Location,
    asset_id: AssetId,
}

pub struct LocatableAssetKindConverter;
impl sp_runtime::traits::TryConvert<AssetKind, LocatableAssetId> for LocatableAssetKindConverter {
    fn try_convert(value: AssetKind) -> Result<LocatableAssetId, AssetKind> {
        Ok(LocatableAssetId { asset_id: value.asset_id, location: value.destination })
    }
}

parameter_types! {
	pub SenderAccount: AccountId = AccountId::new([3u8; 32]);
	pub InteriorAccount: InteriorLocation = AccountId32 { id: SenderAccount::get().into(), network: None }.into();
	pub InteriorBody: InteriorLocation = Plurality { id: BodyId::Treasury, part: BodyPart::Voice }.into();
	pub Timeout: BlockNumber = 5; // 5 blocks
}

/// Scenario:
/// Account #3 on the local chain, parachain 42, controls an amount of funds on parachain 2.
/// [`PayOverXcm::pay`] creates the correct message for account #3 to pay another account, account
/// #5, on parachain 1000, remotely, in its native token.
#[test]
fn transfer_over_xcm_works() {
    sp_tracing::init_for_tests();

    let sender = AccountId::new([1u8; 32]);
    let sender_location_on_target =
        Location::new(1, X2([Parachain(42), AccountId32 { network: None, id: [1; 32] }].into()));
    let sender_account_on_target =
        SovereignAccountOf::convert_location(&sender_location_on_target).expect("can convert");

    let recipient = AccountId::new([5u8; 32]);

    // transact the parents native asset on parachain 1000.
    let asset_kind = AssetKind {
        destination: (Parent, Parachain(1000)).into(),
        asset_id: KsmLocation::get().into(),
    };
    let transfer_amount = INITIAL_BALANCE / 10;

    new_test_ext().execute_with(|| {
        // The parachain's native token
        mock::Assets::set_balance(0, &sender_account_on_target, INITIAL_BALANCE);
        // The relaychain's native token
        mock::Assets::set_balance(1, &sender_account_on_target, INITIAL_BALANCE);
        mock::Balances::set_balance(&sender_account_on_target, INITIAL_BALANCE);

        // Check starting balance
        assert_eq!(mock::Assets::balance(0, &recipient), 0);
        assert_eq!(mock::Assets::balance(1, &recipient), 0);

        assert_ok!(TransferOverXcm::<
			TestMessageSender,
			TestQueryHandler<TestConfig, BlockNumber>,
			Timeout,
			AccountId,
			AssetKind,
			LocatableAssetKindConverter,
			AliasesIntoAccountId32<AnyNetwork, AccountId>,
			ConstantKsmFee,
		>::transfer(&sender, &recipient, asset_kind.clone(), transfer_amount));

        let fee_asset = ConstantKsmFee::get_remote_fee(Xcm::new(), None);
        let Asset { id: _, ref fun } = fee_asset;
        let fee_amount = match fun {
            Fungible(fee) => *fee,
            NonFungible(_) => panic!("Invalid fee"),
        };

        let expected_message = Xcm(vec![
            // Change the origin to the local account on the target chain
            DescendOrigin(AccountId32 { id: sender.into(), network: None }.into()),
            // Assume that we always pay in native for now
            WithdrawAsset(fee_asset.clone().into()),
            PayFees { asset: fee_asset },
            SetAppendix(Xcm(vec![
                ReportError(QueryResponseInfo {
                    destination: (Parent, Parachain(42)).into(),
                    query_id: 1,
                    max_weight: Weight::zero(),
                }),
                RefundSurplus,
                DepositAsset {
                    assets: AssetFilter::Wild(WildAsset::All),
                    beneficiary: sender_location_on_target,
                },
            ])),
            TransferAsset {
                beneficiary: AccountId32 { network: None, id: recipient.clone().into() }.into(),
                assets: (asset_kind.asset_id, transfer_amount).into(),
            },
        ]);
        let expected_hash = fake_message_hash(&expected_message);

        assert_eq!(
            sent_xcm(),
            vec![((Parent, Parachain(1000)).into(), expected_message, expected_hash)]
        );

        let (_, message, mut hash) = sent_xcm()[0].clone();
        let message =
            Xcm::<<XcmConfig as xcm_executor::Config>::RuntimeCall>::from(message.clone());

        // Execute message in parachain 1000 with parachain 42's origin
        let origin = (Parent, Parachain(42));
        let _result = XcmExecutor::<XcmConfig>::prepare_and_execute(
            origin,
            message,
            &mut hash,
            Weight::MAX,
            Weight::zero(),
        );

        assert_eq!(mock::Assets::balance(1, &recipient), transfer_amount);

        let expected_lower_bound = INITIAL_BALANCE - transfer_amount - fee_amount;
        assert!(mock::Assets::balance(1, &sender_account_on_target) > expected_lower_bound);
    });
}

#[test]
fn sender_on_remote_works() {
    sp_tracing::init_for_tests();

    let sender = AccountId::new([1u8; 32]);
    let sender_location_on_target =
        Location::new(1, X2([Parachain(42), AccountId32 { network: None, id: [1; 32] }].into()));

    let asset_kind = AssetKind {
        destination: (Parent, Parachain(1000)).into(),
        asset_id: KsmLocation::get().into(),
    };

    let sender_on_remote = TransferOverXcm::<
        TestMessageSender,
        TestQueryHandler<TestConfig, BlockNumber>,
        Timeout,
        AccountId,
        AssetKind,
        LocatableAssetKindConverter,
        AliasesIntoAccountId32<AnyNetwork, AccountId>,
        ConstantKsmFee,
    >::from_on_remote(&sender, asset_kind.clone())
        .unwrap();

    assert_eq!(sender_location_on_target, sender_on_remote,);
}
