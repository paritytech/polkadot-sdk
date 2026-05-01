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
use codec::{Decode, Encode, MaxEncodedLen};
use ethereum_types::H256;
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;

define_versioned_type! {
	/// Version 1 of a contract code reference.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	pub enum CodeV1 {
		/// A new contract module as raw bytes.
		Upload(Vec<u8>),
		/// The hash of an existing on-chain contract module.
		Existing(H256),
	}
}

define_versioned_type! {
	/// Version 1 of the successful return value produced by uploading contract code.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub struct CodeUploadReturnValueV1<Balance> {
		/// The key under which the uploaded code is stored.
		pub code_hash: H256,
		/// The deposit reserved from the caller.
		pub deposit: Balance,
	}
}
