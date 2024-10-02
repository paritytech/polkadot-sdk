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

use crate::{AccountIdOf, CollatorSessionKeys, ExtBuilder, ValidatorIdOf};
use codec::Encode;
use frame_support::{assert_ok, traits::Get};

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

			// estimate - storing just 1 value
			use frame_system::WeightInfo;
			let require_weight_at_most =
				<Runtime as frame_system::Config>::SystemWeightInfo::set_storage(1);

			// execute XCM with Transact to `set_storage` as governance does
			assert_ok!(RuntimeHelper::<Runtime>::execute_as_governance(
				set_storage_call,
				require_weight_at_most
			)
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

		// estimate - storing just 1 value
		use frame_system::WeightInfo;
		let require_weight_at_most =
			<Runtime as frame_system::Config>::SystemWeightInfo::set_storage(
				storage_items.len().try_into().unwrap(),
			);

		// execute XCM with Transact to `set_storage` as governance does
		assert_ok!(RuntimeHelper::<Runtime>::execute_as_governance(
			kill_storage_call,
			require_weight_at_most
		)
		.ensure_complete());
	});
	runtime.execute_with(|| {
		assert_storage();
	});
}
