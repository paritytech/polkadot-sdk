// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use codec::Encode;
use frame_support::{
	assert_err, assert_ok,
	traits::{fungible::Mutate, OnFinalize, OnInitialize},
};
use frame_system::pallet_prelude::BlockNumberFor;
use parachains_runtimes_test_utils::{
	AccountIdOf, BalanceOf, CollatorSessionKeys, ExtBuilder, ValidatorIdOf, XcmReceivedFrom,
};
use snowbridge_core::{ChannelId, ParaId};
use snowbridge_pallet_ethereum_client_fixtures::*;
use sp_core::{Get, H160, U256};
use sp_keyring::AccountKeyring::*;
use sp_runtime::{traits::Header, AccountId32, DigestItem, SaturatedConversion, Saturating};
use xcm::latest::prelude::*;
use xcm_executor::XcmExecutor;

type RuntimeHelper<Runtime, AllPalletsWithoutSystem = ()> =
	parachains_runtimes_test_utils::RuntimeHelper<Runtime, AllPalletsWithoutSystem>;

pub fn initial_fund<Runtime>(assethub_parachain_id: u32, initial_amount: u128)
where
	Runtime: frame_system::Config + pallet_balances::Config,
{
	// fund asset hub sovereign account enough so it can pay fees
	let asset_hub_sovereign_account =
		snowbridge_core::sibling_sovereign_account::<Runtime>(assethub_parachain_id.into());
	<pallet_balances::Pallet<Runtime>>::mint_into(
		&asset_hub_sovereign_account,
		initial_amount.saturated_into::<BalanceOf<Runtime>>(),
	)
	.unwrap();
}

pub fn send_transfer_token_message<Runtime, XcmConfig>(
	ethereum_chain_id: u64,
	assethub_parachain_id: u32,
	weth_contract_address: H160,
	destination_address: H160,
	fee_amount: u128,
) -> Outcome
where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ snowbridge_pallet_outbound_queue::Config
		+ pallet_timestamp::Config,
	XcmConfig: xcm_executor::Config,
{
	let assethub_parachain_location = Location::new(1, Parachain(assethub_parachain_id));
	let asset = Asset {
		id: AssetId(Location::new(
			0,
			[AccountKey20 { network: None, key: weth_contract_address.into() }],
		)),
		fun: Fungible(1000000000),
	};
	let assets = vec![asset.clone()];

	let inner_xcm = Xcm(vec![
		WithdrawAsset(Assets::from(assets.clone())),
		ClearOrigin,
		BuyExecution { fees: asset, weight_limit: Unlimited },
		DepositAsset {
			assets: Wild(All),
			beneficiary: Location::new(
				0,
				[AccountKey20 { network: None, key: destination_address.into() }],
			),
		},
		SetTopic([0; 32]),
	]);

	let fee =
		Asset { id: AssetId(Location { parents: 1, interior: Here }), fun: Fungible(fee_amount) };

	// prepare transfer token message
	let xcm = Xcm(vec![
		WithdrawAsset(Assets::from(vec![fee.clone()])),
		BuyExecution { fees: fee, weight_limit: Unlimited },
		ExportMessage {
			network: Ethereum { chain_id: ethereum_chain_id },
			destination: Here,
			xcm: inner_xcm,
		},
	]);

	// execute XCM
	let mut hash = xcm.using_encoded(sp_io::hashing::blake2_256);
	XcmExecutor::<XcmConfig>::prepare_and_execute(
		assethub_parachain_location,
		xcm,
		&mut hash,
		RuntimeHelper::<Runtime>::xcm_max_weight(XcmReceivedFrom::Sibling),
		Weight::zero(),
	)
}

pub fn send_transfer_token_message_success<Runtime, XcmConfig>(
	ethereum_chain_id: u64,
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	assethub_parachain_id: u32,
	weth_contract_address: H160,
	destination_address: H160,
	fee_amount: u128,
	snowbridge_pallet_outbound_queue: Box<
		dyn Fn(Vec<u8>) -> Option<snowbridge_pallet_outbound_queue::Event<Runtime>>,
	>,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ pallet_message_queue::Config
		+ cumulus_pallet_parachain_system::Config
		+ snowbridge_pallet_outbound_queue::Config
		+ snowbridge_pallet_system::Config
		+ pallet_timestamp::Config,
	XcmConfig: xcm_executor::Config,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<Runtime as frame_system::Config>::AccountId: From<sp_runtime::AccountId32> + AsRef<[u8]>,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			<snowbridge_pallet_system::Pallet<Runtime>>::initialize(
				runtime_para_id.into(),
				assethub_parachain_id.into(),
			)
			.unwrap();

			// fund asset hub sovereign account enough so it can pay fees
			initial_fund::<Runtime>(assethub_parachain_id, 5_000_000_000_000);

			let outcome = send_transfer_token_message::<Runtime, XcmConfig>(
				ethereum_chain_id,
				assethub_parachain_id,
				weth_contract_address,
				destination_address,
				fee_amount,
			);

			assert_ok!(outcome.ensure_complete());

			// check events
			let mut events = <frame_system::Pallet<Runtime>>::events()
				.into_iter()
				.filter_map(|e| snowbridge_pallet_outbound_queue(e.event.encode()));
			assert!(events.any(|e| matches!(
				e,
				snowbridge_pallet_outbound_queue::Event::MessageQueued { .. }
			)));

			let block_number = <frame_system::Pallet<Runtime>>::block_number();
			let next_block_number = <frame_system::Pallet<Runtime>>::block_number()
				.saturating_add(BlockNumberFor::<Runtime>::from(1u32));

			// finish current block
			<pallet_message_queue::Pallet<Runtime>>::on_finalize(block_number);
			<snowbridge_pallet_outbound_queue::Pallet<Runtime>>::on_finalize(block_number);
			<frame_system::Pallet<Runtime>>::on_finalize(block_number);

			// start next block
			<frame_system::Pallet<Runtime>>::set_block_number(next_block_number);
			<frame_system::Pallet<Runtime>>::on_initialize(next_block_number);
			<snowbridge_pallet_outbound_queue::Pallet<Runtime>>::on_initialize(next_block_number);
			<pallet_message_queue::Pallet<Runtime>>::on_initialize(next_block_number);

			// finish next block
			<pallet_message_queue::Pallet<Runtime>>::on_finalize(next_block_number);
			<snowbridge_pallet_outbound_queue::Pallet<Runtime>>::on_finalize(next_block_number);
			let included_head = <frame_system::Pallet<Runtime>>::finalize();

			let origin: ParaId = assethub_parachain_id.into();
			let channel_id: ChannelId = origin.into();

			let nonce = snowbridge_pallet_outbound_queue::Nonce::<Runtime>::try_get(channel_id);
			assert_ok!(nonce);
			assert_eq!(nonce.unwrap(), 1);

			let digest = included_head.digest();

			let digest_items = digest.logs();
			assert!(digest_items.len() == 1 && digest_items[0].as_other().is_some());
		});
}

pub fn ethereum_outbound_queue_processes_messages_before_message_queue_works<
	Runtime,
	XcmConfig,
	AllPalletsWithoutSystem,
>(
	ethereum_chain_id: u64,
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	assethub_parachain_id: u32,
	weth_contract_address: H160,
	destination_address: H160,
	fee_amount: u128,
	snowbridge_pallet_outbound_queue: Box<
		dyn Fn(Vec<u8>) -> Option<snowbridge_pallet_outbound_queue::Event<Runtime>>,
	>,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ pallet_message_queue::Config
		+ cumulus_pallet_parachain_system::Config
		+ snowbridge_pallet_outbound_queue::Config
		+ snowbridge_pallet_system::Config
		+ pallet_timestamp::Config,
	XcmConfig: xcm_executor::Config,
	AllPalletsWithoutSystem:
		OnInitialize<BlockNumberFor<Runtime>> + OnFinalize<BlockNumberFor<Runtime>>,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<Runtime as frame_system::Config>::AccountId: From<sp_runtime::AccountId32> + AsRef<[u8]>,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			<snowbridge_pallet_system::Pallet<Runtime>>::initialize(
				runtime_para_id.into(),
				assethub_parachain_id.into(),
			)
			.unwrap();

			// fund asset hub sovereign account enough so it can pay fees
			initial_fund::<Runtime>(assethub_parachain_id, 5_000_000_000_000);

			let outcome = send_transfer_token_message::<Runtime, XcmConfig>(
				ethereum_chain_id,
				assethub_parachain_id,
				weth_contract_address,
				destination_address,
				fee_amount,
			);

			assert_ok!(outcome.ensure_complete());

			// check events
			let mut events = <frame_system::Pallet<Runtime>>::events()
				.into_iter()
				.filter_map(|e| snowbridge_pallet_outbound_queue(e.event.encode()));
			assert!(events.any(|e| matches!(
				e,
				snowbridge_pallet_outbound_queue::Event::MessageQueued { .. }
			)));

			let next_block_number: U256 = <frame_system::Pallet<Runtime>>::block_number()
				.saturating_add(BlockNumberFor::<Runtime>::from(1u32))
				.into();

			let included_head =
				RuntimeHelper::<Runtime, AllPalletsWithoutSystem>::run_to_block_with_finalize(
					next_block_number.as_u32(),
				);
			let digest = included_head.digest();
			let digest_items = digest.logs();

			let mut found_outbound_digest = false;
			for digest_item in digest_items {
				match digest_item {
					DigestItem::Other(_) => found_outbound_digest = true,
					_ => {},
				}
			}

			assert_eq!(found_outbound_digest, true);
		});
}

pub fn send_unpaid_transfer_token_message<Runtime, XcmConfig>(
	ethereum_chain_id: u64,
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	assethub_parachain_id: u32,
	weth_contract_address: H160,
	destination_contract: H160,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ snowbridge_pallet_outbound_queue::Config
		+ pallet_timestamp::Config,
	XcmConfig: xcm_executor::Config,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
{
	let assethub_parachain_location = Location::new(1, Parachain(assethub_parachain_id));

	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			let asset_hub_sovereign_account =
				snowbridge_core::sibling_sovereign_account::<Runtime>(assethub_parachain_id.into());

			<pallet_balances::Pallet<Runtime>>::mint_into(
				&asset_hub_sovereign_account,
				4000000000u32.into(),
			)
			.unwrap();

			let asset = Asset {
				id: AssetId(Location::new(
					0,
					[AccountKey20 { network: None, key: weth_contract_address.into() }],
				)),
				fun: Fungible(1000000000),
			};
			let assets = vec![asset.clone()];

			let inner_xcm = Xcm(vec![
				WithdrawAsset(Assets::from(assets.clone())),
				ClearOrigin,
				BuyExecution { fees: asset, weight_limit: Unlimited },
				DepositAsset {
					assets: Wild(AllCounted(1)),
					beneficiary: Location::new(
						0,
						[AccountKey20 { network: None, key: destination_contract.into() }],
					),
				},
				SetTopic([0; 32]),
			]);

			// prepare transfer token message
			let xcm = Xcm(vec![
				UnpaidExecution { weight_limit: Unlimited, check_origin: None },
				ExportMessage {
					network: Ethereum { chain_id: ethereum_chain_id },
					destination: Here,
					xcm: inner_xcm,
				},
			]);

			// execute XCM
			let mut hash = xcm.using_encoded(sp_io::hashing::blake2_256);
			let outcome = XcmExecutor::<XcmConfig>::prepare_and_execute(
				assethub_parachain_location,
				xcm,
				&mut hash,
				RuntimeHelper::<Runtime>::xcm_max_weight(XcmReceivedFrom::Sibling),
				Weight::zero(),
			);
			// check error is barrier
			assert_err!(outcome.ensure_complete(), XcmError::Barrier);
		});
}

#[allow(clippy::too_many_arguments)]
pub fn send_transfer_token_message_failure<Runtime, XcmConfig>(
	ethereum_chain_id: u64,
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	assethub_parachain_id: u32,
	initial_amount: u128,
	weth_contract_address: H160,
	destination_address: H160,
	fee_amount: u128,
	expected_error: XcmError,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ snowbridge_pallet_outbound_queue::Config
		+ snowbridge_pallet_system::Config
		+ pallet_timestamp::Config,
	XcmConfig: xcm_executor::Config,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			<snowbridge_pallet_system::Pallet<Runtime>>::initialize(
				runtime_para_id.into(),
				assethub_parachain_id.into(),
			)
			.unwrap();

			// fund asset hub sovereign account enough so it can pay fees
			initial_fund::<Runtime>(assethub_parachain_id, initial_amount);

			let outcome = send_transfer_token_message::<Runtime, XcmConfig>(
				ethereum_chain_id,
				assethub_parachain_id,
				weth_contract_address,
				destination_address,
				fee_amount,
			);
			assert_err!(outcome.ensure_complete(), expected_error);
		});
}

pub fn ethereum_extrinsic<Runtime>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	construct_and_apply_extrinsic: fn(
		sp_keyring::AccountKeyring,
		<Runtime as frame_system::Config>::RuntimeCall,
	) -> sp_runtime::DispatchOutcome,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ pallet_utility::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ snowbridge_pallet_outbound_queue::Config
		+ snowbridge_pallet_system::Config
		+ snowbridge_pallet_ethereum_client::Config
		+ pallet_timestamp::Config,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<Runtime as pallet_utility::Config>::RuntimeCall:
		From<snowbridge_pallet_ethereum_client::Call<Runtime>>,
	<Runtime as frame_system::Config>::RuntimeCall: From<pallet_utility::Call<Runtime>>,
	AccountIdOf<Runtime>: From<AccountId32>,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			let initial_checkpoint = make_checkpoint();
			let update = make_finalized_header_update();
			let sync_committee_update = make_sync_committee_update();
			let mut invalid_update = make_finalized_header_update();
			let mut invalid_sync_committee_update = make_sync_committee_update();
			invalid_update.finalized_header.slot = 4354;
			invalid_sync_committee_update.finalized_header.slot = 4354;

			let alice = Alice;
			let alice_account = alice.to_account_id();
			<pallet_balances::Pallet<Runtime>>::mint_into(
				&alice_account.clone().into(),
				10_000_000_000_000_u128.saturated_into::<BalanceOf<Runtime>>(),
			)
			.unwrap();
			let balance_before =
				<pallet_balances::Pallet<Runtime>>::free_balance(&alice_account.clone().into());

			assert_ok!(<snowbridge_pallet_ethereum_client::Pallet<Runtime>>::force_checkpoint(
				RuntimeHelper::<Runtime>::root_origin(),
				initial_checkpoint.clone(),
			));
			let balance_after_checkpoint =
				<pallet_balances::Pallet<Runtime>>::free_balance(&alice_account.clone().into());

			let update_call: <Runtime as pallet_utility::Config>::RuntimeCall =
				snowbridge_pallet_ethereum_client::Call::<Runtime>::submit {
					update: Box::new(*update.clone()),
				}
				.into();

			let invalid_update_call: <Runtime as pallet_utility::Config>::RuntimeCall =
				snowbridge_pallet_ethereum_client::Call::<Runtime>::submit {
					update: Box::new(*invalid_update),
				}
				.into();

			let update_sync_committee_call: <Runtime as pallet_utility::Config>::RuntimeCall =
				snowbridge_pallet_ethereum_client::Call::<Runtime>::submit {
					update: Box::new(*sync_committee_update),
				}
				.into();

			let invalid_update_sync_committee_call: <Runtime as pallet_utility::Config>::RuntimeCall =
				snowbridge_pallet_ethereum_client::Call::<Runtime>::submit {
					update: Box::new(*invalid_sync_committee_update),
				}
					.into();

			// Finalized header update
			let update_outcome = construct_and_apply_extrinsic(alice, update_call.into());
			assert_ok!(update_outcome);
			let balance_after_update =
				<pallet_balances::Pallet<Runtime>>::free_balance(&alice_account.clone().into());

			// Invalid finalized header update
			let invalid_update_outcome =
				construct_and_apply_extrinsic(alice, invalid_update_call.into());
			assert_err!(
				invalid_update_outcome,
				snowbridge_pallet_ethereum_client::Error::<Runtime>::InvalidUpdateSlot
			);
			let balance_after_invalid_update =
				<pallet_balances::Pallet<Runtime>>::free_balance(&alice_account.clone().into());

			// Sync committee update
			let sync_committee_outcome =
				construct_and_apply_extrinsic(alice, update_sync_committee_call.into());
			assert_ok!(sync_committee_outcome);
			let balance_after_sync_com_update =
				<pallet_balances::Pallet<Runtime>>::free_balance(&alice_account.clone().into());

			// Invalid sync committee update
			let invalid_sync_committee_outcome =
				construct_and_apply_extrinsic(alice, invalid_update_sync_committee_call.into());
			assert_err!(
				invalid_sync_committee_outcome,
				snowbridge_pallet_ethereum_client::Error::<Runtime>::InvalidUpdateSlot
			);
			let balance_after_invalid_sync_com_update =
				<pallet_balances::Pallet<Runtime>>::free_balance(&alice_account.clone().into());

			// Assert paid operations are charged and free operations are free
			// Checkpoint is a free operation
			assert!(balance_before == balance_after_checkpoint);
			let gap =
				<Runtime as snowbridge_pallet_ethereum_client::Config>::FreeHeadersInterval::get();
			// Large enough header gap is free
			if update.finalized_header.slot >= initial_checkpoint.header.slot + gap as u64 {
				assert!(balance_after_checkpoint == balance_after_update);
			} else {
				// Otherwise paid
				assert!(balance_after_checkpoint > balance_after_update);
			}
			// An invalid update is paid
			assert!(balance_after_update > balance_after_invalid_update);
			// A successful sync committee update is free
			assert!(balance_after_invalid_update == balance_after_sync_com_update);
			// An invalid sync committee update is paid
			assert!(balance_after_sync_com_update > balance_after_invalid_sync_com_update);
		});
}

pub fn ethereum_to_polkadot_message_extrinsics_work<Runtime>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	construct_and_apply_extrinsic: fn(
		sp_keyring::AccountKeyring,
		<Runtime as frame_system::Config>::RuntimeCall,
	) -> sp_runtime::DispatchOutcome,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ pallet_utility::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ snowbridge_pallet_outbound_queue::Config
		+ snowbridge_pallet_system::Config
		+ snowbridge_pallet_ethereum_client::Config
		+ pallet_timestamp::Config,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	<Runtime as pallet_utility::Config>::RuntimeCall:
		From<snowbridge_pallet_ethereum_client::Call<Runtime>>,
	<Runtime as frame_system::Config>::RuntimeCall: From<pallet_utility::Call<Runtime>>,
	AccountIdOf<Runtime>: From<AccountId32>,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			let initial_checkpoint = make_checkpoint();
			let sync_committee_update = make_sync_committee_update();

			let alice = Alice;
			let alice_account = alice.to_account_id();
			<pallet_balances::Pallet<Runtime>>::mint_into(
				&alice_account.into(),
				10_000_000_000_000_u128.saturated_into::<BalanceOf<Runtime>>(),
			)
			.unwrap();

			assert_ok!(<snowbridge_pallet_ethereum_client::Pallet<Runtime>>::force_checkpoint(
				RuntimeHelper::<Runtime>::root_origin(),
				initial_checkpoint,
			));

			let update_sync_committee_call: <Runtime as pallet_utility::Config>::RuntimeCall =
				snowbridge_pallet_ethereum_client::Call::<Runtime>::submit {
					update: Box::new(*sync_committee_update),
				}
				.into();

			let sync_committee_outcome =
				construct_and_apply_extrinsic(alice, update_sync_committee_call.into());
			assert_ok!(sync_committee_outcome);
		});
}
