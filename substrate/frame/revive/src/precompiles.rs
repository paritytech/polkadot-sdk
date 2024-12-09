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

#[cfg(test)]
mod tests;

pub use alloy_core as alloy;

use crate::{exec::Ext, Config};
use alloc::vec::Vec;
use alloy::sol_types::{Panic, PanicKind, SolError, SolInterface, SolValue};

pub enum AddressMatcher {
	Fixed([u8; 20]),
	Prefix([u8; 16]),
}

pub trait EthPrecompile {
	type T: Config;
	type Interface: SolInterface;
	const MATCHER: AddressMatcher;

	fn call(
		address: &[u8; 20],
		input: &Self::Interface,
		env: &impl Ext<T = Self::T>,
	) -> Result<impl SolValue + 'static, impl SolError + 'static>;
}

pub(crate) trait Precompile {
	type T: Config;
	const MATCHER: AddressMatcher;
	const CHECK_COLLISION: ();

	fn call(
		address: &[u8; 20],
		input: &[u8],
		env: &impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Vec<u8>>;
}

pub trait Precompiles<T: Config> {
	const CHECK_COLLISION: ();

	fn matches(address: &[u8; 20]) -> bool;
	fn call(
		address: &[u8; 20],
		input: &[u8],
		env: &impl Ext<T = T>,
	) -> Option<Result<Vec<u8>, Vec<u8>>>;
}

impl<P: EthPrecompile> Precompile for P {
	type T = <Self as EthPrecompile>::T;
	const MATCHER: AddressMatcher = P::MATCHER;
	const CHECK_COLLISION: () = {
		// we reserve everthing up until 0xFF for future eth precompiles
		// this means at least on byte (except the first) has to be != 0
		let bytes = Self::MATCHER.bytes();
		let mut i = 1;
		let mut invalid_prefix = true;
		while i < bytes.len() {
			if bytes[i] != 0 {
				invalid_prefix = false;
			}
			i += 1;
		}
		if invalid_prefix {
			panic!("Collision with Ethereum native precompiles");
		}
	};

	fn call(
		address: &[u8; 20],
		input: &[u8],
		env: &impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Vec<u8>> {
		let _ = Self::CHECK_COLLISION;
		let call = <Self as EthPrecompile>::Interface::abi_decode(input, true)
			.map_err(|_| Panic::from(PanicKind::Generic).abi_encode())?;
		match Self::call(address, &call, env) {
			Ok(value) => Ok(value.abi_encode()),
			Err(err) => Err(err.abi_encode()),
		}
	}
}

#[impl_trait_for_tuples::impl_for_tuples(10)]
#[tuple_types_custom_trait_bound(Precompile<T=T>)]
impl<T: Config> Precompiles<T> for Tuple {
	const CHECK_COLLISION: () = {
		let matchers = [for_tuples!( #( Tuple::MATCHER ),* )];
		AddressMatcher::check_collision(&matchers);
	};

	fn matches(address: &[u8; 20]) -> bool {
		let _ = <Self as Precompiles<T>>::CHECK_COLLISION;
		for_tuples!(
			#(
				if Tuple::MATCHER.matches(address) {
					return true;
				}
			)*
		);
		false
	}

	fn call(
		address: &[u8; 20],
		input: &[u8],
		env: &impl Ext<T = T>,
	) -> Option<Result<Vec<u8>, Vec<u8>>> {
		for_tuples!(
			#(
				if Self::matches(address) {
					return Some(Tuple::call(address, input, env));
				}
			)*
		);
		None
	}
}

impl AddressMatcher {
	const fn bytes(&self) -> &[u8] {
		match self {
			AddressMatcher::Fixed(needle) => needle.as_slice(),
			AddressMatcher::Prefix(prefix) => prefix.as_slice(),
		}
	}

	const fn matches(&self, address: &[u8; 20]) -> bool {
		Self::cmp_prefix(self.bytes(), address)
	}

	const fn cmp_prefix(a: &[u8], b: &[u8]) -> bool {
		let mut i = 0;
		while i < a.len() && i < b.len() {
			if a[i] != b[i] {
				return false
			}
			i += 1;
		}
		return true
	}

	const fn check_collision(list: &[Self]) {
		let len = list.len();
		let mut i = 0;
		let mut collision = false;
		'outer: while i < len {
			let mut j = i + 1;
			while j < len {
				match (&list[i], &list[j]) {
					(Self::Fixed(addr_a), Self::Fixed(addr_b)) => {
						if Self::cmp_prefix(addr_a, addr_b) {
							collision = true;
							break 'outer
						}
					},
					(Self::Fixed(addr_a), Self::Prefix(pref_b)) =>
						if Self::cmp_prefix(addr_a, pref_b) {
							collision = true;
							break 'outer
						},
					(Self::Prefix(pref_a), Self::Fixed(addr_b)) =>
						if Self::cmp_prefix(pref_a, addr_b) {
							collision = true;
							break 'outer
						},
					(Self::Prefix(pref_a), Self::Prefix(pref_b)) =>
						if Self::cmp_prefix(pref_a, pref_b) {
							collision = true;
							break 'outer
						},
				}
				j += 1;
			}
			i += 1;
		}

		if collision {
			panic!("Collision in precompile addresses detected");
		}
	}
}
