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

//! Runtime API definition for fungibles.

use codec::{Codec, Decode, Encode};
use frame_support::RuntimeDebug;
use sp_std::vec::Vec;
use xcm::latest::MultiAsset;

/// The possible errors that can happen querying the storage of assets.
#[derive(Eq, PartialEq, Encode, Decode, RuntimeDebug)]
pub enum FungiblesAccessError {
	/// `MultiLocation` to `AssetId`/`ClassId` conversion failed.
	AssetIdConversionFailed,
	/// `u128` amount to currency `Balance` conversion failed.
	AmountToBalanceConversionFailed,
}

sp_api::decl_runtime_apis! {
	/// The API for querying account's balances from runtime.
	pub trait FungiblesApi<AccountId>
	where
		AccountId: Codec,
	{
		/// Returns the list of all [`MultiAsset`] that an `AccountId` has.
		fn query_account_balances(account: AccountId) -> Result<Vec<MultiAsset>, FungiblesAccessError>;
	}
}
