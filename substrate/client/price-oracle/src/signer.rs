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

//! The local signer key and signing of reports.

use codec::Decode;
use sp_application_crypto::AppCrypto;
use sp_keystore::KeystorePtr;
use sp_price_oracle::{PriceReport, SignedPriceReport};
use sp_runtime::RuntimeAppPublic;

const LOG_TARGET: &str = "price-oracle";

/// The key among `signers` whose private part the keystore holds. The first such key in the
/// order of `signers`, or `None` if the keystore holds none of them.
pub fn local_signer<Id>(keystore: &KeystorePtr, signers: &[Id]) -> Option<Id>
where
	Id: RuntimeAppPublic + AppCrypto + Clone,
{
	signers
		.iter()
		.find(|id| {
			keystore.has_keys(&[(
				<Id as RuntimeAppPublic>::to_raw_vec(id),
				<Id as RuntimeAppPublic>::ID,
			)])
		})
		.cloned()
}

/// Sign `report` with the key of `signer` held by the keystore.
///
/// Returns `None` if the keystore does not hold the key, refuses to sign, or returns a
/// signature that cannot be decoded.
pub fn sign_report<Id>(
	keystore: &KeystorePtr,
	signer: Id,
	report: PriceReport,
) -> Option<SignedPriceReport<Id, <Id as RuntimeAppPublic>::Signature>>
where
	Id: RuntimeAppPublic + AppCrypto,
{
	let payload = report.signing_payload();
	let public = <Id as RuntimeAppPublic>::to_raw_vec(&signer);
	let signed = keystore.sign_with(
		<Id as RuntimeAppPublic>::ID,
		<Id as AppCrypto>::CRYPTO_ID,
		&public,
		&payload,
	);
	let bytes = match signed {
		Ok(Some(bytes)) => bytes,
		Ok(None) => {
			log::warn!(target: LOG_TARGET, "Signer key is no longer in the keystore");
			return None;
		},
		Err(e) => {
			log::warn!(target: LOG_TARGET, "Keystore refused to sign: {e}");
			return None;
		},
	};
	let Ok(signature) = Decode::decode(&mut &bytes[..]) else {
		log::warn!(target: LOG_TARGET, "Keystore returned an undecodable signature");
		return None;
	};
	Some(SignedPriceReport { report, signer, signature })
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_keystore::LocalKeystore;
	use sp_consensus_aura::sr25519::AuthorityId;
	use sp_keystore::Keystore;
	use sp_price_oracle::{Anchor, PairId, Price, Quote};
	use std::sync::Arc;

	const KEY_TYPE: sp_core::crypto::KeyTypeId = sp_consensus_aura::sr25519::AuthorityPair::ID;

	/// A keystore holding Aura keys for the given seeds, and the ids of all seeds.
	fn keystore_with(held: &[&str], all: &[&str]) -> (KeystorePtr, Vec<AuthorityId>) {
		let keystore = LocalKeystore::in_memory();
		let ids = all
			.iter()
			.map(|seed| {
				let suri = format!("//{seed}");
				if held.contains(seed) {
					keystore.sr25519_generate_new(KEY_TYPE, Some(&suri)).unwrap().into()
				} else {
					let other = LocalKeystore::in_memory();
					other.sr25519_generate_new(KEY_TYPE, Some(&suri)).unwrap().into()
				}
			})
			.collect();
		(Arc::new(keystore), ids)
	}

	fn report() -> PriceReport {
		PriceReport {
			anchor: Anchor(7),
			quotes: vec![Quote { pair: PairId(1), price: Price::from_rational(4_206, 1_000) }],
		}
	}

	#[test]
	fn local_signer_is_the_first_accepted_key_held() {
		let (keystore, ids) = keystore_with(&["bob", "charlie"], &["alice", "bob", "charlie"]);
		assert_eq!(local_signer(&keystore, &ids), Some(ids[1].clone()));
		// Set order decides, not keystore order.
		let reversed: Vec<_> = ids.iter().rev().cloned().collect();
		assert_eq!(local_signer(&keystore, &reversed), Some(ids[2].clone()));
	}

	#[test]
	fn no_local_signer_without_a_held_key() {
		let (keystore, ids) = keystore_with(&[], &["alice", "bob"]);
		assert_eq!(local_signer(&keystore, &ids), None);
		assert_eq!(local_signer::<AuthorityId>(&keystore, &[]), None);
	}

	#[test]
	fn signed_report_verifies() {
		let (keystore, ids) = keystore_with(&["alice"], &["alice"]);
		let signed = sign_report(&keystore, ids[0].clone(), report()).unwrap();
		assert!(signed.verify_signature());
		assert_eq!(signed.report, report());
		assert_eq!(signed.signer, ids[0]);
	}

	#[test]
	fn signing_with_a_key_not_held_fails() {
		let (keystore, ids) = keystore_with(&[], &["alice"]);
		assert!(sign_report(&keystore, ids[0].clone(), report()).is_none());
	}
}
