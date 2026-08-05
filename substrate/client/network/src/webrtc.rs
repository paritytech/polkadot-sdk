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

//! WebRTC DTLS cert/key generation from the node key.

use hmac::{Hmac, Mac};
use p256::{
	ecdsa::{DerSignature, SigningKey, VerifyingKey},
	pkcs8::EncodePrivateKey,
};
use sha2::{Digest, Sha256};
use x509_cert::{
	builder::{Builder, CertificateBuilder, Profile},
	der::{asn1::UtcTime, oid::db::rfc5280::ANY_EXTENDED_KEY_USAGE, DateTime, Encode as _},
	ext::pkix::ExtendedKeyUsage,
	name::Name,
	serial_number::SerialNumber,
	spki::SubjectPublicKeyInfoOwned,
	time::{Time, Validity},
};

use litep2p::{crypto::ed25519::SecretKey as Ed25519SecretKey, transport::webrtc::DtlsCertificate};
use std::str::FromStr;

/// Domain-separation tag used when deriving P-256 key from ed25519 key via HMAC-SHA256.
const CERTIFICATE_KEY_DST: &[u8] = b"substrate-webrtc-p256-v1";

/// Deterministically generate a WebRTC DTLS certificate from the node's secret key.
///
/// Returns `Err` if litep2p's [`DtlsCertificate::load`] doesn't accept DER inputs (logically
/// impossible as of litep2p v0.14.3).
pub fn derive_certificate(
	node_secret_key: Ed25519SecretKey,
) -> Result<DtlsCertificate, litep2p::Error> {
	// NOTE: none of the expects in this function are input-dependent.
	let signing_key = derive_keys(node_secret_key);
	let spki = SubjectPublicKeyInfoOwned::from_key(*signing_key.verifying_key())
		.expect("a P-256 verifying key is SPKI-encodable; qed");
	let serial = derive_serial(signing_key.verifying_key());
	let validity = generate_validity();
	let name = Name::from_str("CN=polkadot-sdk-webrtc")
		.expect("`CN=polkadot-sdk-webrtc` is a valid RDN; qed");

	let mut builder = CertificateBuilder::new(
		Profile::Manual { issuer: None }, // self-signed, no default extensions
		serial,
		validity,
		name,
		spki,
		&signing_key,
	)
	.expect("the signature algorithm is fixed and both validity bounds are in range; qed");

	// Every other WebRTC stack emits v3, but [`CertificateBuilder`] downgrades the certificates
	// without extensions to v1. Include one non-critical extension to also emit v3.
	builder
		.add_extension(&ExtendedKeyUsage(vec![ANY_EXTENDED_KEY_USAGE]))
		.expect("a single OID is DER-encodable; qed");

	// Signing fails only if the RFC 6979 nonce or either signature scalar is zero, each with
	// probability 2^-256.
	let certificate_der = builder
		.build::<DerSignature>()
		.expect("the certificate is well-formed and the signing key valid; qed")
		.to_der()
		.expect("a built certificate is DER-encodable; qed");
	let pk_pkcs8_der = signing_key
		.to_pkcs8_der()
		.expect("a P-256 signing key is PKCS#8-encodable; qed")
		.as_bytes()
		.to_vec();

	DtlsCertificate::load(certificate_der, pk_pkcs8_der)
}

/// Derive P-256 key from ed25519 key.
fn derive_keys(node_secret_key: Ed25519SecretKey) -> SigningKey {
	// P-256 private key is generated via rejection-sampling of a node-secret-key-keyed HMAC-SHA256
	// of `DST || counter` message.
	(0u8..)
		.find_map(|counter| {
			let okm = Hmac::<Sha256>::new_from_slice(node_secret_key.as_ref())
				.expect("HMAC accepts keys of any length; qed")
				.chain_update(CERTIFICATE_KEY_DST)
				.chain_update([counter])
				.finalize()
				.into_bytes();
			SigningKey::from_slice(&okm).ok()
		})
		.expect("each iteration succeeds with probability 1 - 2^-32, and we have 256 of them; qed")
}

/// Derive serial number from a public key. Not required for the operation, only used to not
/// hardcode identical/zero serials for all certificates.
fn derive_serial(pubkey: &VerifyingKey) -> SerialNumber {
	let serial_digest = Sha256::digest(pubkey.to_sec1_bytes());
	SerialNumber::new(&serial_digest[..16]).expect("below the 20-byte serial number limit; qed")
}

/// Fixed validity dates, required for determinism. WebRTC peers pin the certificate by
/// certhash and ignore its lifetime.
fn generate_validity() -> Validity {
	let not_before = DateTime::new(2000, 1, 1, 0, 0, 0)
		.and_then(UtcTime::from_date_time)
		.expect("2000-01-01 00:00:00 is a valid date within the `DateTime`/`UtcTime` range; qed")
		.into();
	Validity { not_before, not_after: Time::INFINITY }
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_network_types::{
		multiaddr::{Multiaddr, Protocol},
		multihash::Code,
	};

	/// Node secret key with every byte set to `byte`.
	fn node_key(byte: u8) -> Ed25519SecretKey {
		Ed25519SecretKey::try_from_bytes([byte; 32])
			.expect("any 32 bytes are a valid ed25519 secret key; qed")
	}

	/// Compute the `/certhash/<hash>` multiaddress component of a certificate.
	fn certhash(certificate: &DtlsCertificate) -> String {
		let hash = Code::Sha2_256.digest(certificate.as_parts().0);
		Multiaddr::empty().with(Protocol::Certhash(hash)).to_string()
	}

	#[test]
	fn deterministic_certificate_generation() {
		let key = node_key(7);
		let first = derive_certificate(key.clone()).unwrap();
		let second = derive_certificate(key).unwrap();

		assert_eq!(first.as_parts(), second.as_parts());
		assert_eq!(certhash(&first), certhash(&second));
	}

	#[test]
	fn derive_certificate_uses_version3() {
		use x509_cert::{der::Decode, Certificate, Version};

		let certificate = derive_certificate(node_key(1)).unwrap();
		let (certificate_der, _) = certificate.as_parts();

		let parsed = Certificate::from_der(certificate_der).unwrap();

		// The builder silently downgrades to v1 when the extension list is empty, which drops
		// the `[0] EXPLICIT version` field from the DER and changes the certhash.
		assert_eq!(parsed.tbs_certificate.version, Version::V3);
		assert!(parsed.tbs_certificate.extensions.as_ref().is_some_and(|ext| !ext.is_empty()));
	}

	#[test]
	fn different_node_keys_produce_different_certificates() {
		let first = derive_certificate(node_key(1)).unwrap();
		let second = derive_certificate(node_key(2)).unwrap();

		assert_ne!(first.as_parts().0, second.as_parts().0);
		assert_ne!(first.as_parts().1, second.as_parts().1);
		assert_ne!(certhash(&first), certhash(&second));
	}

	#[test]
	fn stable_certhash() {
		// Pins the node key -> certificate derivation. If this test ever fails, the derivation
		// changed and the certhash published by every node relying on it breaks.
		let certificate = derive_certificate(node_key(42)).unwrap();
		assert_eq!(
			certhash(&certificate),
			"/certhash/uEiDMlF3mQR1NNRWTKgfPitsu9g9STfkSI5cW2UPRZBvYSA"
		);
	}

	#[test]
	fn generated_certificate_is_valid() {
		use p256::ecdsa::signature::Verifier;
		use x509_cert::{der::Decode, Certificate};

		let certificate = derive_certificate(node_key(8)).unwrap();
		let (certificate_der, _) = certificate.as_parts();

		let parsed = Certificate::from_der(certificate_der).unwrap();
		let validity = parsed.tbs_certificate.validity;
		assert_eq!(validity.not_before.to_date_time(), DateTime::new(2000, 1, 1, 0, 0, 0).unwrap());
		assert_eq!(
			validity.not_after.to_date_time(),
			DateTime::new(9999, 12, 31, 23, 59, 59).unwrap()
		);

		// The self-signature is an ordinary `ecdsa-with-SHA256` signature, verifiable with
		// the certificate's own public key.
		let public_key = VerifyingKey::from_sec1_bytes(
			parsed
				.tbs_certificate
				.subject_public_key_info
				.subject_public_key
				.as_bytes()
				.unwrap(),
		)
		.unwrap();
		let tbs = parsed.tbs_certificate.to_der().unwrap();
		let signature = DerSignature::try_from(parsed.signature.as_bytes().unwrap()).unwrap();
		public_key.verify(&tbs, &signature).unwrap();
	}
}
