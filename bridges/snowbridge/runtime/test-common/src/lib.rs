// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>

use codec::Encode;
use frame_support::{
	assert_err, assert_ok,
	traits::{fungible::Mutate, OnFinalize, OnInitialize},
};
use frame_system::pallet_prelude::BlockNumberFor;
pub use parachains_runtimes_test_utils::test_cases::change_storage_constant_by_governance_works;
use parachains_runtimes_test_utils::{
	AccountIdOf, BalanceOf, CollatorSessionKeys, ExtBuilder, ValidatorIdOf, XcmReceivedFrom,
};
use snowbridge_core::{ChannelId, ParaId};
use snowbridge_pallet_ethereum_client_fixtures::*;
use sp_core::H160;
use sp_keyring::AccountKeyring::*;
use sp_runtime::{traits::Header, AccountId32, SaturatedConversion, Saturating};
use xcm::{
	latest::prelude::*,
	v3::Error::{self, Barrier},
};
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
		+ snowbridge_pallet_outbound_queue::Config,
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
			network: Ethereum { chain_id: 11155111 },
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
		+ snowbridge_pallet_system::Config,
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

			//let digest = frame_system::Pallet::<Runtime>::digest();
			let digest_items = digest.logs();
			assert!(digest_items.len() == 1 && digest_items[0].as_other().is_some());
		});
}

pub fn send_unpaid_transfer_token_message<Runtime, XcmConfig>(
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
		+ snowbridge_pallet_outbound_queue::Config,
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
					network: Ethereum { chain_id: 11155111 },
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
			assert_err!(outcome.ensure_complete(), Barrier);
		});
}

#[allow(clippy::too_many_arguments)]
pub fn send_transfer_token_message_failure<Runtime, XcmConfig>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	assethub_parachain_id: u32,
	initial_amount: u128,
	weth_contract_address: H160,
	destination_address: H160,
	fee_amount: u128,
	expected_error: Error,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ snowbridge_pallet_outbound_queue::Config
		+ snowbridge_pallet_system::Config,
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
		+ snowbridge_pallet_ethereum_client::Config,
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
			let execution_header_update = make_execution_header_update();

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

			let update_call: <Runtime as pallet_utility::Config>::RuntimeCall =
				snowbridge_pallet_ethereum_client::Call::<Runtime>::submit {
					update: Box::new(*update),
				}
				.into();

			let update_sync_committee_call: <Runtime as pallet_utility::Config>::RuntimeCall =
				snowbridge_pallet_ethereum_client::Call::<Runtime>::submit {
					update: Box::new(*sync_committee_update),
				}
				.into();

			let execution_header_call: <Runtime as pallet_utility::Config>::RuntimeCall =
				snowbridge_pallet_ethereum_client::Call::<Runtime>::submit_execution_header {
					update: Box::new(*execution_header_update),
				}
				.into();

			let update_outcome = construct_and_apply_extrinsic(alice, update_call.into());
			assert_ok!(update_outcome);

			let sync_committee_outcome =
				construct_and_apply_extrinsic(alice, update_sync_committee_call.into());
			assert_ok!(sync_committee_outcome);

			let execution_header_outcome =
				construct_and_apply_extrinsic(alice, execution_header_call.into());
			assert_ok!(execution_header_outcome);
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
		+ snowbridge_pallet_ethereum_client::Config,
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
			let execution_header_update = make_execution_header_update();

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

			let execution_header_call: <Runtime as pallet_utility::Config>::RuntimeCall =
				snowbridge_pallet_ethereum_client::Call::<Runtime>::submit_execution_header {
					update: Box::new(*execution_header_update),
				}
				.into();

			let sync_committee_outcome =
				construct_and_apply_extrinsic(alice, update_sync_committee_call.into());
			assert_ok!(sync_committee_outcome);

			let execution_header_outcome =
				construct_and_apply_extrinsic(alice, execution_header_call.into());
			assert_ok!(execution_header_outcome);
		});
}
