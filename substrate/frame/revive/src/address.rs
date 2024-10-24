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

//! Functions that deal contract addresses.

use alloc::vec::Vec;
use sp_core::H160;
use sp_io::hashing::keccak_256;
use sp_runtime::AccountId32;

/// Map between the native chain account id `T` and an Ethereum [`H160`].
///
/// This trait exists only to emulate specialization for different concrete
/// native account ids. **Not** to make the mapping user configurable. Hence
/// the trait is `Sealed` and only one mandatory implementor [`DefaultAddressMapper`]
/// exists.
///
/// Please note that we assume that the native account is at least 20 bytes and
/// only implement this type for a `T` where this is the case. Luckily, this is the
/// case for all existing runtimes as of right now. Reasing is that this will allow
/// us to reverse an address -> account_id mapping by just stripping the prefix.
pub trait AddressMapper<T>: private::Sealed {
	/// Convert an account id to an ethereum address.
	///
	/// This mapping is **not** required to be reversible.
	fn to_address(account_id: &T) -> H160;

	/// Convert an ethereum address to a native account id.
	///
	/// This mapping is **required** to be reversible.
	fn to_account_id(address: &H160) -> T;

	/// Same as [`Self::to_account_id`] but when we know the address is a contract.
	///
	/// This is only the case when we just generated the new address.
	fn to_account_id_contract(address: &H160) -> T;
}

mod private {
	pub trait Sealed {}
	impl Sealed for super::DefaultAddressMapper {}
}

/// The only implementor for `AddressMapper`.
pub enum DefaultAddressMapper {}

impl AddressMapper<AccountId32> for DefaultAddressMapper {
	fn to_address(account_id: &AccountId32) -> H160 {
		H160::from_slice(&<AccountId32 as AsRef<[u8; 32]>>::as_ref(&account_id)[..20])
	}

	fn to_account_id(address: &H160) -> AccountId32 {
		let mut account_id = AccountId32::new([0xEE; 32]);
		<AccountId32 as AsMut<[u8; 32]>>::as_mut(&mut account_id)[..20]
			.copy_from_slice(address.as_bytes());
		account_id
	}

	fn to_account_id_contract(address: &H160) -> AccountId32 {
		Self::to_account_id(address)
	}
}

/// Determine the address of a contract using CREATE semantics.
pub fn create1(deployer: &H160, nonce: u64) -> H160 {
	let mut list = rlp::RlpStream::new_list(2);
	list.append(&deployer.as_bytes());
	list.append(&nonce);
	let hash = keccak_256(&list.out());
	H160::from_slice(&hash[12..])
}

/// Determine the address of a contract using the CREATE2 semantics.
pub fn create2(deployer: &H160, code: &[u8], input_data: &[u8], salt: &[u8; 32]) -> H160 {
	let init_code_hash = {
		let init_code: Vec<u8> = code.into_iter().chain(input_data).cloned().collect();
		keccak_256(init_code.as_ref())
	};
	let mut bytes = [0; 85];
	bytes[0] = 0xff;
	bytes[1..21].copy_from_slice(deployer.as_bytes());
	bytes[21..53].copy_from_slice(salt);
	bytes[53..85].copy_from_slice(&init_code_hash);
	let hash = keccak_256(&bytes);
	H160::from_slice(&hash[12..])
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::test_utils::ALICE_ADDR;
	use sp_core::{hex2array, H160};

	#[test]
	fn create1_works() {
		assert_eq!(
			create1(&ALICE_ADDR, 1u64),
			H160(hex2array!("c851da37e4e8d3a20d8d56be2963934b4ad71c3b")),
		)
	}

	#[test]
	fn create2_works() {
		assert_eq!(
			create2(
				&ALICE_ADDR,
				&hex2array!("600060005560016000"),
				&hex2array!("55"),
				&hex2array!("1234567890123456789012345678901234567890123456789012345678901234")
			),
			H160(hex2array!("7f31e795e5836a19a8f919ab5a9de9a197ecd2b6")),
		)
	}
}
