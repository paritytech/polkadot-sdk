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

//! Module contains predefined test-case scenarios for `Runtime` with various assets transferred
//! over a bridge.

use crate::{assert_matches_reserve_asset_deposited_instructions, get_fungible_delivery_fees};
use codec::Encode;
use cumulus_primitives_core::XcmpMessageSource;
use frame_support::{
	assert_ok,
	traits::{Currency, Get, OnFinalize, OnInitialize, OriginTrait, ProcessMessageError},
};
use frame_system::pallet_prelude::BlockNumberFor;
use parachains_common::{AccountId, Balance};
use parachains_runtimes_test_utils::{
	mock_open_hrmp_channel, AccountIdOf, BalanceOf, CollatorSessionKeys, ExtBuilder, RuntimeHelper,
	SlotDurations, ValidatorIdOf, XcmReceivedFrom,
};
use sp_runtime::{traits::StaticLookup, Saturating};
use xcm::{latest::prelude::*, VersionedAssets};
use xcm_builder::{CreateMatcher, MatchXcm};
use xcm_executor::{traits::ConvertLocation, XcmExecutor};

pub struct TestBridgingConfig {
	pub bridged_network: NetworkId,
	pub local_bridge_hub_para_id: u32,
	pub local_bridge_hub_location: Location,
	pub bridged_target_location: Location,
}

/// Test-case makes sure that `Runtime` can initiate **reserve transfer assets** over bridge.
pub fn limited_reserve_transfer_assets_for_native_asset_works<
	Runtime,
	AllPalletsWithoutSystem,
	XcmConfig,
	HrmpChannelOpener,
	HrmpChannelSource,
	LocationToAccountId,
>(
	collator_session_keys: CollatorSessionKeys<Runtime>,
	slot_durations: SlotDurations,
	existential_deposit: BalanceOf<Runtime>,
	alice_account: AccountIdOf<Runtime>,
	unwrap_pallet_xcm_event: Box<dyn Fn(Vec<u8>) -> Option<pallet_xcm::Event<Runtime>>>,
	unwrap_xcmp_queue_event: Box<
		dyn Fn(Vec<u8>) -> Option<cumulus_pallet_xcmp_queue::Event<Runtime>>,
	>,
	prepare_configuration: fn() -> TestBridgingConfig,
	weight_limit: WeightLimit,
	maybe_paid_export_message: Option<AssetId>,
	delivery_fees_account: Option<AccountIdOf<Runtime>>,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ cumulus_pallet_xcmp_queue::Config
		+ pallet_timestamp::Config,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	AccountIdOf<Runtime>: Into<[u8; 32]>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	BalanceOf<Runtime>: From<Balance>,
	<Runtime as pallet_balances::Config>::Balance: From<Balance> + Into<u128>,
	XcmConfig: xcm_executor::Config,
	LocationToAccountId: ConvertLocation<AccountIdOf<Runtime>>,
	<Runtime as frame_system::Config>::AccountId:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source:
		From<<Runtime as frame_system::Config>::AccountId>,
	<Runtime as frame_system::Config>::AccountId: From<AccountId>,
	HrmpChannelOpener: frame_support::inherent::ProvideInherent<
		Call = cumulus_pallet_parachain_system::Call<Runtime>,
	>,
	HrmpChannelSource: XcmpMessageSource,
{
	let runtime_para_id = 1000;
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_keys.collators())
		.with_session_keys(collator_session_keys.session_keys())
		.with_tracing()
		.with_safe_xcm_version(3)
		.with_para_id(runtime_para_id.into())
		.build()
		.execute_with(|| {
			let mut alice = [0u8; 32];
			alice[0] = 1;
			let included_head = RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::run_to_block(
				2,
				AccountId::from(alice).into(),
			);

			// prepare bridge config
			let TestBridgingConfig {
				bridged_network,
				local_bridge_hub_para_id,
				bridged_target_location: target_location_from_different_consensus,
				..
			} = prepare_configuration();

			let reserve_account =
				LocationToAccountId::convert_location(&target_location_from_different_consensus)
					.expect("Sovereign account for reserves");
			let balance_to_transfer = 1_000_000_000_000_u128;
			let native_asset = Location::parent();

			// open HRMP to bridge hub
			mock_open_hrmp_channel::<Runtime, HrmpChannelOpener>(
				runtime_para_id.into(),
				local_bridge_hub_para_id.into(),
				included_head,
				&alice,
				&slot_durations,
			);

			// we calculate exact delivery fees _after_ sending the message by weighing the sent
			// xcm, and this delivery fee varies for different runtimes, so just add enough buffer,
			// then verify the arithmetics check out on final balance.
			let delivery_fees_buffer = 8_000_000_000_000u128;
			// drip ED + transfer_amount + delivery_fees_buffer to Alice account
			let alice_account_init_balance =
				existential_deposit + balance_to_transfer.into() + delivery_fees_buffer.into();
			let _ = <pallet_balances::Pallet<Runtime>>::deposit_creating(
				&alice_account,
				alice_account_init_balance,
			);
			// SA of target location needs to have at least ED, otherwise making reserve fails
			let _ = <pallet_balances::Pallet<Runtime>>::deposit_creating(
				&reserve_account,
				existential_deposit,
			);

			// we just check here, that user retains enough balance after withdrawal
			// and also we check if `balance_to_transfer` is more than `existential_deposit`,
			assert!(
				(<pallet_balances::Pallet<Runtime>>::free_balance(&alice_account) -
					balance_to_transfer.into()) >=
					existential_deposit
			);
			// SA has just ED
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&reserve_account),
				existential_deposit
			);

			let delivery_fees_account_balance_before = delivery_fees_account
				.as_ref()
				.map(|dfa| <pallet_balances::Pallet<Runtime>>::free_balance(dfa))
				.unwrap_or(0.into());

			// local native asset (pallet_balances)
			let asset_to_transfer =
				Asset { fun: Fungible(balance_to_transfer.into()), id: native_asset.into() };

			// destination is (some) account relative to the destination different consensus
			let target_destination_account = Location::new(
				0,
				[AccountId32 {
					network: Some(bridged_network),
					id: sp_runtime::AccountId32::new([3; 32]).into(),
				}],
			);

			let assets_to_transfer = Assets::from(asset_to_transfer);
			let mut expected_assets = assets_to_transfer.clone();
			let context = XcmConfig::UniversalLocation::get();
			expected_assets
				.reanchor(&target_location_from_different_consensus, &context)
				.unwrap();

			let expected_beneficiary = target_destination_account.clone();

			// do pallet_xcm call reserve transfer
			assert_ok!(<pallet_xcm::Pallet<Runtime>>::limited_reserve_transfer_assets(
				RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::origin_of(alice_account.clone()),
				Box::new(target_location_from_different_consensus.clone().into_versioned()),
				Box::new(target_destination_account.into_versioned()),
				Box::new(VersionedAssets::from(assets_to_transfer)),
				0,
				weight_limit,
			));

			// check events
			// check pallet_xcm attempted
			RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::assert_pallet_xcm_event_outcome(
				&unwrap_pallet_xcm_event,
				|outcome| {
					assert_ok!(outcome.ensure_complete());
				},
			);

			// check that xcm was sent
			let xcm_sent_message_hash = <frame_system::Pallet<Runtime>>::events()
				.into_iter()
				.filter_map(|e| unwrap_xcmp_queue_event(e.event.encode()))
				.find_map(|e| match e {
					cumulus_pallet_xcmp_queue::Event::XcmpMessageSent { message_hash } =>
						Some(message_hash),
					_ => None,
				});

			// read xcm
			let xcm_sent = RuntimeHelper::<HrmpChannelSource, AllPalletsWithoutSystem>::take_xcm(
				local_bridge_hub_para_id.into(),
			)
			.unwrap();
			assert_eq!(
				xcm_sent_message_hash,
				Some(xcm_sent.using_encoded(sp_io::hashing::blake2_256))
			);
			let mut xcm_sent: Xcm<()> = xcm_sent.try_into().expect("versioned xcm");

			// check sent XCM ExportMessage to BridgeHub

			let mut delivery_fees = 0;
			// 1. check paid or unpaid
			if let Some(expected_fee_asset_id) = maybe_paid_export_message {
				xcm_sent
					.0
					.matcher()
					.match_next_inst(|instr| match instr {
						WithdrawAsset(_) => Ok(()),
						_ => Err(ProcessMessageError::BadFormat),
					})
					.expect("contains WithdrawAsset")
					.match_next_inst(|instr| match instr {
						BuyExecution { fees, .. } if fees.id.eq(&expected_fee_asset_id) => Ok(()),
						_ => Err(ProcessMessageError::BadFormat),
					})
					.expect("contains BuyExecution")
					.match_next_inst(|instr| match instr {
						SetAppendix(_) => Ok(()),
						_ => Err(ProcessMessageError::BadFormat),
					})
					.expect("contains SetAppendix")
			} else {
				xcm_sent
					.0
					.matcher()
					.match_next_inst(|instr| match instr {
						// first instruction could be UnpaidExecution (because we could have
						// explicit unpaid execution on BridgeHub)
						UnpaidExecution { weight_limit, check_origin }
							if weight_limit == &Unlimited && check_origin.is_none() =>
							Ok(()),
						_ => Err(ProcessMessageError::BadFormat),
					})
					.expect("contains UnpaidExecution")
			}
			// 2. check ExportMessage
			.match_next_inst(|instr| match instr {
				// next instruction is ExportMessage
				ExportMessage { network, destination, xcm: inner_xcm } => {
					assert_eq!(network, &bridged_network);
					let (_, target_location_junctions_without_global_consensus) =
						target_location_from_different_consensus
							.interior
							.clone()
							.split_global()
							.expect("split works");
					assert_eq!(destination, &target_location_junctions_without_global_consensus);
					// Call `SendXcm::validate` to get delivery fees.
					delivery_fees = get_fungible_delivery_fees::<
						<XcmConfig as xcm_executor::Config>::XcmSender,
					>(
						target_location_from_different_consensus.clone(),
						inner_xcm.clone(),
					);
					assert_matches_reserve_asset_deposited_instructions(
						inner_xcm,
						&expected_assets,
						&expected_beneficiary,
					);
					Ok(())
				},
				_ => Err(ProcessMessageError::BadFormat),
			})
			.expect("contains ExportMessage");

			// check alice account decreased by balance_to_transfer
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&alice_account),
				alice_account_init_balance
					.saturating_sub(balance_to_transfer.into())
					.saturating_sub(delivery_fees.into())
			);

			// check reserve account increased by balance_to_transfer
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&reserve_account),
				existential_deposit + balance_to_transfer.into()
			);

			// check dedicated account increased by delivery fees (if configured)
			if let Some(delivery_fees_account) = delivery_fees_account {
				let delivery_fees_account_balance_after =
					<pallet_balances::Pallet<Runtime>>::free_balance(&delivery_fees_account);
				assert!(
					delivery_fees_account_balance_after - delivery_fees.into() >=
						delivery_fees_account_balance_before
				);
			}
		})
}

pub fn receive_reserve_asset_deposited_from_different_consensus_works<
	Runtime,
	AllPalletsWithoutSystem,
	XcmConfig,
	ForeignAssetsPalletInstance,
>(
	collator_session_keys: CollatorSessionKeys<Runtime>,
	existential_deposit: BalanceOf<Runtime>,
	target_account: AccountIdOf<Runtime>,
	block_author_account: AccountIdOf<Runtime>,
	(foreign_asset_owner, foreign_asset_id_location, foreign_asset_id_minimum_balance): (
		AccountIdOf<Runtime>,
		xcm::v5::Location,
		u128,
	),
	foreign_asset_id_amount_to_transfer: u128,
	prepare_configuration: impl FnOnce() -> TestBridgingConfig,
	(bridge_instance, universal_origin, descend_origin): (Junctions, Junction, Junctions), /* bridge adds origin manipulation on the way */
	additional_checks_before: impl FnOnce(),
	additional_checks_after: impl FnOnce(),
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ cumulus_pallet_xcmp_queue::Config
		+ pallet_assets::Config<ForeignAssetsPalletInstance>
		+ pallet_timestamp::Config,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	AccountIdOf<Runtime>: Into<[u8; 32]> + From<[u8; 32]>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	BalanceOf<Runtime>: From<Balance> + Into<Balance>,
	XcmConfig: xcm_executor::Config,
	<Runtime as pallet_assets::Config<ForeignAssetsPalletInstance>>::AssetId:
		From<xcm::v5::Location> + Into<xcm::v5::Location>,
	<Runtime as pallet_assets::Config<ForeignAssetsPalletInstance>>::AssetIdParameter:
		From<xcm::v5::Location> + Into<xcm::v5::Location>,
	<Runtime as pallet_assets::Config<ForeignAssetsPalletInstance>>::Balance:
		From<Balance> + Into<u128> + From<u128>,
	<Runtime as frame_system::Config>::AccountId: Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>
		+ Into<AccountId>,
	<<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source:
		From<<Runtime as frame_system::Config>::AccountId>,
	ForeignAssetsPalletInstance: 'static,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_keys.collators())
		.with_session_keys(collator_session_keys.session_keys())
		.with_tracing()
		.build()
		.execute_with(|| {
			// Set account as block author, who will receive fees
			RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::run_to_block(
				2,
				block_author_account.clone().into(),
			);

			// drip 'ED' user target account
			let _ = <pallet_balances::Pallet<Runtime>>::deposit_creating(
				&target_account,
				existential_deposit,
			);

			// create foreign asset for wrapped/derived representation
			assert_ok!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::force_create(
					RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::root_origin(),
					foreign_asset_id_location.clone().into(),
					foreign_asset_owner.into(),
					true, // is_sufficient=true
					foreign_asset_id_minimum_balance.into()
				)
			);

			// prepare bridge config
			let TestBridgingConfig { local_bridge_hub_location, .. } = prepare_configuration();

			// Balances before
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&target_account),
				existential_deposit.clone()
			);

			// ForeignAssets balances before
			assert_eq!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::balance(
					foreign_asset_id_location.clone().into(),
					&target_account
				),
				0.into()
			);

			// additional check before
			additional_checks_before();

			let expected_assets = Assets::from(vec![Asset {
				id: AssetId(foreign_asset_id_location.clone()),
				fun: Fungible(foreign_asset_id_amount_to_transfer),
			}]);
			let expected_beneficiary = Location::new(
				0,
				[AccountId32 { network: None, id: target_account.clone().into() }],
			);

			// Call received XCM execution
			let xcm = Xcm(vec![
				DescendOrigin(bridge_instance),
				UniversalOrigin(universal_origin),
				DescendOrigin(descend_origin),
				ReserveAssetDeposited(expected_assets.clone()),
				ClearOrigin,
				BuyExecution {
					fees: Asset {
						id: AssetId(foreign_asset_id_location.clone()),
						fun: Fungible(foreign_asset_id_amount_to_transfer),
					},
					weight_limit: Unlimited,
				},
				DepositAsset {
					assets: Wild(AllCounted(1)),
					beneficiary: expected_beneficiary.clone(),
				},
				SetTopic([
					220, 188, 144, 32, 213, 83, 111, 175, 44, 210, 111, 19, 90, 165, 191, 112, 140,
					247, 192, 124, 42, 17, 153, 141, 114, 34, 189, 20, 83, 69, 237, 173,
				]),
			]);
			assert_matches_reserve_asset_deposited_instructions(
				&mut xcm.clone(),
				&expected_assets,
				&expected_beneficiary,
			);

			let mut hash = xcm.using_encoded(sp_io::hashing::blake2_256);

			// execute xcm as XcmpQueue would do
			let outcome = XcmExecutor::<XcmConfig>::prepare_and_execute(
				local_bridge_hub_location,
				xcm,
				&mut hash,
				RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::xcm_max_weight(
					XcmReceivedFrom::Sibling,
				),
				Weight::zero(),
			);
			assert_ok!(outcome.ensure_complete());

			// Balances after
			assert_eq!(
				<pallet_balances::Pallet<Runtime>>::free_balance(&target_account),
				existential_deposit.clone()
			);

			// ForeignAssets balances after
			assert!(
				<pallet_assets::Pallet<Runtime, ForeignAssetsPalletInstance>>::balance(
					foreign_asset_id_location.into(),
					&target_account
				) > 0.into()
			);

			// additional check after
			additional_checks_after();
		})
}

pub fn report_bridge_status_from_xcm_bridge_router_works<
	Runtime,
	AllPalletsWithoutSystem,
	XcmConfig,
	LocationToAccountId,
	XcmBridgeHubRouterInstance,
>(
	collator_session_keys: CollatorSessionKeys<Runtime>,
	prepare_configuration: fn() -> TestBridgingConfig,
	congested_message: fn() -> Xcm<XcmConfig::RuntimeCall>,
	uncongested_message: fn() -> Xcm<XcmConfig::RuntimeCall>,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ cumulus_pallet_xcmp_queue::Config
		+ pallet_xcm_bridge_hub_router::Config<XcmBridgeHubRouterInstance>
		+ pallet_timestamp::Config,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	AccountIdOf<Runtime>: Into<[u8; 32]>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	BalanceOf<Runtime>: From<Balance>,
	<Runtime as pallet_balances::Config>::Balance: From<Balance> + Into<u128>,
	XcmConfig: xcm_executor::Config,
	LocationToAccountId: ConvertLocation<AccountIdOf<Runtime>>,
	<Runtime as frame_system::Config>::AccountId:
		Into<<<Runtime as frame_system::Config>::RuntimeOrigin as OriginTrait>::AccountId>,
	<<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source:
		From<<Runtime as frame_system::Config>::AccountId>,
	<Runtime as frame_system::Config>::AccountId: From<AccountId>,
	XcmBridgeHubRouterInstance: 'static,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_keys.collators())
		.with_session_keys(collator_session_keys.session_keys())
		.with_tracing()
		.build()
		.execute_with(|| {
			let report_bridge_status = |is_congested: bool| {
				// prepare bridge config
				let TestBridgingConfig { local_bridge_hub_location, .. } = prepare_configuration();

				// Call received XCM execution
				let xcm = if is_congested { congested_message() } else { uncongested_message() };
				let mut hash = xcm.using_encoded(sp_io::hashing::blake2_256);

				// execute xcm as XcmpQueue would do
				let outcome = XcmExecutor::<XcmConfig>::prepare_and_execute(
					local_bridge_hub_location.clone(),
					xcm,
					&mut hash,
					RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::xcm_max_weight(
						XcmReceivedFrom::Sibling,
					),
					Weight::zero(),
				);
				assert_ok!(outcome.ensure_complete());
				assert_eq!(
					is_congested,
					<<Runtime as pallet_xcm_bridge_hub_router::Config<XcmBridgeHubRouterInstance>>::LocalXcmChannelManager as pallet_xcm_bridge_hub_router::XcmChannelStatusProvider>::is_congested(&local_bridge_hub_location)
				);
			};

			report_bridge_status(true);
			report_bridge_status(false);
		})
}
