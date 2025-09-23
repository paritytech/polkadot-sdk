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

//! Utility macros to help implementing opcode instruction functions.

/// Converts a `U256` value to a `u64`, saturating to `MAX` if the value is too large.
#[macro_export]
macro_rules! as_u64_saturated {
	($v:expr) => {
		match &$v.0 {
			x =>
				if (x[1] == 0) & (x[2] == 0) & (x[3] == 0) {
					x[0]
				} else {
					u64::MAX
				},
		}
	};
}

/// Converts a `U256` value to a `usize`, saturating to `MAX` if the value is too large.
#[macro_export]
macro_rules! as_usize_saturated {
	($v:expr) => {
		usize::try_from(as_u64_saturated!($v)).unwrap_or(usize::MAX)
	};
}
