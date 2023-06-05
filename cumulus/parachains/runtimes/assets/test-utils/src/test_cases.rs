// Copyright (C) 2023 Parity Technologies (UK) Ltd.
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

//! Module contains predefined test-case scenarios for `Runtime` with various assets.

use codec::Encode;
use frame_support::{
	assert_noop, assert_ok,
	traits::{fungibles::InspectEnumerable, Get, OriginTrait},
	weights::Weight,
};
use parachains_common::Balance;
use parachains_runtimes_test_utils::{
	assert_metadata, assert_total, AccountIdOf, BalanceOf, CollatorSessionKeys, ExtBuilder,
	RuntimeHelper, ValidatorIdOf, XcmReceivedFrom,
};
use sp_runtime::{
	traits::{MaybeEquivalence, StaticLookup, Zero},
	DispatchError, Saturating,
};
use xcm::latest::prelude::*;
use xcm_executor::{traits::ConvertLocation, XcmExecutor};

/// Test-case makes sure that `Runtime` can receive native asset from relay chain
/// and can teleport it back and to the other parachains
pub fn teleports_for_native_asset_works<
	Runtime,
	XcmConfig,
	CheckingAccount,
	WeightToFee,
	HrmpChannelOpener,
>(
	collator_session_keys: CollatorSessionKeys<Runtime>,
	existential_deposit: BalanceOf<Runtime>,
	target_account: AccountIdOf<Runtime>,
	unwrap_pallet_xcm_event: Box<dyn Fn(Vec<u8>) -> Option<pallet_xcm::Event<Runtime>>>,
	unwrap_xcmp_queue_event: Box<
		dyn Fn(Vec<u8>) -> Option<cumulus_pallet_xcmp_queue::Event<Runtime>>,
	>,
	runtime_para_id: u32,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ cumulus_pallet_xcmp_queue::Config,
	AccountIdOf<Runtime>: Into<[u8; 32]>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	BalanceOf<Runtime>: From<Balance> + Into<u128>,
	WeightToFee: frame_support::weights::WeightToFee<Balance = Balance>,
	<WeightToFee as frame_support::weights::WeightToFee>::Balance: From<u128> + Into<u128>,
	<Runtime as frame_system::Config>::AccountId:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source:
		From<<Runtime as frame_system::Config>::AccountId>,
	XcmConfig: xcm_executor::Config,
	CheckingAccount: Get<AccountIdOf<Runtime>>,
	HrmpChannelOpener: frame_support::inherent::ProvideInherent<
		Call = cumulus_pallet_parachain_system::Call<Runtime>,
	>,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_keys.collators())
		.with_session_keys(collator_session_keys.session_keys())
		.with_safe_xcm_version(XCM_VERSION)
		.with_para_id(runtime_para_id.into())
		.build()
		.execute_with(|| {
			// check Balances before
			assert_eq!(<pallet_balances::Pallet<Runtime>>::free_balance(&target_account), 0.into());
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&CheckingAccount::get()),
				0.into()
			);

			let native_asset_id = MultiLocation::parent();
			let buy_execution_fee_amount_eta =
				WeightToFee::weight_to_fee(&Weight::from_parts(90_000_000_000, 0));
			let native_asset_amount_unit = existential_deposit;
			let native_asset_amount_received =
				native_asset_amount_unit * 10.into() + buy_execution_fee_amount_eta.into();

			// 1. process received teleported assets from relaychain
			let xcm = Xcm(vec![
				ReceiveTeleportedAsset(MultiAssets::from(vec![MultiAsset {
					id: Concrete(native_asset_id),
					fun: Fungible(native_asset_amount_received.into()),
				}])),
				ClearOrigin,
				BuyExecution {
					fees: MultiAsset {
						id: Concrete(native_asset_id),
						fun: Fungible(buy_execution_fee_amount_eta),
					},
					weight_limit: Limited(Weight::from_parts(303531000, 65536)),
				},
				DepositAsset {
					assets: Wild(AllCounted(1)),
					beneficiary: MultiLocation {
						parents: 0,
						interior: X1(AccountId32 {
							network: None,
							id: target_account.clone().into(),
						}),
					},
				},
				ExpectTransactStatus(MaybeErrorCode::Success),
			]);

			let hash = xcm.using_encoded(sp_io::hashing::blake2_256);

			let outcome = XcmExecutor::<XcmConfig>::execute_xcm(
				Parent,
				xcm,
				hash,
				RuntimeHelper::<Runtime>::xcm_max_weight(XcmReceivedFrom::Parent),
			);
			assert_eq!(outcome.ensure_complete(), Ok(()));

			// check Balances after
			assert_ne!(<pallet_balances::Pallet<Runtime>>::free_balance(&target_account), 0.into());
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&CheckingAccount::get()),
				0.into()
			);

			// 2. try to teleport asset back to the relaychain
			{
				let dest = MultiLocation::parent();
				let dest_beneficiary = MultiLocation::parent()
					.appended_with(AccountId32 {
						network: None,
						id: sp_runtime::AccountId32::new([3; 32]).into(),
					})
					.unwrap();

				let target_account_balance_before_teleport =
					<pallet_balances::Pallet<Runtime>>::free_balance(&target_account);
				let native_asset_to_teleport_away = native_asset_amount_unit * 3.into();
				assert!(
					native_asset_to_teleport_away <
						target_account_balance_before_teleport - existential_deposit
				);

				assert_ok!(RuntimeHelper::<Runtime>::do_teleport_assets::<HrmpChannelOpener>(
					RuntimeHelper::<Runtime>::origin_of(target_account.clone()),
					dest,
					dest_beneficiary,
					(native_asset_id, native_asset_to_teleport_away.into()),
					None,
				));
				// check balances
				assert_eq!(
					<pallet_balances::Pallet<Runtime>>::free_balance(&target_account),
					target_account_balance_before_teleport - native_asset_to_teleport_away
				);
				assert_eq!(
					<pallet_balances::Pallet<Runtime>>::free_balance(&CheckingAccount::get()),
					0.into()
				);

				// check events
				RuntimeHelper::<Runtime>::assert_pallet_xcm_event_outcome(
					&unwrap_pallet_xcm_event,
					|outcome| {
						assert_ok!(outcome.ensure_complete());
					},
				);
			}

			// 3. try to teleport asset away to other parachain (1234)
			{
				let other_para_id = 1234;
				let dest = MultiLocation::new(1, X1(Parachain(other_para_id)));
				let dest_beneficiary = MultiLocation::new(1, X1(Parachain(other_para_id)))
					.appended_with(AccountId32 {
						network: None,
						id: sp_runtime::AccountId32::new([3; 32]).into(),
					})
					.unwrap();

				let target_account_balance_before_teleport =
					<pallet_balances::Pallet<Runtime>>::free_balance(&target_account);
				let native_asset_to_teleport_away = native_asset_amount_unit * 3.into();
				assert!(
					native_asset_to_teleport_away <
						target_account_balance_before_teleport - existential_deposit
				);

				assert_ok!(RuntimeHelper::<Runtime>::do_teleport_assets::<HrmpChannelOpener>(
					RuntimeHelper::<Runtime>::origin_of(target_account.clone()),
					dest,
					dest_beneficiary,
					(native_asset_id, native_asset_to_teleport_away.into()),
					Some((runtime_para_id, other_para_id)),
				));

				// check balances
				assert_eq!(
					<pallet_balances::Pallet<Runtime>>::free_balance(&target_account),
					target_account_balance_before_teleport - native_asset_to_teleport_away
				);
				assert_eq!(
					<pallet_balances::Pallet<Runtime>>::free_balance(&CheckingAccount::get()),
					0.into()
				);

				// check events
				RuntimeHelper::<Runtime>::assert_pallet_xcm_event_outcome(
					&unwrap_pallet_xcm_event,
					|outcome| {
						assert_ok!(outcome.ensure_complete());
					},
				);
				assert!(RuntimeHelper::<Runtime>::xcmp_queue_message_sent(unwrap_xcmp_queue_event)
					.is_some());
			}
		})
}

#[macro_export]
macro_rules! include_teleports_for_native_asset_works(
	(
		$runtime:path,
		$xcm_config:path,
		$checking_account:path,
		$weight_to_fee:path,
		$hrmp_channel_opener:path,
		$collator_session_key:expr,
		$existential_deposit:expr,
		$unwrap_pallet_xcm_event:expr,
		$unwrap_xcmp_queue_event:expr,
		$runtime_para_id:expr
	) => {
		#[test]
		fn teleports_for_native_asset_works() {
			const BOB: [u8; 32] = [2u8; 32];
			let target_account = parachains_common::AccountId::from(BOB);

			$crate::test_cases::teleports_for_native_asset_works::<
				$runtime,
				$xcm_config,
				$checking_account,
				$weight_to_fee,
				$hrmp_channel_opener
			>(
				$collator_session_key,
				$existential_deposit,
				target_account,
				$unwrap_pallet_xcm_event,
				$unwrap_xcmp_queue_event,
				$runtime_para_id
			)
		}
	}
);

/// Test-case makes sure that `Runtime` can receive teleported assets from sibling parachain relay chain
pub fn teleports_for_foreign_assets_works<
	Runtime,
	XcmConfig,
	CheckingAccount,
	WeightToFee,
	HrmpChannelOpener,
	SovereignAccountOf,
	ForeignAssetsPalletInstance,
>(
	collator_session_keys: CollatorSessionKeys<Runtime>,
	target_account: AccountIdOf<Runtime>,
	existential_deposit: BalanceOf<Runtime>,
	asset_owner: AccountIdOf<Runtime>,
	unwrap_pallet_xcm_event: Box<dyn Fn(Vec<u8>) -> Option<pallet_xcm::Event<Runtime>>>,
	unwrap_xcmp_queue_event: Box<
		dyn Fn(Vec<u8>) -> Option<cumulus_pallet_xcmp_queue::Event<Runtime>>,
	>,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ cumulus_pallet_xcmp_queue::Config
		+ pallet_assets::Config<ForeignAssetsPalletInstance>,
	AccountIdOf<Runtime>: Into<[u8; 32]>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	BalanceOf<Runtime>: From<Balance>,
	XcmConfig: xcm_executor::Config,
	CheckingAccount: Get<AccountIdOf<Runtime>>,
	HrmpChannelOpener: frame_support::inherent::ProvideInherent<
		Call = cumulus_pallet_parachain_system::Call<Runtime>,
	>,
	WeightToFee: frame_support::weights::WeightToFee<Balance = Balance>,
	<WeightToFee as frame_support::weights::WeightToFee>::Balance: From<u128> + Into<u128>,
	SovereignAccountOf: ConvertLocation<AccountIdOf<Runtime>>,
	<Runtime as pallet_assets::Config<ForeignAssetsPalletInstance>>::AssetId:
		From<MultiLocation> + Into<MultiLocation>,
	<Runtime as pallet_assets::Config<ForeignAssetsPalletInstance>>::AssetIdParameter:
		From<MultiLocation> + Into<MultiLocation>,
	<Runtime as pallet_assets::Config<ForeignAssetsPalletInstance>>::Balance:
		From<Balance> + Into<u128>,
	<Runtime as frame_system::Config>::AccountId:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source:
		From<<Runtime as frame_system::Config>::AccountId>,
	ForeignAssetsPalletInstance: 'static,
{
	// foreign parachain with the same consenus currency as asset
	let foreign_para_id = 2222;
	let foreign_asset_id_multilocation = MultiLocation {
		parents: 1,
		interior: X2(Parachain(foreign_para_id), GeneralIndex(1234567)),
	};

	// foreign creator, which can be sibling parachain to match ForeignCreators
	let foreign_creator = MultiLocation { parents: 1, interior: X1(Parachain(foreign_para_id)) };
	let foreign_creator_as_account_id =
		SovereignAccountOf::convert_location(&foreign_creator).expect("");

	// we want to buy execution with local relay chain currency
	let buy_execution_fee_amount =
		WeightToFee::weight_to_fee(&Weight::from_parts(90_000_000_000, 0));
	let buy_execution_fee = MultiAsset {
		id: Concrete(MultiLocation::parent()),
		fun: Fungible(buy_execution_fee_amount),
	};

	let teleported_foreign_asset_amount = 10000000000000;
	let runtime_para_id = 1000;
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_keys.collators())
		.with_session_keys(collator_session_keys.session_keys())
		.with_balances(vec![
			(
				foreign_creator_as_account_id,
				existential_deposit + (buy_execution_fee_amount * 2).into(),
			),
			(target_account.clone(), existential_deposit),
			(CheckingAccount::get(), existential_deposit),
		])
		.with_safe_xcm_version(XCM_VERSION)
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			// checks target_account before
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&target_account),
				existential_deposit
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&CheckingAccount::get()),
				existential_deposit
			);
			// check `CheckingAccount` before
			assert_eq!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::balance(
					foreign_asset_id_multilocation.into(),
					&target_account
				),
				0.into()
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::balance(
					foreign_asset_id_multilocation.into(),
					&CheckingAccount::get()
				),
				0.into()
			);
			// check totals before
			assert_total::<
				pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>,
				AccountIdOf<Runtime>,
			>(foreign_asset_id_multilocation, 0, 0);

			// create foreign asset (0 total issuance)
			let asset_minimum_asset_balance = 3333333_u128;
			assert_ok!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::force_create(
					RuntimeHelper::<Runtime>::root_origin(),
					foreign_asset_id_multilocation.into(),
					asset_owner.into(),
					false,
					asset_minimum_asset_balance.into()
				)
			);
			assert_total::<
				pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>,
				AccountIdOf<Runtime>,
			>(foreign_asset_id_multilocation, 0, 0);
			assert!(teleported_foreign_asset_amount > asset_minimum_asset_balance);

			// 1. process received teleported assets from relaychain
			let xcm = Xcm(vec![
				// BuyExecution with relaychain native token
				WithdrawAsset(buy_execution_fee.clone().into()),
				BuyExecution {
					fees: MultiAsset {
						id: Concrete(MultiLocation::parent()),
						fun: Fungible(buy_execution_fee_amount),
					},
					weight_limit: Limited(Weight::from_parts(403531000, 65536)),
				},
				// Process teleported asset
				ReceiveTeleportedAsset(MultiAssets::from(vec![MultiAsset {
					id: Concrete(foreign_asset_id_multilocation),
					fun: Fungible(teleported_foreign_asset_amount),
				}])),
				DepositAsset {
					assets: Wild(AllOf {
						id: Concrete(foreign_asset_id_multilocation),
						fun: WildFungibility::Fungible,
					}),
					beneficiary: MultiLocation {
						parents: 0,
						interior: X1(AccountId32 {
							network: None,
							id: target_account.clone().into(),
						}),
					},
				},
				ExpectTransactStatus(MaybeErrorCode::Success),
			]);
			let hash = xcm.using_encoded(sp_io::hashing::blake2_256);

			let outcome = XcmExecutor::<XcmConfig>::execute_xcm(
				foreign_creator,
				xcm,
				hash,
				RuntimeHelper::<Runtime>::xcm_max_weight(XcmReceivedFrom::Sibling),
			);
			assert_eq!(outcome.ensure_complete(), Ok(()));

			// checks target_account after
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&target_account),
				existential_deposit
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::balance(
					foreign_asset_id_multilocation.into(),
					&target_account
				),
				teleported_foreign_asset_amount.into()
			);
			// checks `CheckingAccount` after
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&CheckingAccount::get()),
				existential_deposit
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::balance(
					foreign_asset_id_multilocation.into(),
					&CheckingAccount::get()
				),
				0.into()
			);
			// check total after (twice: target_account + CheckingAccount)
			assert_total::<
				pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>,
				AccountIdOf<Runtime>,
			>(
				foreign_asset_id_multilocation,
				teleported_foreign_asset_amount,
				teleported_foreign_asset_amount,
			);

			// 2. try to teleport asset back to source parachain (foreign_para_id)
			{
				let dest = MultiLocation::new(1, X1(Parachain(foreign_para_id)));
				let dest_beneficiary = MultiLocation::new(1, X1(Parachain(foreign_para_id)))
					.appended_with(AccountId32 {
						network: None,
						id: sp_runtime::AccountId32::new([3; 32]).into(),
					})
					.unwrap();

				let target_account_balance_before_teleport =
					<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::balance(
						foreign_asset_id_multilocation.into(),
						&target_account,
					);
				let asset_to_teleport_away = asset_minimum_asset_balance * 3;
				assert!(
					asset_to_teleport_away <
						(target_account_balance_before_teleport -
							asset_minimum_asset_balance.into())
						.into()
				);

				assert_ok!(RuntimeHelper::<Runtime>::do_teleport_assets::<HrmpChannelOpener>(
					RuntimeHelper::<Runtime>::origin_of(target_account.clone()),
					dest,
					dest_beneficiary,
					(foreign_asset_id_multilocation, asset_to_teleport_away),
					Some((runtime_para_id, foreign_para_id)),
				));

				// check balances
				assert_eq!(
					<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::balance(
						foreign_asset_id_multilocation.into(),
						&target_account
					),
					(target_account_balance_before_teleport - asset_to_teleport_away.into())
				);
				assert_eq!(
					<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::balance(
						foreign_asset_id_multilocation.into(),
						&CheckingAccount::get()
					),
					0.into()
				);
				// check total after (twice: target_account + CheckingAccount)
				assert_total::<
					pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>,
					AccountIdOf<Runtime>,
				>(
					foreign_asset_id_multilocation,
					teleported_foreign_asset_amount - asset_to_teleport_away,
					teleported_foreign_asset_amount - asset_to_teleport_away,
				);

				// check events
				RuntimeHelper::<Runtime>::assert_pallet_xcm_event_outcome(
					&unwrap_pallet_xcm_event,
					|outcome| {
						assert_ok!(outcome.ensure_complete());
					},
				);
				assert!(RuntimeHelper::<Runtime>::xcmp_queue_message_sent(unwrap_xcmp_queue_event)
					.is_some());
			}
		})
}

#[macro_export]
macro_rules! include_teleports_for_foreign_assets_works(
	(
		$runtime:path,
		$xcm_config:path,
		$checking_account:path,
		$weight_to_fee:path,
		$hrmp_channel_opener:path,
		$sovereign_account_of:path,
		$assets_pallet_instance:path,
		$collator_session_key:expr,
		$existential_deposit:expr,
		$unwrap_pallet_xcm_event:expr,
		$unwrap_xcmp_queue_event:expr
	) => {
		#[test]
		fn teleports_for_foreign_assets_works() {
			const BOB: [u8; 32] = [2u8; 32];
			let target_account = parachains_common::AccountId::from(BOB);
			const SOME_ASSET_OWNER: [u8; 32] = [5u8; 32];
			let asset_owner = parachains_common::AccountId::from(SOME_ASSET_OWNER);

			$crate::test_cases::teleports_for_foreign_assets_works::<
				$runtime,
				$xcm_config,
				$checking_account,
				$weight_to_fee,
				$hrmp_channel_opener,
				$sovereign_account_of,
				$assets_pallet_instance
			>(
				$collator_session_key,
				target_account,
				$existential_deposit,
				asset_owner,
				$unwrap_pallet_xcm_event,
				$unwrap_xcmp_queue_event
			)
		}
	}
);

/// Test-case makes sure that `Runtime`'s `xcm::AssetTransactor` can handle native relay chain currency
pub fn asset_transactor_transfer_with_local_consensus_currency_works<Runtime, XcmConfig>(
	collator_session_keys: CollatorSessionKeys<Runtime>,
	source_account: AccountIdOf<Runtime>,
	target_account: AccountIdOf<Runtime>,
	existential_deposit: BalanceOf<Runtime>,
	additional_checks_before: Box<dyn Fn()>,
	additional_checks_after: Box<dyn Fn()>,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config,
	AccountIdOf<Runtime>: Into<[u8; 32]>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	BalanceOf<Runtime>: From<Balance>,
	XcmConfig: xcm_executor::Config,
	<Runtime as pallet_balances::Config>::Balance: From<Balance> + Into<u128>,
	<Runtime as frame_system::Config>::AccountId:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source:
		From<<Runtime as frame_system::Config>::AccountId>,
{
	let unit = existential_deposit;

	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_keys.collators())
		.with_session_keys(collator_session_keys.session_keys())
		.with_balances(vec![(source_account.clone(), (BalanceOf::<Runtime>::from(10_u128) * unit))])
		.with_tracing()
		.build()
		.execute_with(|| {
			// check Balances before
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&source_account),
				(BalanceOf::<Runtime>::from(10_u128) * unit)
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&target_account),
				(BalanceOf::<Runtime>::zero() * unit)
			);

			// additional check before
			additional_checks_before();

			// transfer_asset (deposit/withdraw) ALICE -> BOB
			let _ = RuntimeHelper::<XcmConfig>::do_transfer(
				MultiLocation {
					parents: 0,
					interior: X1(AccountId32 { network: None, id: source_account.clone().into() }),
				},
				MultiLocation {
					parents: 0,
					interior: X1(AccountId32 { network: None, id: target_account.clone().into() }),
				},
				// local_consensus_currency_asset, e.g.: relaychain token (KSM, DOT, ...)
				(
					MultiLocation { parents: 1, interior: Here },
					(BalanceOf::<Runtime>::from(1_u128) * unit).into(),
				),
			)
			.expect("no error");

			// check Balances after
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(source_account),
				(BalanceOf::<Runtime>::from(9_u128) * unit)
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(target_account),
				(BalanceOf::<Runtime>::from(1_u128) * unit)
			);

			additional_checks_after();
		})
}

#[macro_export]
macro_rules! include_asset_transactor_transfer_with_local_consensus_currency_works(
	(
		$runtime:path,
		$xcm_config:path,
		$collator_session_key:expr,
		$existential_deposit:expr,
		$additional_checks_before:expr,
		$additional_checks_after:expr
	) => {
		#[test]
		fn asset_transactor_transfer_with_local_consensus_currency_works() {
			const ALICE: [u8; 32] = [1u8; 32];
			let source_account = parachains_common::AccountId::from(ALICE);
			const BOB: [u8; 32] = [2u8; 32];
			let target_account = parachains_common::AccountId::from(BOB);

			$crate::test_cases::asset_transactor_transfer_with_local_consensus_currency_works::<
				$runtime,
				$xcm_config
			>(
				$collator_session_key,
				source_account,
				target_account,
				$existential_deposit,
				$additional_checks_before,
				$additional_checks_after
			)
		}
	}
);

///Test-case makes sure that `Runtime`'s `xcm::AssetTransactor` can handle native relay chain currency
pub fn asset_transactor_transfer_with_pallet_assets_instance_works<
	Runtime,
	XcmConfig,
	AssetsPalletInstance,
	AssetId,
	AssetIdConverter,
>(
	collator_session_keys: CollatorSessionKeys<Runtime>,
	existential_deposit: BalanceOf<Runtime>,
	asset_id: AssetId,
	asset_owner: AccountIdOf<Runtime>,
	alice_account: AccountIdOf<Runtime>,
	bob_account: AccountIdOf<Runtime>,
	charlie_account: AccountIdOf<Runtime>,
	additional_checks_before: Box<dyn Fn()>,
	additional_checks_after: Box<dyn Fn()>,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ pallet_assets::Config<AssetsPalletInstance>,
	AccountIdOf<Runtime>: Into<[u8; 32]>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	BalanceOf<Runtime>: From<Balance>,
	XcmConfig: xcm_executor::Config,
	<Runtime as pallet_assets::Config<AssetsPalletInstance>>::AssetId:
		From<AssetId> + Into<AssetId>,
	<Runtime as pallet_assets::Config<AssetsPalletInstance>>::AssetIdParameter:
		From<AssetId> + Into<AssetId>,
	<Runtime as pallet_assets::Config<AssetsPalletInstance>>::Balance: From<Balance> + Into<u128>,
	<Runtime as frame_system::Config>::AccountId:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source:
		From<<Runtime as frame_system::Config>::AccountId>,
	AssetsPalletInstance: 'static,
	AssetId: Clone + Copy,
	AssetIdConverter: MaybeEquivalence<MultiLocation, AssetId>,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_keys.collators())
		.with_session_keys(collator_session_keys.session_keys())
		.with_balances(vec![
			(asset_owner.clone(), existential_deposit),
			(alice_account.clone(), existential_deposit),
			(bob_account.clone(), existential_deposit),
		])
		.with_tracing()
		.build()
		.execute_with(|| {
			// create  some asset class
			let asset_minimum_asset_balance = 3333333_u128;
			let asset_id_as_multilocation = AssetIdConverter::convert_back(&asset_id).unwrap();
			assert_ok!(<pallet_assets::Pallet<Runtime, AssetsPalletInstance>>::force_create(
				RuntimeHelper::<Runtime>::root_origin(),
				asset_id.into(),
				asset_owner.clone().into(),
				false,
				asset_minimum_asset_balance.into()
			));

			// We first mint enough asset for the account to exist for assets
			assert_ok!(<pallet_assets::Pallet<Runtime, AssetsPalletInstance>>::mint(
				RuntimeHelper::<Runtime>::origin_of(asset_owner.clone()),
				asset_id.into(),
				alice_account.clone().into(),
				(6 * asset_minimum_asset_balance).into()
			));

			// check Assets before
			assert_eq!(
				<pallet_assets::Pallet<Runtime, AssetsPalletInstance>>::balance(
					asset_id.into(),
					&alice_account
				),
				(6 * asset_minimum_asset_balance).into()
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, AssetsPalletInstance>>::balance(
					asset_id.into(),
					&bob_account
				),
				0.into()
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, AssetsPalletInstance>>::balance(
					asset_id.into(),
					&charlie_account
				),
				0.into()
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, AssetsPalletInstance>>::balance(
					asset_id.into(),
					&asset_owner
				),
				0.into()
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&alice_account),
				existential_deposit
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&bob_account),
				existential_deposit
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&charlie_account),
				0.into()
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&asset_owner),
				existential_deposit
			);
			additional_checks_before();

			// transfer_asset (deposit/withdraw) ALICE -> CHARLIE (not ok - Charlie does not have ExistentialDeposit)
			assert_noop!(
				RuntimeHelper::<XcmConfig>::do_transfer(
					MultiLocation {
						parents: 0,
						interior: X1(AccountId32 {
							network: None,
							id: alice_account.clone().into()
						}),
					},
					MultiLocation {
						parents: 0,
						interior: X1(AccountId32 {
							network: None,
							id: charlie_account.clone().into()
						}),
					},
					(asset_id_as_multilocation, asset_minimum_asset_balance),
				),
				XcmError::FailedToTransactAsset(Into::<&str>::into(
					sp_runtime::TokenError::CannotCreate
				))
			);

			// transfer_asset (deposit/withdraw) ALICE -> BOB (ok - has ExistentialDeposit)
			assert!(matches!(
				RuntimeHelper::<XcmConfig>::do_transfer(
					MultiLocation {
						parents: 0,
						interior: X1(AccountId32 {
							network: None,
							id: alice_account.clone().into()
						}),
					},
					MultiLocation {
						parents: 0,
						interior: X1(AccountId32 { network: None, id: bob_account.clone().into() }),
					},
					(asset_id_as_multilocation, asset_minimum_asset_balance),
				),
				Ok(_)
			));

			// check Assets after
			assert_eq!(
				<pallet_assets::Pallet<Runtime, AssetsPalletInstance>>::balance(
					asset_id.into(),
					&alice_account
				),
				(5 * asset_minimum_asset_balance).into()
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, AssetsPalletInstance>>::balance(
					asset_id.into(),
					&bob_account
				),
				asset_minimum_asset_balance.into()
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, AssetsPalletInstance>>::balance(
					asset_id.into(),
					&charlie_account
				),
				0.into()
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, AssetsPalletInstance>>::balance(
					asset_id.into(),
					&asset_owner
				),
				0.into()
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&alice_account),
				existential_deposit
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&bob_account),
				existential_deposit
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&charlie_account),
				0.into()
			);
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&asset_owner),
				existential_deposit
			);

			additional_checks_after();
		})
}

#[macro_export]
macro_rules! include_asset_transactor_transfer_with_pallet_assets_instance_works(
	(
		$test_name:tt,
		$runtime:path,
		$xcm_config:path,
		$assets_pallet_instance:path,
		$asset_id:path,
		$asset_id_converter:path,
		$collator_session_key:expr,
		$existential_deposit:expr,
		$tested_asset_id:expr,
		$additional_checks_before:expr,
		$additional_checks_after:expr
	) => {
		#[test]
		fn $test_name() {
			const SOME_ASSET_OWNER: [u8; 32] = [5u8; 32];
			let asset_owner = parachains_common::AccountId::from(SOME_ASSET_OWNER);
			const ALICE: [u8; 32] = [1u8; 32];
			let alice_account = parachains_common::AccountId::from(ALICE);
			const BOB: [u8; 32] = [2u8; 32];
			let bob_account = parachains_common::AccountId::from(BOB);
			const CHARLIE: [u8; 32] = [3u8; 32];
			let charlie_account = parachains_common::AccountId::from(CHARLIE);

			$crate::test_cases::asset_transactor_transfer_with_pallet_assets_instance_works::<
				$runtime,
				$xcm_config,
				$assets_pallet_instance,
				$asset_id,
				$asset_id_converter
			>(
				$collator_session_key,
				$existential_deposit,
				$tested_asset_id,
				asset_owner,
				alice_account,
				bob_account,
				charlie_account,
				$additional_checks_before,
				$additional_checks_after
			)
		}
	}
);

pub fn create_and_manage_foreign_assets_for_local_consensus_parachain_assets_works<
	Runtime,
	XcmConfig,
	WeightToFee,
	SovereignAccountOf,
	ForeignAssetsPalletInstance,
	AssetId,
	AssetIdConverter,
>(
	collator_session_keys: CollatorSessionKeys<Runtime>,
	existential_deposit: BalanceOf<Runtime>,
	asset_deposit: BalanceOf<Runtime>,
	metadata_deposit_base: BalanceOf<Runtime>,
	metadata_deposit_per_byte: BalanceOf<Runtime>,
	alice_account: AccountIdOf<Runtime>,
	bob_account: AccountIdOf<Runtime>,
	runtime_call_encode: Box<
		dyn Fn(pallet_assets::Call<Runtime, ForeignAssetsPalletInstance>) -> Vec<u8>,
	>,
	unwrap_pallet_assets_event: Box<
		dyn Fn(Vec<u8>) -> Option<pallet_assets::Event<Runtime, ForeignAssetsPalletInstance>>,
	>,
	additional_checks_before: Box<dyn Fn()>,
	additional_checks_after: Box<dyn Fn()>,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ pallet_assets::Config<ForeignAssetsPalletInstance>,
	AccountIdOf<Runtime>: Into<[u8; 32]>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	BalanceOf<Runtime>: From<Balance>,
	XcmConfig: xcm_executor::Config,
	WeightToFee: frame_support::weights::WeightToFee<Balance = Balance>,
	<WeightToFee as frame_support::weights::WeightToFee>::Balance: From<u128> + Into<u128>,
	SovereignAccountOf: ConvertLocation<AccountIdOf<Runtime>>,
	<Runtime as pallet_assets::Config<ForeignAssetsPalletInstance>>::AssetId:
		From<AssetId> + Into<AssetId>,
	<Runtime as pallet_assets::Config<ForeignAssetsPalletInstance>>::AssetIdParameter:
		From<AssetId> + Into<AssetId>,
	<Runtime as pallet_assets::Config<ForeignAssetsPalletInstance>>::Balance:
		From<Balance> + Into<u128>,
	<Runtime as frame_system::Config>::AccountId:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source:
		From<<Runtime as frame_system::Config>::AccountId>,
	ForeignAssetsPalletInstance: 'static,
	AssetId: Clone + Copy,
	AssetIdConverter: MaybeEquivalence<MultiLocation, AssetId>,
{
	// foreign parachain with the same consenus currency as asset
	let foreign_asset_id_multilocation =
		MultiLocation { parents: 1, interior: X2(Parachain(2222), GeneralIndex(1234567)) };
	let asset_id = AssetIdConverter::convert(&foreign_asset_id_multilocation).unwrap();

	// foreign creator, which can be sibling parachain to match ForeignCreators
	let foreign_creator = MultiLocation { parents: 1, interior: X1(Parachain(2222)) };
	let foreign_creator_as_account_id =
		SovereignAccountOf::convert_location(&foreign_creator).expect("");

	// we want to buy execution with local relay chain currency
	let buy_execution_fee_amount =
		WeightToFee::weight_to_fee(&Weight::from_parts(90_000_000_000, 0));
	let buy_execution_fee = MultiAsset {
		id: Concrete(MultiLocation::parent()),
		fun: Fungible(buy_execution_fee_amount),
	};

	const ASSET_NAME: &str = "My super coin";
	const ASSET_SYMBOL: &str = "MY_S_COIN";
	let metadata_deposit_per_byte_eta = metadata_deposit_per_byte
		.saturating_mul(((ASSET_NAME.len() + ASSET_SYMBOL.len()) as u128).into());

	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_keys.collators())
		.with_session_keys(collator_session_keys.session_keys())
		.with_balances(vec![(
			foreign_creator_as_account_id.clone(),
			existential_deposit +
				asset_deposit + metadata_deposit_base +
				metadata_deposit_per_byte_eta +
				buy_execution_fee_amount.into() +
				buy_execution_fee_amount.into(),
		)])
		.with_tracing()
		.build()
		.execute_with(|| {
			assert!(<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::asset_ids()
				.collect::<Vec<_>>()
				.is_empty());
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&foreign_creator_as_account_id),
				existential_deposit +
					asset_deposit + metadata_deposit_base +
					metadata_deposit_per_byte_eta +
					buy_execution_fee_amount.into() +
					buy_execution_fee_amount.into()
			);
			additional_checks_before();

			// execute XCM with Transacts to create/manage foreign assets by foreign governance
			// prepare data for xcm::Transact(create)
			let foreign_asset_create = runtime_call_encode(pallet_assets::Call::<
				Runtime,
				ForeignAssetsPalletInstance,
			>::create {
				id: asset_id.into(),
				// admin as sovereign_account
				admin: foreign_creator_as_account_id.clone().into(),
				min_balance: 1.into(),
			});
			// prepare data for xcm::Transact(set_metadata)
			let foreign_asset_set_metadata = runtime_call_encode(pallet_assets::Call::<
				Runtime,
				ForeignAssetsPalletInstance,
			>::set_metadata {
				id: asset_id.into(),
				name: Vec::from(ASSET_NAME),
				symbol: Vec::from(ASSET_SYMBOL),
				decimals: 12,
			});
			// prepare data for xcm::Transact(set_team - change just freezer to Bob)
			let foreign_asset_set_team = runtime_call_encode(pallet_assets::Call::<
				Runtime,
				ForeignAssetsPalletInstance,
			>::set_team {
				id: asset_id.into(),
				issuer: foreign_creator_as_account_id.clone().into(),
				admin: foreign_creator_as_account_id.clone().into(),
				freezer: bob_account.clone().into(),
			});

			// lets simulate this was triggered by relay chain from local consensus sibling parachain
			let xcm = Xcm(vec![
				WithdrawAsset(buy_execution_fee.clone().into()),
				BuyExecution { fees: buy_execution_fee.clone(), weight_limit: Unlimited },
				Transact {
					origin_kind: OriginKind::Xcm,
					require_weight_at_most: Weight::from_parts(40_000_000_000, 8000),
					call: foreign_asset_create.into(),
				},
				Transact {
					origin_kind: OriginKind::SovereignAccount,
					require_weight_at_most: Weight::from_parts(20_000_000_000, 8000),
					call: foreign_asset_set_metadata.into(),
				},
				Transact {
					origin_kind: OriginKind::SovereignAccount,
					require_weight_at_most: Weight::from_parts(20_000_000_000, 8000),
					call: foreign_asset_set_team.into(),
				},
				ExpectTransactStatus(MaybeErrorCode::Success),
			]);

			// messages with different consensus should go through the local bridge-hub
			let hash = xcm.using_encoded(sp_io::hashing::blake2_256);

			// execute xcm as XcmpQueue would do
			let outcome = XcmExecutor::<XcmConfig>::execute_xcm(
				foreign_creator,
				xcm,
				hash,
				RuntimeHelper::<Runtime>::xcm_max_weight(XcmReceivedFrom::Sibling),
			);
			assert_eq!(outcome.ensure_complete(), Ok(()));

			// check events
			let mut events = <frame_system::Pallet<Runtime>>::events()
				.into_iter()
				.filter_map(|e| unwrap_pallet_assets_event(e.event.encode()));
			assert!(events.any(|e| matches!(e, pallet_assets::Event::Created { .. })));
			assert!(events.any(|e| matches!(e, pallet_assets::Event::MetadataSet { .. })));
			assert!(events.any(|e| matches!(e, pallet_assets::Event::TeamChanged { .. })));

			// check assets after
			assert!(!<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::asset_ids()
				.collect::<Vec<_>>()
				.is_empty());

			// check update metadata
			use frame_support::traits::tokens::fungibles::roles::Inspect as InspectRoles;
			assert_eq!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::owner(
					asset_id.into()
				),
				Some(foreign_creator_as_account_id.clone())
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::admin(
					asset_id.into()
				),
				Some(foreign_creator_as_account_id.clone())
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::issuer(
					asset_id.into()
				),
				Some(foreign_creator_as_account_id.clone())
			);
			assert_eq!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::freezer(
					asset_id.into()
				),
				Some(bob_account.clone())
			);
			assert!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&foreign_creator_as_account_id) >=
					existential_deposit + buy_execution_fee_amount.into(),
				"Free balance: {:?} should be ge {:?}",
				<pallet_balances::Pallet<Runtime>>::free_balance(&foreign_creator_as_account_id),
				existential_deposit + buy_execution_fee_amount.into()
			);
			assert_metadata::<
				pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>,
				AccountIdOf<Runtime>,
			>(asset_id, ASSET_NAME, ASSET_SYMBOL, 12);

			// check if changed freezer, can freeze
			assert_noop!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::freeze(
					RuntimeHelper::<Runtime>::origin_of(bob_account),
					asset_id.into(),
					alice_account.clone().into()
				),
				pallet_assets::Error::<Runtime, ForeignAssetsPalletInstance>::NoAccount
			);
			assert_noop!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::freeze(
					RuntimeHelper::<Runtime>::origin_of(foreign_creator_as_account_id.clone()),
					asset_id.into(),
					alice_account.into()
				),
				pallet_assets::Error::<Runtime, ForeignAssetsPalletInstance>::NoPermission
			);

			// lets try create asset for different parachain(3333) (foreign_creator(2222) can create just his assets)
			let foreign_asset_id_multilocation =
				MultiLocation { parents: 1, interior: X2(Parachain(3333), GeneralIndex(1234567)) };
			let asset_id = AssetIdConverter::convert(&foreign_asset_id_multilocation).unwrap();

			// prepare data for xcm::Transact(create)
			let foreign_asset_create = runtime_call_encode(pallet_assets::Call::<
				Runtime,
				ForeignAssetsPalletInstance,
			>::create {
				id: asset_id.into(),
				// admin as sovereign_account
				admin: foreign_creator_as_account_id.clone().into(),
				min_balance: 1.into(),
			});
			let xcm = Xcm(vec![
				WithdrawAsset(buy_execution_fee.clone().into()),
				BuyExecution { fees: buy_execution_fee.clone(), weight_limit: Unlimited },
				Transact {
					origin_kind: OriginKind::Xcm,
					require_weight_at_most: Weight::from_parts(20_000_000_000, 8000),
					call: foreign_asset_create.into(),
				},
				ExpectTransactStatus(MaybeErrorCode::from(DispatchError::BadOrigin.encode())),
			]);

			// messages with different consensus should go through the local bridge-hub
			let hash = xcm.using_encoded(sp_io::hashing::blake2_256);

			// execute xcm as XcmpQueue would do
			let outcome = XcmExecutor::<XcmConfig>::execute_xcm(
				foreign_creator,
				xcm,
				hash,
				RuntimeHelper::<Runtime>::xcm_max_weight(XcmReceivedFrom::Sibling),
			);
			assert_eq!(outcome.ensure_complete(), Ok(()));

			additional_checks_after();
		})
}

#[macro_export]
macro_rules! include_create_and_manage_foreign_assets_for_local_consensus_parachain_assets_works(
	(
		$runtime:path,
		$xcm_config:path,
		$weight_to_fee:path,
		$sovereign_account_of:path,
		$assets_pallet_instance:path,
		$asset_id:path,
		$asset_id_converter:path,
		$collator_session_key:expr,
		$existential_deposit:expr,
		$asset_deposit:expr,
		$metadata_deposit_base:expr,
		$metadata_deposit_per_byte:expr,
		$runtime_call_encode:expr,
		$unwrap_pallet_assets_event:expr,
		$additional_checks_before:expr,
		$additional_checks_after:expr
	) => {
		#[test]
		fn create_and_manage_foreign_assets_for_local_consensus_parachain_assets_works() {
			const ALICE: [u8; 32] = [1u8; 32];
			let alice_account = parachains_common::AccountId::from(ALICE);
			const BOB: [u8; 32] = [2u8; 32];
			let bob_account = parachains_common::AccountId::from(BOB);

			$crate::test_cases::create_and_manage_foreign_assets_for_local_consensus_parachain_assets_works::<
				$runtime,
				$xcm_config,
				$weight_to_fee,
				$sovereign_account_of,
				$assets_pallet_instance,
				$asset_id,
				$asset_id_converter
			>(
				$collator_session_key,
				$existential_deposit,
				$asset_deposit,
				$metadata_deposit_base,
				$metadata_deposit_per_byte,
				alice_account,
				bob_account,
				$runtime_call_encode,
				$unwrap_pallet_assets_event,
				$additional_checks_before,
				$additional_checks_after
			)
		}
	}
);
