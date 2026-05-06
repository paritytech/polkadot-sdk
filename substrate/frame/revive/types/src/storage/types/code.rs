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

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

define_versioned_type! {
	/// Version 1 of the bytecode format recorded for a code hash.
	#[derive(
		Debug,
		Clone,
		Copy,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		Serialize,
		Deserialize,
		MaxEncodedLen,
	)]
	pub enum BytecodeTypeV1 {
		/// The stored code is a PolkaVM program.
		Pvm,
		/// The stored code is EVM bytecode.
		Evm,
	}
}

define_versioned_type! {
	/// Version 1 of the `CodeInfoOf` storage value keyed by code hash.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		Serialize,
		Deserialize,
	)]
	pub struct CodeInfoV1<Owner, Balance> {
		/// The account that uploaded the code and may remove it when it is no longer used.
		pub owner: Owner,
		/// The balance reserved to keep the code and its metadata on chain.
		#[codec(compact)]
		pub deposit: Balance,
		/// The number of contracts that currently reference this code hash.
		#[codec(compact)]
		pub refcount: u64,
		/// The stored code length in bytes.
		pub code_len: u32,
		/// The behaviour version that fixes the observable contract semantics.
		pub behaviour_version: u32,
	}

	/// Version 2 of the `CodeInfoOf` storage value keyed by code hash.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
		Serialize,
		Deserialize,
	)]
	#[codec(mel_bound(
		Owner: MaxEncodedLen,
		Balance: MaxEncodedLen + codec::HasCompact,
	))]
	#[versioned_type(extend)]
	pub struct CodeInfoV2<Owner, Balance> {
		/// The bytecode format used by the stored code.
		#[versioned_type(insert_after = "code_len")]
		pub code_type: BytecodeTypeV1,
	}
}
