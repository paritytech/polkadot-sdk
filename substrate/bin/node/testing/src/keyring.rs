// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Test accounts.

use sp_keyring::{AccountKeyring, Sr25519Keyring, Ed25519Keyring};
use node_primitives::{AccountId, Balance, Index};
use node_runtime::{CheckedExtrinsic, UncheckedExtrinsic, SessionKeys, SignedExtra};
use sp_runtime::generic::Era;
use codec::Encode;

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
pub fn to_session_keys(
	ed25519_keyring: &Ed25519Keyring,
	sr25519_keyring: &Sr25519Keyring,
) -> SessionKeys {
	SessionKeys {
		grandpa: ed25519_keyring.to_owned().public().into(),
		babe: sr25519_keyring.to_owned().public().into(),
		im_online: sr25519_keyring.to_owned().public().into(),
		authority_discovery: sr25519_keyring.to_owned().public().into(),
	}
}

/// Returns transaction extra.
pub fn signed_extra(nonce: Index, extra_fee: Balance) -> SignedExtra {
	(
		frame_system::CheckVersion::new(),
		frame_system::CheckGenesis::new(),
		frame_system::CheckEra::from(Era::mortal(256, 0)),
		frame_system::CheckNonce::from(nonce),
		frame_system::CheckWeight::new(),
		pallet_transaction_payment::ChargeTransactionPayment::from(extra_fee),
	)
}

/// Sign given `CheckedExtrinsic`.
pub fn sign(xt: CheckedExtrinsic, version: u32, genesis_hash: [u8; 32]) -> UncheckedExtrinsic {
	match xt.signed {
		Some((signed, extra)) => {
			let payload = (xt.function, extra.clone(), version, genesis_hash, genesis_hash);
			let key = AccountKeyring::from_account_id(&signed).unwrap();
			let signature = payload.using_encoded(|b| {
				if b.len() > 256 {
					key.sign(&sp_io::hashing::blake2_256(b))
				} else {
					key.sign(b)
				}
			}).into();
			UncheckedExtrinsic {
				signature: Some((pallet_indices::address::Address::Id(signed), signature, extra)),
				function: payload.0,
			}
		}
		None => UncheckedExtrinsic {
			signature: None,
			function: xt.function,
		},
	}
}
