// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;
use crate::tests::Test;
use sp_core::hex2array;

#[test]
fn matching_works() {
	struct Matcher1;
	struct Matcher2;

	impl Precompile for Matcher1 {
		type T = Test;
		const MATCHER: AddressMatcher = AddressMatcher::Fixed([0x42; 20]);
		const CHECK_COLLISION: () = ();

		fn call(
			address: &[u8; 20],
			_input: &[u8],
			_env: &impl Ext<T = Self::T>,
		) -> Result<Vec<u8>, Vec<u8>> {
			Ok(address.to_vec())
		}
	}

	impl Precompile for Matcher2 {
		type T = Test;
		const MATCHER: AddressMatcher = AddressMatcher::Prefix([0x88; 16]);
		const CHECK_COLLISION: () = ();

		fn call(
			address: &[u8; 20],
			_input: &[u8],
			_env: &impl Ext<T = Self::T>,
		) -> Result<Vec<u8>, Vec<u8>> {
			Ok(address.to_vec())
		}
	}

	type Col = (Matcher1, Matcher2);

	assert!(Col::matches(&[0x42; 20]));
	assert!(Col::matches(&hex2array!("8888888888888888888888888888888888888888")));
	assert!(Col::matches(&hex2array!("8888888888888888888888888888888800000000")));
	assert!(Col::matches(&hex2array!("8888888888888888888888888888888846788952")));
	assert!(!Col::matches(&hex2array!("8888888888888888868888888888888846788952")));
}
