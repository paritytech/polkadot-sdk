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

use alloc::vec::Vec;
use codec::{Decode, Encode};
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

use crate::CodeUploadReturnValueV1;

define_versioned_interface! {
	/// Version 1 of the input payload that uploads contract code so that future instantiations can
	/// refer to it by hash instead of resending the code with every instantiation. The uploader is
	/// charged a storage deposit for keeping the code on chain.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct UploadCodeVersionedInputPayloadV1<AccountId, Balance> {
		/// `AccountId` that should be treated as the uploader and charged the storage deposit.
		pub origin: AccountId,
		/// Contract bytecode to upload.
		pub code: Vec<u8>,
		/// Optional cap on the storage deposit charged for keeping the uploaded code on chain.
		/// `None` lets the runtime charge the actual storage deposit the code requires.
		pub storage_deposit_limit: Option<Balance>
	}

	/// Version 1 of the output payload returned for an [`UploadCodeVersionedInputPayloadV1`]
	/// request, carrying the code hash and the storage deposit that was reserved.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct UploadCodeVersionedOutputPayloadV1<Balance> {
		/// Hash under which the uploaded code is stored, plus the deposit reserved.
		pub return_value: CodeUploadReturnValueV1<Balance>
	}
}
