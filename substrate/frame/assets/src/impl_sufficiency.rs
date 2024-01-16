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

//! Assets pallet's `StoredMap` implementation.

use crate::{
	traits::sufficiency::{IsSufficient, SetSufficiency},
	Asset, Config, Pallet,
};

impl<T: Config<I>, I: 'static> IsSufficient<<T as Config<I>>::AssetId> for Pallet<T, I> {
	fn is_sufficient(asset_id: <T as Config<I>>::AssetId) -> bool {
		Asset::<T, I>::get(asset_id).map(|asset| asset.is_sufficient).unwrap_or(false)
	}
}

impl<T: Config<I>, I: 'static> SetSufficiency<<T as Config<I>>::AssetId> for Pallet<T, I> {
	fn make_sufficient(asset_id: <T as Config<I>>::AssetId) -> sp_runtime::DispatchResult {
		Pallet::<T, I>::do_set_sufficiency(asset_id, true)
	}

	fn make_insufficient(asset_id: <T as Config<I>>::AssetId) -> sp_runtime::DispatchResult {
		Pallet::<T, I>::do_set_sufficiency(asset_id, false)
	}
}
