// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Test accounts.

use codec::Encode;
use kitchensink_runtime::{CheckedExtrinsic, SessionKeys, SignedExtra, UncheckedExtrinsic};
use node_primitives::{AccountId, Balance, Nonce};
use sp_core::{ecdsa, ed25519, sr25519};
use sp_crypto_hashing::blake2_256;
use sp_keyring::Sr25519Keyring;
use sp_runtime::generic::Era;
use std::str::FromStr;

/// Alice's account id.
pub fn alice() -> AccountId {
	Sr25519Keyring::Alice.into()
}

/// Bob's account id.
pub fn bob() -> AccountId {
	Sr25519Keyring::Bob.into()
}

/// Charlie's account id.
pub fn charlie() -> AccountId {
	Sr25519Keyring::Charlie.into()
}

/// Dave's account id.
pub fn dave() -> AccountId {
	Sr25519Keyring::Dave.into()
}

/// Eve's account id.
pub fn eve() -> AccountId {
	Sr25519Keyring::Eve.into()
}

/// Ferdie's account id.
pub fn ferdie() -> AccountId {
	Sr25519Keyring::Ferdie.into()
}

/// Convert keyrings into `SessionKeys`.
pub fn session_keys_from_seed(seed: &str) -> SessionKeys {
	SessionKeys {
		grandpa: ed25519::Public::from_str(seed)
			.expect("should parse str seed to sr25519 public")
			.into(),
		babe: sr25519::Public::from_str(seed)
			.expect("should parse str seed to sr25519 public")
			.into(),
		im_online: sr25519::Public::from_str(seed)
			.expect("should parse str seed to sr25519 public")
			.into(),
		authority_discovery: sr25519::Public::from_str(seed)
			.expect("should parse str seed to sr25519 public")
			.into(),
		mixnet: sr25519::Public::from_str(seed)
			.expect("should parse str seed to sr25519 public")
			.into(),
		beefy: ecdsa::Public::from_str(seed)
			.expect("should parse str seed to ecdsa public")
			.into(),
	}
}

/// Returns transaction extra.
pub fn signed_extra(nonce: Nonce, extra_fee: Balance) -> SignedExtra {
	(
		frame_system::CheckNonZeroSender::new(),
		frame_system::CheckSpecVersion::new(),
		frame_system::CheckTxVersion::new(),
		frame_system::CheckGenesis::new(),
		frame_system::CheckEra::from(Era::mortal(256, 0)),
		frame_system::CheckNonce::from(nonce),
		frame_system::CheckWeight::new(),
		pallet_skip_feeless_payment::SkipCheckIfFeeless::from(
			pallet_asset_conversion_tx_payment::ChargeAssetTxPayment::from(extra_fee, None),
		),
		frame_metadata_hash_extension::CheckMetadataHash::new(false),
	)
}

/// Sign given `CheckedExtrinsic`.
pub fn sign(
	xt: CheckedExtrinsic,
	spec_version: u32,
	tx_version: u32,
	genesis_hash: [u8; 32],
	metadata_hash: Option<[u8; 32]>,
) -> UncheckedExtrinsic {
	match xt.signed {
		Some((signed, extra)) => {
			let payload = (
				xt.function,
				extra.clone(),
				spec_version,
				tx_version,
				genesis_hash,
				genesis_hash,
				metadata_hash,
			);
			let key = Sr25519Keyring::from_account_id(&signed).unwrap();
			let signature =
				payload
					.using_encoded(|b| {
						if b.len() > 256 {
							key.sign(&blake2_256(b))
						} else {
							key.sign(b)
						}
					})
					.into();
			UncheckedExtrinsic {
				signature: Some((sp_runtime::MultiAddress::Id(signed), signature, extra)),
				function: payload.0,
			}
		},
		None => UncheckedExtrinsic { signature: None, function: xt.function },
	}
}
