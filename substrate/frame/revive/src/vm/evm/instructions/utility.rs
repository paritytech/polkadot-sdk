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

use sp_core::{H160, U256};

/// Converts a `U256` value to a `usize`, saturating to `MAX` if the value is too large.
pub fn as_usize_saturated(v: U256) -> usize {
	let x = &v.0;
	if (x[1] == 0) & (x[2] == 0) & (x[3] == 0) {
		usize::try_from(x[0]).unwrap_or(usize::MAX)
	} else {
		usize::MAX
	}
}

/// Trait for converting types into Address values.
pub trait IntoAddress {
	/// Converts the implementing type into an Address value.
	fn into_address(self) -> H160;
}

impl IntoAddress for U256 {
	fn into_address(self) -> H160 {
		H160::from_slice(&self.to_big_endian()[12..])
	}
}
