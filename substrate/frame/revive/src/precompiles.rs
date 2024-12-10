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

pub use alloy_core as alloy;

use alloy::{
	primitives::Address,
	sol_types::{SolError, SolInterface, SolValue},
};

pub enum AddressMatcher {
	Fixed([u8; 20]),
	Prefix([u8; 8]),
}

impl AddressMatcher {
	pub fn matches(&self, address: &[u8; 20]) -> bool {
		match self {
			AddressMatcher::Fixed(needle) => needle == address,
			AddressMatcher::Prefix(prefix) => prefix == &address[..8],
		}
	}
}

pub trait Precompile {
	const MATCHER: AddressMatcher;
	type Interface: SolInterface;

	fn call(
		address: &[u8; 20],
		input: &Self::Interface,
		env: &impl Environment,
	) -> Result<impl SolValue + 'static, impl SolError + 'static>;
}

pub trait Environment {
	fn address(&self) -> Address;
}
