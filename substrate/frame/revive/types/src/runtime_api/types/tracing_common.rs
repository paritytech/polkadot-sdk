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
	/// Version 1 of the call kind reported by call tracing.
	#[derive(
		Debug,
		Default,
		Clone,
		Copy,
		Serialize,
		Deserialize,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	#[serde(rename_all = "UPPERCASE")]
	pub enum CallTypeV1 {
		/// A regular call.
		#[default]
		Call,
		/// A read-only call.
		StaticCall,
		/// A delegate call.
		DelegateCall,
		/// A create call.
		Create,
		/// A create2 call.
		Create2,
		/// A self-destruct call.
		Selfdestruct,
	}
}
