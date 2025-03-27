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

//! Module contains predefined test-case scenarios for `Runtime` with common functionality.

use crate::{
	AccountIdOf, BasicParachainRuntime, CollatorSessionKeys, ExtBuilder, GovernanceOrigin,
	RuntimeCallOf, RuntimeOriginOf, ValidatorIdOf,
};
use codec::Encode;
use frame_support::{
	assert_err_ignore_postinfo, assert_ok,
	traits::{fungible::Inspect, Get, OriginTrait},
};
use parachains_common::{AccountId, Balance};
use sp_runtime::{
	traits::{Block as BlockT, StaticLookup},
	DispatchError, Either,
};
use xcm::{
	latest::{AssetId, Assets, Location, Weight, Xcm},
	prelude::{All, DepositAsset, ExchangeAsset, Parachain, Wild, WithdrawAsset, XcmError},
};
use xcm_runtime_apis::fees::{
	runtime_decl_for_xcm_payment_api::XcmPaymentApiV1, Error as XcmPaymentApiError,
};

type RuntimeHelper<Runtime, AllPalletsWithoutSystem = ()> =
	crate::RuntimeHelper<Runtime, AllPalletsWithoutSystem>;

/// Test-case makes sure that `Runtime` can change storage constant via governance-like call
pub fn change_storage_constant_by_governance_works<Runtime, StorageConstant, StorageConstantType>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	governance_origin: GovernanceOrigin<RuntimeOriginOf<Runtime>>,
	storage_constant_key_value: fn() -> (Vec<u8>, StorageConstantType),
	new_storage_constant_value: fn(&StorageConstantType) -> StorageConstantType,
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ pallet_timestamp::Config,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	StorageConstant: Get<StorageConstantType>,
	StorageConstantType: Encode + PartialEq + std::fmt::Debug,
{
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			let (storage_constant_key, storage_constant_init_value): (
				Vec<u8>,
				StorageConstantType,
			) = storage_constant_key_value();

			// check delivery reward constant before (not stored yet, just as default value is used)
			assert_eq!(StorageConstant::get(), storage_constant_init_value);
			assert_eq!(sp_io::storage::get(&storage_constant_key), None);

			let new_storage_constant_value =
				new_storage_constant_value(&storage_constant_init_value);
			assert_ne!(new_storage_constant_value, storage_constant_init_value);

			// encode `set_storage` call
			let set_storage_call =
				RuntimeCallOf::<Runtime>::from(frame_system::Call::<Runtime>::set_storage {
					items: vec![(
						storage_constant_key.clone(),
						new_storage_constant_value.encode(),
					)],
				});

			// execute XCM with Transact to `set_storage` as governance does
			assert_ok!(RuntimeHelper::<Runtime>::execute_as_governance_call(
				set_storage_call,
				governance_origin
			));

			// check delivery reward constant after (stored)
			assert_eq!(StorageConstant::get(), new_storage_constant_value);
			assert_eq!(
				sp_io::storage::get(&storage_constant_key),
				Some(new_storage_constant_value.encode().into())
			);
		})
}

/// Test-case makes sure that `Runtime` can change storage constant via governance-like call
pub fn set_storage_keys_by_governance_works<Runtime>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	governance_origin: GovernanceOrigin<RuntimeOriginOf<Runtime>>,
	storage_items: Vec<(Vec<u8>, Vec<u8>)>,
	initialize_storage: impl FnOnce() -> (),
	assert_storage: impl FnOnce() -> (),
) where
	Runtime: frame_system::Config
		+ pallet_balances::Config
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ pallet_timestamp::Config,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
{
	let mut runtime = ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build();
	runtime.execute_with(|| {
		initialize_storage();
	});
	runtime.execute_with(|| {
		// encode `kill_storage` call
		let kill_storage_call =
			RuntimeCallOf::<Runtime>::from(frame_system::Call::<Runtime>::set_storage {
				items: storage_items.clone(),
			});

		// execute XCM with Transact to `set_storage` as governance does
		assert_ok!(RuntimeHelper::<Runtime>::execute_as_governance_call(
			kill_storage_call,
			governance_origin
		));
	});
	runtime.execute_with(|| {
		assert_storage();
	});
}

pub fn xcm_payment_api_with_native_token_works<Runtime, RuntimeCall, RuntimeOrigin, Block>()
where
	Runtime: XcmPaymentApiV1<Block>
		+ frame_system::Config<RuntimeOrigin = RuntimeOrigin, AccountId = AccountId>
		+ pallet_balances::Config<Balance = u128>
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config
		+ cumulus_pallet_xcmp_queue::Config
		+ pallet_timestamp::Config,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	RuntimeOrigin: OriginTrait<AccountId = <Runtime as frame_system::Config>::AccountId>,
	<<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source:
		From<<Runtime as frame_system::Config>::AccountId>,
	Block: BlockT,
{
	use xcm::prelude::*;
	ExtBuilder::<Runtime>::default().build().execute_with(|| {
		let transfer_amount = 100u128;
		let xcm_to_weigh = Xcm::<RuntimeCall>::builder_unsafe()
			.withdraw_asset((Here, transfer_amount))
			.buy_execution((Here, transfer_amount), Unlimited)
			.deposit_asset(AllCounted(1), [1u8; 32])
			.build();
		let versioned_xcm_to_weigh = VersionedXcm::from(xcm_to_weigh.clone().into());

		// We first try calling it with a lower XCM version.
		let lower_version_xcm_to_weigh =
			versioned_xcm_to_weigh.clone().into_version(XCM_VERSION - 1).unwrap();
		let xcm_weight = Runtime::query_xcm_weight(lower_version_xcm_to_weigh);
		assert!(xcm_weight.is_ok());
		let native_token: Location = Parent.into();
		let native_token_versioned = VersionedAssetId::from(AssetId(native_token));
		let lower_version_native_token =
			native_token_versioned.clone().into_version(XCM_VERSION - 1).unwrap();
		let execution_fees =
			Runtime::query_weight_to_asset_fee(xcm_weight.unwrap(), lower_version_native_token);
		assert!(execution_fees.is_ok());

		// Now we call it with the latest version.
		let xcm_weight = Runtime::query_xcm_weight(versioned_xcm_to_weigh);
		assert!(xcm_weight.is_ok());
		let execution_fees =
			Runtime::query_weight_to_asset_fee(xcm_weight.unwrap(), native_token_versioned);
		assert!(execution_fees.is_ok());

		// If we call it with anything other than the native token it will error.
		let non_existent_token: Location = Here.into();
		let non_existent_token_versioned = VersionedAssetId::from(AssetId(non_existent_token));
		let execution_fees =
			Runtime::query_weight_to_asset_fee(xcm_weight.unwrap(), non_existent_token_versioned);
		assert_eq!(execution_fees, Err(XcmPaymentApiError::AssetNotFound));
	});
}

/// Generic test case for Cumulus-based parachain that verifies if runtime can process
/// `frame_system::Call::authorize_upgrade` from governance system.
pub fn can_governance_authorize_upgrade<Runtime, RuntimeOrigin>(
	governance_origin: GovernanceOrigin<RuntimeOrigin>,
) -> Result<(), Either<DispatchError, XcmError>>
where
	Runtime: BasicParachainRuntime
		+ frame_system::Config<RuntimeOrigin = RuntimeOrigin, AccountId = AccountId>,
{
	ExtBuilder::<Runtime>::default().build().execute_with(|| {
		// check before
		assert!(frame_system::Pallet::<Runtime>::authorized_upgrade().is_none());

		// execute call as governance does
		let code_hash = Runtime::Hash::default();
		let authorize_upgrade_call: <Runtime as frame_system::Config>::RuntimeCall =
			frame_system::Call::<Runtime>::authorize_upgrade { code_hash }.into();
		RuntimeHelper::<Runtime>::execute_as_governance_call(
			authorize_upgrade_call,
			governance_origin,
		)?;

		// check after
		match frame_system::Pallet::<Runtime>::authorized_upgrade() {
			None => Err(Either::Left(frame_system::Error::<Runtime>::NothingAuthorized.into())),
			Some(_) => Ok(()),
		}
	})
}

pub fn exchange_asset_on_asset_hub_works<Runtime, RuntimeCall, RuntimeOrigin, Block>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	alice: AccountId,
	native_asset_location: Location,
	create_pool: bool,
	give_amount: Balance,
	want_amount: Balance,
	should_succeed: bool,
) where
	Runtime: XcmPaymentApiV1<Block>
		+ frame_system::Config<RuntimeOrigin = RuntimeOrigin, AccountId = AccountId>
		+ pallet_assets::Config
		+ pallet_balances::Config<Balance = u128>
		+ pallet_session::Config
		+ pallet_xcm::Config
		+ parachain_info::Config
		+ pallet_collator_selection::Config
		+ cumulus_pallet_parachain_system::Config,
	ValidatorIdOf<Runtime>: From<AccountIdOf<Runtime>>,
	RuntimeOrigin: OriginTrait<AccountId = <Runtime as frame_system::Config>::AccountId>,
	<<Runtime as frame_system::Config>::Lookup as StaticLookup>::Source:
		From<<Runtime as frame_system::Config>::AccountId>,
	Block: BlockT,
{
	const UNITS: Balance = 1_000_000_000;

	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.with_tracing()
		.build()
		.execute_with(|| {
			let native_asset_id = AssetId(native_asset_location.clone());
			let origin = RuntimeOrigin::signed(alice.clone());
			let asset_location = Location::new(1, [Parachain(2001)]);
			let asset_id = AssetId(asset_location.clone());

			// Setup initial state
			assert_ok!(<pallet_balances::Pallet<Runtime> as frame_support::traits::fungible::Mutate<_>>::mint_into(
				&alice,
				10_000 * UNITS // Enough balance for most test cases
			));

			assert_ok!(pallet_assets::Pallet::<Runtime>::force_create(
				RuntimeOrigin::root(),
				asset_location.clone().into(),
				<Runtime as frame_system::Config>::Lookup::unlookup(alice.clone()),
				true,
				1
			));

			// Simulate pool creation if required
			if create_pool {
				// In the original, this was `create_pool_with_wnd_on!`
				// Here we simulate liquidity by minting assets to Alice
				assert_ok!(pallet_assets::Pallet::<Runtime>::mint(
					RuntimeOrigin::signed(alice.clone()),
					asset_location.clone().into(),
					<Runtime as frame_system::Config>::Lookup::unlookup(alice.clone()),
					10_000 * UNITS
				));
			}

			// Execute the exchange
			let foreign_balance_before = pallet_assets::Pallet::<Runtime>::balance(asset_location.clone().into(), &alice);
			let native_balance_before = pallet_balances::Pallet::<Runtime>::total_balance(&alice);

			let give: Assets = (native_asset_id, give_amount).into();
			let want: Assets = (asset_id, want_amount).into();
			let xcm = Xcm(vec![
				WithdrawAsset(give.clone().into()),
				ExchangeAsset { give: give.into(), want: want.into(), maximal: true },
				DepositAsset { assets: Wild(All), beneficiary: alice.clone().into() },
			]);

			let result = pallet_xcm::Pallet::<Runtime>::execute(
				origin,
				xcm::VersionedXcm::from(xcm).into(),
				Weight::MAX
			);

			// Verify results
			let foreign_balance_after = pallet_assets::Pallet::<Runtime>::balance(asset_location.into(), &alice);
			let native_balance_after = pallet_balances::Pallet::<Runtime>::total_balance(&alice);

			if should_succeed {
				assert_ok!(result);
				assert!(
					foreign_balance_after >= foreign_balance_before + want_amount,
					"Expected foreign balance to increase by at least {want_amount}, got {foreign_balance_after} from {foreign_balance_before}"
				);
				assert_eq!(
					native_balance_after,
					native_balance_before - give_amount,
					"Expected native balance to decrease by {give_amount}, got {native_balance_after} from {native_balance_before}"
				);
			} else {
				assert_err_ignore_postinfo!(
					result,
					pallet_xcm::Error::<Runtime>::LocalExecutionIncomplete
				);
				assert_eq!(
					foreign_balance_after,
					foreign_balance_before,
					"Foreign balance changed unexpectedly: got {foreign_balance_after}, expected {foreign_balance_before}"
				);
				assert_eq!(
					native_balance_after,
					native_balance_before,
					"Native balance changed unexpectedly: got {native_balance_after}, expected {native_balance_before}"
				);
			}
		});
}
