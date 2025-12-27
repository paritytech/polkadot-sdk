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

pub type PredefinedIdCollectionsInstance = unique_items::Instance1;
impl unique_items::Config<PredefinedIdCollectionsInstance> for Test {
	type ItemId = AssetId;
}

// Below are the operations implementations:
// * collection creation (with a predefined ID)
// * collection destruction

impl Create<WithConfig<ConfigValue<Owner<AccountId>>, PredefinedId<AssetId>>>
	for PredefinedIdCollections
{
	fn create(
		strategy: WithConfig<ConfigValue<Owner<AccountId>>, PredefinedId<AssetId>>,
	) -> Result<AssetId, DispatchError> {
		let WithConfig { config: ConfigValue(owner), extra: id_assignment } = strategy;
		let id = id_assignment.params;

		unique_items::ItemOwner::<Test, PredefinedIdCollectionsInstance>::try_mutate(
			id.clone(),
			|current_owner| {
				if current_owner.is_none() {
					*current_owner = Some(owner);
					Ok(())
				} else {
					Err(unique_items::Error::<Test, PredefinedIdCollectionsInstance>::AlreadyExists)
				}
			},
		)?;

		Ok(id)
	}
}
impl AssetDefinition for PredefinedIdCollections {
	type Id = AssetId;
}
impl Destroy<NoParams> for PredefinedIdCollections {
	fn destroy(id: &Self::Id, _strategy: NoParams) -> DispatchResult {
		unique_items::ItemOwner::<Test, PredefinedIdCollectionsInstance>::remove(id);

		Ok(())
	}
}
