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
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

define_versioned_type! {
	/// Version 1 of the `DeletionQueueCounter` storage value.
	///
	/// The runtime type carries a `PhantomData<T>` marker, but that marker contributes no SCALE
	/// bytes and is therefore omitted from the client-facing representation.
	#[derive(
		Debug, Default, Clone, Eq, PartialEq, TypeInfo, Encode, Decode, Serialize, Deserialize,
	)]
	pub struct DeletionQueueManagerV1 {
		/// Counter used when inserting the next trie ID into the deletion queue.
		pub insert_counter: u32,
		/// Counter used when reading the next trie ID to delete from the queue.
		pub delete_counter: u32,
	}
}
