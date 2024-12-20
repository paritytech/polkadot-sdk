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
//! Ethereum signature utilities
use super::{TransactionLegacySigned, TransactionLegacyUnsigned};
use rlp::Encodable;
use sp_core::{H160, U256};
use sp_io::{crypto::secp256k1_ecdsa_recover, hashing::keccak_256};

impl TransactionLegacyUnsigned {
	/// Recover the Ethereum address, from an RLP encoded transaction and a 65 bytes signature.
	pub fn recover_eth_address(rlp_encoded: &[u8], signature: &[u8; 65]) -> Result<H160, ()> {
		let hash = keccak_256(rlp_encoded);
		let mut addr = H160::default();
		let pk = secp256k1_ecdsa_recover(&signature, &hash).map_err(|_| ())?;
		addr.assign_from_slice(&keccak_256(&pk[..])[12..]);

		Ok(addr)
	}
}

impl TransactionLegacySigned {
	/// Create a signed transaction from an [`TransactionLegacyUnsigned`] and a signature.
	pub fn from(
		transaction_legacy_unsigned: TransactionLegacyUnsigned,
		signature: &[u8; 65],
	) -> TransactionLegacySigned {
		let r = U256::from_big_endian(&signature[..32]);
		let s = U256::from_big_endian(&signature[32..64]);
		let recovery_id = signature[64] as u32;
		let v = transaction_legacy_unsigned
			.chain_id
			.map(|chain_id| chain_id * 2 + 35 + recovery_id)
			.unwrap_or_else(|| U256::from(27) + recovery_id);

		TransactionLegacySigned { transaction_legacy_unsigned, r, s, v }
	}

	/// Get the raw 65 bytes signature from the signed transaction.
	pub fn raw_signature(&self) -> Result<[u8; 65], ()> {
		let mut s = [0u8; 65];
		self.r.write_as_big_endian(s[0..32].as_mut());
		self.s.write_as_big_endian(s[32..64].as_mut());
		s[64] = self.extract_recovery_id().ok_or(())?;
		Ok(s)
	}

	/// Get the recovery ID from the signed transaction.
	/// See https://eips.ethereum.org/EIPS/eip-155
	fn extract_recovery_id(&self) -> Option<u8> {
		if let Some(chain_id) = self.transaction_legacy_unsigned.chain_id {
			// self.v - chain_id * 2 - 35
			let v: u64 = self.v.try_into().ok()?;
			let chain_id: u64 = chain_id.try_into().ok()?;
			let r = v.checked_sub(chain_id.checked_mul(2)?)?.checked_sub(35)?;
			r.try_into().ok()
		} else {
			self.v.try_into().ok()
		}
	}

	/// Recover the Ethereum address from the signed transaction.
	pub fn recover_eth_address(&self) -> Result<H160, ()> {
		let rlp_encoded = self.transaction_legacy_unsigned.rlp_bytes();
		TransactionLegacyUnsigned::recover_eth_address(&rlp_encoded, &self.raw_signature()?)
	}
}
