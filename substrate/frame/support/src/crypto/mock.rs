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

use codec::{Decode, Encode};

/// A mock type for account, identifies a u64 and consider any signature valid.
#[derive(Clone, Debug, Encode, Decode, PartialEq, Eq, PartialOrd, Ord, scale_info::TypeInfo)]
pub struct AccountU64(u64);
impl sp_runtime::traits::IdentifyAccount for AccountU64 {
	type AccountId = u64;
	fn into_account(self) -> u64 {
		self.0
	}
}

impl sp_runtime::traits::Verify for AccountU64 {
	type Signer = AccountU64;
	fn verify<L: sp_runtime::traits::Lazy<[u8]>>(
		&self,
		_msg: L,
		_signer: &<Self::Signer as sp_runtime::traits::IdentifyAccount>::AccountId,
	) -> bool {
		true
	}
}

impl From<u64> for AccountU64 {
	fn from(value: u64) -> Self {
		Self(value)
	}
}
