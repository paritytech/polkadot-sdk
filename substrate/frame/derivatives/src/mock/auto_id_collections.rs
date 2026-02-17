// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

use super::*;

pub type AutoIdCollectionsInstance = unique_items::Instance2;
pub type CollectionAutoId = u64;
impl unique_items::Config<unique_items::Instance2> for Test {
	type ItemId = CollectionAutoId;
}

// Below are the operations implementations:
// * collection creation (with an automatically assigned ID)
// * collection destruction

impl Create<WithConfig<ConfigValue<Owner<AccountId>>, AutoId<CollectionAutoId>>>
	for AutoIdCollections
{
	fn create(
		strategy: WithConfig<ConfigValue<Owner<AccountId>>, AutoId<CollectionAutoId>>,
	) -> Result<CollectionAutoId, DispatchError> {
		let WithConfig { config: ConfigValue(owner), .. } = strategy;
		let id = unique_items::CurrentItemId::<Test, Instance2>::get().unwrap_or(0);

		unique_items::ItemOwner::<Test, unique_items::Instance2>::insert(id, owner);
		unique_items::CurrentItemId::<Test, Instance2>::set(Some(id.saturating_add(1)));

		Ok(id)
	}
}
impl AssetDefinition for AutoIdCollections {
	type Id = CollectionAutoId;
}
impl Destroy<NoParams> for AutoIdCollections {
	fn destroy(id: &Self::Id, _strategy: NoParams) -> DispatchResult {
		unique_items::ItemOwner::<Test, unique_items::Instance2>::remove(id);

		Ok(())
	}
}
