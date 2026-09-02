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

use sp_runtime_interface::{pass_by::PassFatPointerAndRead, runtime_interface};

#[allow(unused_imports)]
use crate::*;

/// Interface that provides functions to access the Offchain DB.
#[runtime_interface]
pub trait OffchainIndex {
	/// Write a key value pair to the Offchain DB database in a buffered fashion.
	fn set(&mut self, key: PassFatPointerAndRead<&[u8]>, value: PassFatPointerAndRead<&[u8]>) {
		self.set_offchain_storage(key, Some(value));
	}

	/// Remove a key and its associated value from the Offchain DB.
	fn clear(&mut self, key: PassFatPointerAndRead<&[u8]>) {
		self.set_offchain_storage(key, None);
	}
}
