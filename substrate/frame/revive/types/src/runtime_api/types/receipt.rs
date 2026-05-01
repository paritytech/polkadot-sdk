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
use ethereum_types::U256;
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;

define_versioned_type! {
	/// Version 1 of the gas data needed to reconstruct an Ethereum receipt.
	#[derive(
		Debug,
		Default,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub struct ReceiptGasInfoV1 {
		/// The amount of gas used by one transaction.
		pub gas_used: U256,
		/// The effective gas price paid by one transaction.
		pub effective_gas_price: U256,
	}
}
