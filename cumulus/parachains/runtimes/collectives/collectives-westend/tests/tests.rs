// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

#![cfg(test)]

use codec::Encode;
use collectives_westend_runtime::{
	dday::{
		prover,
		prover::{AssetHubAccountProver, StalledAssetHubDataProvider},
		DDayDetectionInstance, DDayReferendaInstance, DDayVotingInstance,
		StalledAssetHubBlockThreshold, SubmissionDeposit,
	},
	fellowship::pallet_fellowship_origins::Origin,
	xcm_config::{AssetHub, GovernanceLocation, LocationToAccountId},
	AllPalletsWithoutSystem, Balances, Block, DDayDetection, DDayReferenda, DDayVoting, Executive,
	ExistentialDeposit, FellowshipCollective, Preimage, Runtime, RuntimeCall, RuntimeOrigin,
	System, TxExtension, UncheckedExtrinsic,
};
use frame_support::{
	assert_err, assert_ok,
	traits::{fungible::Mutate, schedule::DispatchTime, StorePreimage, VoteTally},
};
use pallet_dday_detection::IsStalled;
use pallet_dday_voting::{AccountVote, Conviction, ProvideHash, VerifyProof, Vote};
use pallet_referenda::{ReferendumCount, ReferendumInfoFor};
use parachains_common::{AccountId, Hash};
use parachains_runtimes_test_utils::{GovernanceOrigin, RuntimeHelper};
use polkadot_parachain_primitives::primitives::HeadData;
use sp_core::{crypto::Ss58Codec, Pair};
use sp_runtime::{
	generic::{Era, SignedPayload},
	transaction_validity::TransactionValidityError,
	ApplyExtrinsicResult, DispatchError, Either, MultiSignature, Perbill,
};
use testnet_parachains_constants::westend::fee::WeightToFee;
use xcm::latest::prelude::*;
use xcm_runtime_apis::conversions::LocationToAccountHelper;

const ALICE: [u8; 32] = [1u8; 32];

fn construct_extrinsic(
	sender: sp_core::sr25519::Pair,
	call: RuntimeCall,
) -> Result<UncheckedExtrinsic, TransactionValidityError> {
	let account_id = sp_core::crypto::AccountId32::from(sender.public());
	frame_system::BlockHash::<Runtime>::insert(0, Hash::default());
	let tx_ext: TxExtension = (
		frame_system::AuthorizeCall::<Runtime>::new(),
		frame_system::CheckNonZeroSender::<Runtime>::new(),
		frame_system::CheckSpecVersion::<Runtime>::new(),
		frame_system::CheckTxVersion::<Runtime>::new(),
		frame_system::CheckGenesis::<Runtime>::new(),
		frame_system::CheckEra::<Runtime>::from(Era::immortal()),
		frame_system::CheckNonce::<Runtime>::from(
			frame_system::Pallet::<Runtime>::account(&account_id).nonce,
		)
		.into(),
		frame_system::CheckWeight::<Runtime>::new(),
		pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::from(0).into(),
		frame_metadata_hash_extension::CheckMetadataHash::<Runtime>::new(false),
	)
		.into();
	let payload = SignedPayload::new(call.clone(), tx_ext.clone())?;
	let signature = payload.using_encoded(|e| sender.sign(e));
	Ok(UncheckedExtrinsic::new_signed(
		call,
		account_id.into(),
		MultiSignature::Sr25519(signature),
		tx_ext,
	))
}

fn construct_and_apply_extrinsic(
	account: sp_core::sr25519::Pair,
	call: RuntimeCall,
) -> ApplyExtrinsicResult {
	let xt = construct_extrinsic(account, call)?;
	Executive::apply_extrinsic(xt)
}

#[test]
fn location_conversion_works() {
	// the purpose of hardcoded values is to catch an unintended location conversion logic change.
	struct TestCase {
		description: &'static str,
		location: Location,
		expected_account_id_str: &'static str,
	}

	let test_cases = vec![
		// DescribeTerminus
		TestCase {
			description: "DescribeTerminus Parent",
			location: Location::new(1, Here),
			expected_account_id_str: "5Dt6dpkWPwLaH4BBCKJwjiWrFVAGyYk3tLUabvyn4v7KtESG",
		},
		TestCase {
			description: "DescribeTerminus Sibling",
			location: Location::new(1, [Parachain(1111)]),
			expected_account_id_str: "5Eg2fnssmmJnF3z1iZ1NouAuzciDaaDQH7qURAy3w15jULDk",
		},
		// DescribePalletTerminal
		TestCase {
			description: "DescribePalletTerminal Parent",
			location: Location::new(1, [PalletInstance(50)]),
			expected_account_id_str: "5CnwemvaAXkWFVwibiCvf2EjqwiqBi29S5cLLydZLEaEw6jZ",
		},
		TestCase {
			description: "DescribePalletTerminal Sibling",
			location: Location::new(1, [Parachain(1111), PalletInstance(50)]),
			expected_account_id_str: "5GFBgPjpEQPdaxEnFirUoa51u5erVx84twYxJVuBRAT2UP2g",
		},
		// DescribeAccountId32Terminal
		TestCase {
			description: "DescribeAccountId32Terminal Parent",
			location: Location::new(
				1,
				[Junction::AccountId32 { network: None, id: AccountId::from(ALICE).into() }],
			),
			expected_account_id_str: "5DN5SGsuUG7PAqFL47J9meViwdnk9AdeSWKFkcHC45hEzVz4",
		},
		TestCase {
			description: "DescribeAccountId32Terminal Sibling",
			location: Location::new(
				1,
				[
					Parachain(1111),
					Junction::AccountId32 { network: None, id: AccountId::from(ALICE).into() },
				],
			),
			expected_account_id_str: "5DGRXLYwWGce7wvm14vX1Ms4Vf118FSWQbJkyQigY2pfm6bg",
		},
		// DescribeAccountKey20Terminal
		TestCase {
			description: "DescribeAccountKey20Terminal Parent",
			location: Location::new(1, [AccountKey20 { network: None, key: [0u8; 20] }]),
			expected_account_id_str: "5F5Ec11567pa919wJkX6VHtv2ZXS5W698YCW35EdEbrg14cg",
		},
		TestCase {
			description: "DescribeAccountKey20Terminal Sibling",
			location: Location::new(
				1,
				[Parachain(1111), AccountKey20 { network: None, key: [0u8; 20] }],
			),
			expected_account_id_str: "5CB2FbUds2qvcJNhDiTbRZwiS3trAy6ydFGMSVutmYijpPAg",
		},
		// DescribeTreasuryVoiceTerminal
		TestCase {
			description: "DescribeTreasuryVoiceTerminal Parent",
			location: Location::new(1, [Plurality { id: BodyId::Treasury, part: BodyPart::Voice }]),
			expected_account_id_str: "5CUjnE2vgcUCuhxPwFoQ5r7p1DkhujgvMNDHaF2bLqRp4D5F",
		},
		TestCase {
			description: "DescribeTreasuryVoiceTerminal Sibling",
			location: Location::new(
				1,
				[Parachain(1111), Plurality { id: BodyId::Treasury, part: BodyPart::Voice }],
			),
			expected_account_id_str: "5G6TDwaVgbWmhqRUKjBhRRnH4ry9L9cjRymUEmiRsLbSE4gB",
		},
		// DescribeBodyTerminal
		TestCase {
			description: "DescribeBodyTerminal Parent",
			location: Location::new(1, [Plurality { id: BodyId::Unit, part: BodyPart::Voice }]),
			expected_account_id_str: "5EBRMTBkDisEXsaN283SRbzx9Xf2PXwUxxFCJohSGo4jYe6B",
		},
		TestCase {
			description: "DescribeBodyTerminal Sibling",
			location: Location::new(
				1,
				[Parachain(1111), Plurality { id: BodyId::Unit, part: BodyPart::Voice }],
			),
			expected_account_id_str: "5DBoExvojy8tYnHgLL97phNH975CyT45PWTZEeGoBZfAyRMH",
		},
	];

	for tc in test_cases {
		let expected =
			AccountId::from_string(tc.expected_account_id_str).expect("Invalid AccountId string");

		let got = LocationToAccountHelper::<AccountId, LocationToAccountId>::convert_location(
			tc.location.into(),
		)
		.unwrap();

		assert_eq!(got, expected, "{}", tc.description);
	}
}

#[test]
fn xcm_payment_api_works() {
	parachains_runtimes_test_utils::test_cases::xcm_payment_api_with_native_token_works::<
		Runtime,
		RuntimeCall,
		RuntimeOrigin,
		Block,
		WeightToFee,
	>();
}

#[test]
fn governance_authorize_upgrade_works() {
	use westend_runtime_constants::system_parachain::{ASSET_HUB_ID, COLLECTIVES_ID};

	// no - random para
	assert_err!(
		parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
			Runtime,
			RuntimeOrigin,
		>(GovernanceOrigin::Location(Location::new(1, Parachain(12334)))),
		Either::Right(XcmError::Barrier)
	);
	// no - AssetHub
	assert_err!(
		parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
			Runtime,
			RuntimeOrigin,
		>(GovernanceOrigin::Location(Location::new(1, Parachain(ASSET_HUB_ID)))),
		Either::Right(XcmError::Barrier)
	);
	// no - Collectives
	assert_err!(
		parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
			Runtime,
			RuntimeOrigin,
		>(GovernanceOrigin::Location(Location::new(1, Parachain(COLLECTIVES_ID)))),
		Either::Right(XcmError::Barrier)
	);
	// no - Collectives Voice of Fellows plurality
	assert_err!(
		parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
			Runtime,
			RuntimeOrigin,
		>(GovernanceOrigin::LocationAndDescendOrigin(
			Location::new(1, Parachain(COLLECTIVES_ID)),
			Plurality { id: BodyId::Technical, part: BodyPart::Voice }.into()
		)),
		Either::Right(XcmError::Barrier)
	);

	// ok - relaychain
	assert_ok!(parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
		Runtime,
		RuntimeOrigin,
	>(GovernanceOrigin::Location(Location::parent())));
	assert_ok!(parachains_runtimes_test_utils::test_cases::can_governance_authorize_upgrade::<
		Runtime,
		RuntimeOrigin,
	>(GovernanceOrigin::Location(GovernanceLocation::get())));
}

#[test]
fn dday_referenda_and_voting_works() {
	sp_io::TestExternalities::new(Default::default()).execute_with(|| {
		// Create rank3+ fellow with some balance.
		let account_fellow_rank3 = AccountId::from([0; 32]);
		assert_ok!(FellowshipCollective::do_add_member_to_rank(
			account_fellow_rank3.clone(),
			3,
			false
		));
		assert_ok!(Balances::mint_into(
			&account_fellow_rank3,
			ExistentialDeposit::get() + SubmissionDeposit::get()
		));

		// Create DDay referendum - error.
		assert_err!(
			DDayReferenda::submit(
				RuntimeOrigin::signed(account_fellow_rank3.clone()),
				Box::new(Origin::Fellows.into()),
				{
					// Random call executed when referendum passes.
					let c = RuntimeCall::System(frame_system::Call::remark_with_event {
						remark: vec![],
					});
					<Preimage as StorePreimage>::bound(c).unwrap()
				},
				DispatchTime::At(10),
			),
			DispatchError::BadOrigin,
		);

		// Prepare sample proofs.
		let (asset_hub_header, proof, (ss58_account, ss58_account_secret_key), ..) =
			prover::tests::sample_proof();
		let asset_hub_block_number = asset_hub_header.number;
		let valid_asset_hub_account =
			AccountId::from_ss58check(ss58_account).expect("valid accountId");
		let account_voting_power = AssetHubAccountProver::query_voting_power_for(
			&valid_asset_hub_account,
			asset_hub_header.state_root,
			proof.clone(),
		)
		.expect("valid proof");

		System::set_block_number(13);
		// Sync AssetHub header - execute XCM as source origin would do with `Transact ->
		// Origin::Xcm`.
		assert_ok!(RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::execute_as_origin(
			(AssetHub::get(), OriginKind::Xcm),
			RuntimeCall::DDayDetection(pallet_dday_detection::Call::<
				Runtime,
				DDayDetectionInstance,
			>::note_new_head {
				remote_block_number: asset_hub_block_number,
				remote_head: HeadData(asset_hub_header.encode())
			}),
			None,
		)
		.ensure_complete());
		// Make AssetHub stalled.
		assert!(!DDayDetection::is_stalled());
		System::set_block_number(13 + StalledAssetHubBlockThreshold::get() + 1);
		assert!(DDayDetection::is_stalled());

		// Create DDay referendum.
		assert_ok!(DDayReferenda::submit(
			RuntimeOrigin::signed(account_fellow_rank3),
			Box::new(Origin::Fellows.into()),
			{
				// Random call executed when referendum passes.
				let c =
					RuntimeCall::System(frame_system::Call::remark_with_event { remark: vec![] });
				<Preimage as StorePreimage>::bound(c).unwrap()
			},
			DispatchTime::At(10),
		));
		assert_eq!(ReferendumCount::<Runtime, DDayReferendaInstance>::get(), 1);
		let referenda_id = ReferendumCount::<Runtime, DDayReferendaInstance>::get() - 1;
		assert!(ReferendumInfoFor::<Runtime, DDayReferendaInstance>::get(referenda_id).is_some());

		// Err - vote by proof - a random account cannot vote
		assert_err!(
			DDayVoting::vote(
				RuntimeOrigin::signed(AccountId::from([1; 32])), // invalid account
				referenda_id,
				AccountVote::Standard {
					vote: Vote { aye: true, conviction: Conviction::Locked1x },
					balance: 1,
				},
				(asset_hub_block_number, proof.clone())
			),
			<pallet_dday_voting::Error<Runtime, DDayVotingInstance>>::InvalidProof
		);

		// Err - when more vote.balance than proven voting power
		assert_err!(
			DDayVoting::vote(
				RuntimeOrigin::signed(valid_asset_hub_account.clone()),
				referenda_id,
				AccountVote::Standard {
					vote: Vote { aye: true, conviction: Conviction::Locked1x },
					// more than proven
					balance: account_voting_power.account_power + 1
				},
				(asset_hub_block_number, proof.clone())
			),
			<pallet_dday_voting::Error<Runtime, DDayVotingInstance>>::InsufficientFunds
		);

		// check before
		{
			let status = DDayReferenda::ensure_ongoing(referenda_id).expect("ongoing referenda");
			assert_eq!(status.tally.ayes(status.track), 0);
			assert_eq!(status.tally.support(status.track), Perbill::zero());
			assert_ok!(DDayReferenda::is_referendum_passing(referenda_id), false);
		}

		// Check that AssetHub account does not exist at Collectives (means no balance).
		assert!(!System::account_exists(&valid_asset_hub_account));

		// Ok - vote by proof - generated for proving account `ss58_account`
		// This submits an extrinsic with all transaction extensions, just as an AssetHub user would
		// need to do.
		assert_ok!(construct_and_apply_extrinsic(
			sp_core::sr25519::Pair::from_string(ss58_account_secret_key, None).unwrap(),
			RuntimeCall::DDayVoting(pallet_dday_voting::Call::vote {
				poll_index: referenda_id,
				vote: AccountVote::Standard {
					vote: Vote { aye: true, conviction: Conviction::Locked1x },
					balance: account_voting_power.account_power
				},
				proof: (asset_hub_block_number, proof.clone()),
			})
		));

		// check after - vote is registered, and the total was recorded from the proof.
		{
			let status = DDayReferenda::ensure_ongoing(referenda_id).expect("ongoing referenda");
			assert_eq!(status.tally.ayes(status.track), account_voting_power.account_power);
			assert_eq!(
				status.tally.support(status.track),
				Perbill::from_rational(
					account_voting_power.account_power,
					account_voting_power.total
				)
			);
			assert_ok!(DDayReferenda::is_referendum_passing(referenda_id), false);
		}

		// Make AssetHub not stalled.
		System::set_block_number(14);
		assert!(!DDayDetection::is_stalled());
		// Err - cannot vote, AssetHub is not stalled.
		assert_err!(
			DDayVoting::vote(
				RuntimeOrigin::signed(valid_asset_hub_account.clone()),
				referenda_id,
				AccountVote::Standard {
					vote: Vote { aye: true, conviction: Conviction::Locked1x },
					balance: account_voting_power.account_power
				},
				(asset_hub_block_number, proof.clone())
			),
			<pallet_dday_voting::Error<Runtime, DDayVotingInstance>>::NotOngoing
		);
	})
}

#[test]
pub fn asset_hub_can_sync_dday_data() {
	// Prepare sample proofs.
	let (asset_hub_header, ..) = prover::tests::sample_proof();
	let asset_hub_block_number = asset_hub_header.number;

	sp_io::TestExternalities::new(Default::default()).execute_with(|| {
		// Check before.
		assert!(DDayDetection::last_known_head().is_none());
		assert!(StalledAssetHubDataProvider::<DDayDetection>::provide_hash_for(
			&asset_hub_block_number
		)
		.is_none());

		// Sync AssetHub header - execute XCM as source origin would do with `Transact ->
		// Origin::Xcm`.
		assert_ok!(RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::execute_as_origin(
			(AssetHub::get(), OriginKind::Xcm),
			RuntimeCall::DDayDetection(pallet_dday_detection::Call::<
				Runtime,
				DDayDetectionInstance,
			>::note_new_head {
				remote_block_number: asset_hub_block_number,
				remote_head: HeadData(asset_hub_header.encode())
			}),
			None,
		)
		.ensure_complete());

		// Check after header data.
		let last_known_head = DDayDetection::last_known_head();
		assert_eq!(last_known_head.clone().map(|h| h.head.hash()), Some(asset_hub_header.hash()));
		// None - because it is not stalled yet.
		assert!(StalledAssetHubDataProvider::<DDayDetection>::provide_hash_for(
			&asset_hub_block_number
		)
		.is_none());

		// Simulate stalled head - move local block number behind threshold.
		System::set_block_number(
			last_known_head.unwrap().known_at + StalledAssetHubBlockThreshold::get() + 1,
		);
		// Now the provider works.
		assert_eq!(
			StalledAssetHubDataProvider::<DDayDetection>::provide_hash_for(&asset_hub_block_number),
			Some(asset_hub_header.state_root)
		);
	})
}
