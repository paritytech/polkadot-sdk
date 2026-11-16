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
	assert_ok,
	traits::{Currency, Get, OnRuntimeUpgrade, OriginTrait, StorageVersion},
	weights::WeightToFee as WeightToFeeT,
};
use parachains_common::AccountId;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, Block as BlockT, Hash, SaturatedConversion, StaticLookup},
	DispatchError, Either,
};
use xcm::prelude::*;
use xcm_executor::traits::{ClaimAssets, DropAssets, TransactAsset, XcmAssetTransfers};
use xcm_runtime_apis::fees::{
	runtime_decl_for_xcm_payment_api::XcmPaymentApiV2, Error as XcmPaymentApiError,
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

pub fn xcm_payment_api_with_native_token_works<
	Runtime,
	RuntimeCall,
	RuntimeOrigin,
	Block,
	WeightToFee,
>()
where
	Runtime: XcmPaymentApiV2<Block>
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
	WeightToFee: WeightToFeeT,
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
		let xcm_weight = Runtime::query_xcm_weight(lower_version_xcm_to_weigh)
			.expect("xcm weight must be computed");

		let expected_weight_fee: u128 = WeightToFee::weight_to_fee(&xcm_weight).saturated_into();

		let native_token: Location = Parent.into();
		let native_token_versioned = VersionedAssetId::from(AssetId(native_token));
		let lower_version_native_token =
			native_token_versioned.clone().into_version(XCM_VERSION - 1).unwrap();
		let execution_fees =
			Runtime::query_weight_to_asset_fee(xcm_weight, lower_version_native_token)
				.expect("weight must be converted to native fee");

		assert_eq!(execution_fees, expected_weight_fee);

		// Now we call it with the latest version.
		let xcm_weight =
			Runtime::query_xcm_weight(versioned_xcm_to_weigh).expect("xcm weight must be computed");

		let expected_weight_fee: u128 = WeightToFee::weight_to_fee(&xcm_weight).saturated_into();

		let execution_fees = Runtime::query_weight_to_asset_fee(xcm_weight, native_token_versioned)
			.expect("weight must be converted to native fee");

		assert_eq!(execution_fees, expected_weight_fee);

		// If we call it with anything other than the native token it will error.
		let non_existent_token: Location = Here.into();
		let non_existent_token_versioned = VersionedAssetId::from(AssetId(non_existent_token));
		let execution_fees =
			Runtime::query_weight_to_asset_fee(xcm_weight, non_existent_token_versioned);
		assert_eq!(execution_fees, Err(XcmPaymentApiError::AssetNotFound));
	});
}

/// Generic test case for Cumulus-based parachain that verifies if runtime can process
/// `frame_system::Call::authorize_upgrade` from governance system.
pub fn can_governance_authorize_upgrade<Runtime, RuntimeOrigin>(
	governance_origin: GovernanceOrigin<RuntimeOrigin>,
) -> Result<(), Either<DispatchError, InstructionError>>
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

/// Records `(origin, assets)` in `map` the way a runtime at XCM `version` did before trapped
/// assets were held by the trap account, `count` times: the hash is of the pair as encoded at
/// that version, and the relay token amount is counted in total issuance without being held by
/// anyone.
fn legacy_asset_trap<Runtime, Map>(
	origin: &Location,
	assets: &Assets,
	version: XcmVersion,
	count: u32,
) -> H256
where
	Runtime: pallet_xcm::Config,
	Map: frame_support::StorageMap<H256, u32, Query = u32>,
	pallet_xcm::BalanceOf<Runtime>: From<u128>,
{
	let versioned_assets = VersionedAssets::from(assets.clone()).into_version(version).unwrap();
	let hash = match VersionedLocation::from(origin.clone()).into_version(version).unwrap() {
		VersionedLocation::V3(origin) => BlakeTwo256::hash_of(&(origin, versioned_assets)),
		VersionedLocation::V4(origin) => BlakeTwo256::hash_of(&(origin, versioned_assets)),
		VersionedLocation::V5(origin) => BlakeTwo256::hash_of(&(origin, versioned_assets)),
	};
	Map::mutate(hash, |n| *n += count);
	for asset in assets.inner() {
		if let (AssetId(location), Fungible(amount)) = (&asset.id, &asset.fun) {
			if location == &Location::parent() {
				for _ in 0..count {
					// Issued and forgotten: in total issuance, in nobody's balance.
					core::mem::forget(<Runtime as pallet_xcm::Config>::Currency::issue(
						(*amount).into(),
					));
				}
			}
		}
	}
	hash
}

/// Sum of all relay token balances, which equals total issuance once no legacy trap is left.
fn sum_of_balances<Runtime: pallet_xcm::Config>() -> u128
where
	pallet_xcm::BalanceOf<Runtime>: Into<u128>,
{
	frame_system::Account::<Runtime>::iter_keys()
		.map(|who| <Runtime as pallet_xcm::Config>::Currency::total_balance(&who).into())
		.sum()
}

/// Test-case makes sure that `pallet_xcm`'s trap account works on `Runtime` and that
/// `pallet_xcm::migration::v2::MigrateToV2` brings a chain that trapped assets before the trap
/// account existed into the state `pallet_xcm`'s try-state expects.
///
/// Starting from a chain without the trap account and with assets trapped under older XCM
/// versions (counted in total issuance, held by nobody), running the migration must:
/// - move those traps to `LegacyAssetTraps` and leave `AssetTraps` empty,
/// - fund the trap account with the existential deposit, the only issuance change,
/// - pass `pallet_xcm::Pallet::try_state_trap_account`,
/// - leave every legacy trap claimable through its original version ticket, by minting.
///
/// Finally, assets trapped on this runtime land in the trap account and can be claimed, with
/// the sum of balances matching total issuance throughout.
pub fn pallet_xcm_trap_account_migration_works<Runtime>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
) where
	Runtime: BasicParachainRuntime,
	pallet_xcm::BalanceOf<Runtime>: From<u128> + Into<u128>,
{
	type Xcm<Runtime> = pallet_xcm::Pallet<Runtime>;
	ExtBuilder::<Runtime>::default()
		.with_collators(collator_session_key.collators())
		.with_session_keys(collator_session_key.session_keys())
		.with_para_id(runtime_para_id.into())
		.build()
		.execute_with(|| {
			let trap_account = Xcm::<Runtime>::trap_account()
				.expect("`TrapAccountLocation` converts to an account");
			let ed: u128 = <Runtime as pallet_xcm::Config>::Currency::minimum_balance().into();
			let free = |who| <Runtime as pallet_xcm::Config>::Currency::free_balance(who).into();
			let issuance = || <Runtime as pallet_xcm::Config>::Currency::total_issuance().into();

			// A chain from before the trap account existed: no balance, storage version 1.
			assert_eq!(free(&trap_account), 0u128);
			StorageVersion::new(1).put::<Xcm<Runtime>>();
			assert!(Xcm::<Runtime>::try_state_trap_account().is_err());

			// The pair was trapped twice under XCM v4 and once under v5.
			let origin = Location::new(1, [Parachain(2000)]);
			let amount = 10 * ed;
			let assets: Assets = (Parent, amount).into();
			let hash_v4 =
				legacy_asset_trap::<Runtime, pallet_xcm::AssetTraps<Runtime>>(&origin, &assets, 4, 2);
			let hash_v5 =
				legacy_asset_trap::<Runtime, pallet_xcm::AssetTraps<Runtime>>(&origin, &assets, 5, 1);
			let issuance_before: u128 = issuance();
			assert_eq!(sum_of_balances::<Runtime>(), issuance_before - 3 * amount);

			pallet_xcm::migration::v2::MigrateToV2::<Runtime>::on_runtime_upgrade();

			assert_eq!(StorageVersion::get::<Xcm<Runtime>>(), 2);
			assert!(pallet_xcm::AssetTraps::<Runtime>::iter().next().is_none());
			assert_eq!(pallet_xcm::LegacyAssetTraps::<Runtime>::get(hash_v4), 2);
			assert_eq!(pallet_xcm::LegacyAssetTraps::<Runtime>::get(hash_v5), 1);
			assert_eq!(free(&trap_account), ed);
			assert_eq!(issuance(), issuance_before + ed);
			assert_ok!(Xcm::<Runtime>::try_state_trap_account());

			// Running the migration again does nothing.
			pallet_xcm::migration::v2::MigrateToV2::<Runtime>::on_runtime_upgrade();
			assert_eq!(free(&trap_account), ed);
			assert_eq!(issuance(), issuance_before + ed);

			// Each legacy trap is claimable through its version ticket; the claim mints the
			// amount and the claimed holding is dropped here, which burns it again.
			let context = XcmContext { origin: Some(origin.clone()), message_id: [0; 32], topic: None };
			for (ticket, hash, count) in [(4u128, hash_v4, 2u32), (5, hash_v5, 1)] {
				let ticket: Location = GeneralIndex(ticket).into();
				for remaining in (0..count).rev() {
					let claimed = <Xcm<Runtime> as ClaimAssets>::claim_assets(
						&origin, &ticket, &assets, &context,
					)
					.expect("legacy traps are claimable");
					assert!(claimed.ensure_contains(&assets).is_ok());
					assert_eq!(pallet_xcm::LegacyAssetTraps::<Runtime>::get(hash), remaining);
				}
				assert!(<Xcm<Runtime> as ClaimAssets>::claim_assets(
					&origin, &ticket, &assets, &context,
				)
				.is_none());
			}
			assert_eq!(free(&trap_account), ed);
			assert_eq!(issuance(), issuance_before + ed - 3 * amount);
			assert_eq!(sum_of_balances::<Runtime>(), issuance_before + ed - 3 * amount);
			assert_ok!(Xcm::<Runtime>::try_state_trap_account());

			// Assets trapped on this runtime are held by the trap account until claimed.
			let issuance_before: u128 = issuance();
			let mut holding = xcm_executor::AssetsInHolding::new();
			for asset in assets.inner() {
				let minted = <<Runtime as pallet_xcm::Config>::XcmExecutor as XcmAssetTransfers>::AssetTransactor::mint_asset(asset, &context)
					.expect("the relay token can be minted");
				holding.subsume_assets(minted);
			}
			assert_eq!(issuance(), issuance_before + amount);
			<Xcm<Runtime> as DropAssets>::drop_assets(&origin, holding, &context);
			let hash = BlakeTwo256::hash_of(&(origin.clone(), VersionedAssets::from(assets.clone())));
			assert_eq!(pallet_xcm::AssetTraps::<Runtime>::get(hash), 1);
			assert!(pallet_xcm::LegacyAssetTraps::<Runtime>::iter().next().is_none());
			assert_eq!(free(&trap_account), ed + amount);
			assert_eq!(sum_of_balances::<Runtime>(), issuance_before + amount);
			let claimed = <Xcm<Runtime> as ClaimAssets>::claim_assets(
				&origin, &Here.into(), &assets, &context,
			)
			.expect("trapped assets are held by the trap account");
			assert!(claimed.ensure_contains(&assets).is_ok());
			drop(claimed);
			assert_eq!(pallet_xcm::AssetTraps::<Runtime>::get(hash), 0);
			assert_eq!(free(&trap_account), ed);
			assert_eq!(issuance(), issuance_before);
			assert_eq!(sum_of_balances::<Runtime>(), issuance_before);
			assert_ok!(Xcm::<Runtime>::try_state_trap_account());
		})
}
