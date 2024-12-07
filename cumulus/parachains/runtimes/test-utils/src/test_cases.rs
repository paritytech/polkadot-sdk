// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Module contains predefined test-case scenarios for `Runtime` with common functionality.

use crate::{AccountIdOf, CollatorSessionKeys, ExtBuilder, ValidatorIdOf};
use codec::Encode;
use frame_support::{
	assert_ok,
	traits::{Get, OriginTrait},
};
use parachains_common::AccountId;
use sp_runtime::traits::{Block as BlockT, StaticLookup};
use xcm_runtime_apis::fees::{
	runtime_decl_for_xcm_payment_api::XcmPaymentApiV1, Error as XcmPaymentApiError,
};

type RuntimeHelper<Runtime, AllPalletsWithoutSystem = ()> =
	crate::RuntimeHelper<Runtime, AllPalletsWithoutSystem>;

/// Test-case makes sure that `Runtime` can change storage constant via governance-like call
pub fn change_storage_constant_by_governance_works<Runtime, StorageConstant, StorageConstantType>(
	collator_session_key: CollatorSessionKeys<Runtime>,
	runtime_para_id: u32,
	runtime_call_encode: Box<dyn Fn(frame_system::Call<Runtime>) -> Vec<u8>>,
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
				runtime_call_encode(frame_system::Call::<Runtime>::set_storage {
					items: vec![(
						storage_constant_key.clone(),
						new_storage_constant_value.encode(),
					)],
				});

			// execute XCM with Transact to `set_storage` as governance does
			assert_ok!(RuntimeHelper::<Runtime>::execute_as_governance(set_storage_call,)
				.ensure_complete());

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
	runtime_call_encode: Box<dyn Fn(frame_system::Call<Runtime>) -> Vec<u8>>,
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
		let kill_storage_call = runtime_call_encode(frame_system::Call::<Runtime>::set_storage {
			items: storage_items.clone(),
		});

		// execute XCM with Transact to `set_storage` as governance does
		assert_ok!(
			RuntimeHelper::<Runtime>::execute_as_governance(kill_storage_call,).ensure_complete()
		);
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
