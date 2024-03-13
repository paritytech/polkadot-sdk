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
use node_cli::chain_spec::get_from_seed;
use node_primitives::{AccountId, Balance, Nonce};
use sp_core::{ecdsa, ed25519, sr25519};
use sp_crypto_hashing::blake2_256;
use sp_keyring::AccountKeyring;
use sp_runtime::generic::Era;

/// Alice's account id.
pub fn alice() -> AccountId {
	AccountKeyring::Alice.into()
}

/// Bob's account id.
pub fn bob() -> AccountId {
	AccountKeyring::Bob.into()
}

/// Charlie's account id.
pub fn charlie() -> AccountId {
	AccountKeyring::Charlie.into()
}

/// Dave's account id.
pub fn dave() -> AccountId {
	AccountKeyring::Dave.into()
}

/// Eve's account id.
pub fn eve() -> AccountId {
	AccountKeyring::Eve.into()
}

/// Ferdie's account id.
pub fn ferdie() -> AccountId {
	AccountKeyring::Ferdie.into()
}

/// Convert keyrings into `SessionKeys`.
pub fn session_keys_from_seed(seed: &str) -> SessionKeys {
	SessionKeys {
		grandpa: get_from_seed::<ed25519::Public>(seed).into(),
		babe: get_from_seed::<sr25519::Public>(seed).into(),
		im_online: get_from_seed::<sr25519::Public>(seed).into(),
		authority_discovery: get_from_seed::<sr25519::Public>(seed).into(),
		mixnet: get_from_seed::<sr25519::Public>(seed).into(),
		beefy: get_from_seed::<ecdsa::Public>(seed).into(),
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
	)
}

/// Sign given `CheckedExtrinsic`.
pub fn sign(
	xt: CheckedExtrinsic,
	spec_version: u32,
	tx_version: u32,
	genesis_hash: [u8; 32],
) -> UncheckedExtrinsic {
	match xt.signed {
		Some((signed, extra)) => {
			let payload =
				(xt.function, extra.clone(), spec_version, tx_version, genesis_hash, genesis_hash);
			let key = AccountKeyring::from_account_id(&signed).unwrap();
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
