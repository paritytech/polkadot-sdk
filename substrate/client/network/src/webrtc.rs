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
//!
//! The certificate is assembled field by field. Its DER encoding is hashed into the
//! `/certhash/...` component of every address the node advertises, so it must stay
//! bit-identical across dependency upgrades: no field may be left to a library default that
//! the X.509 specs leave open.

use hmac::{Hmac, Mac};
use p256::{
	ecdsa::{
		signature::{hazmat::PrehashSigner, SignatureEncoding},
		DerSignature, SigningKey,
	},
	elliptic_curve::sec1::ToEncodedPoint,
	pkcs8::EncodePrivateKey,
	EncodedPoint,
};
use sha2::{Digest, Sha256};
use x509_cert::{
	attr::AttributeTypeAndValue,
	der::{
		asn1::{Any, BitString, GeneralizedTime, SetOfVec, UtcTime},
		oid::db::{
			rfc4519::COMMON_NAME,
			rfc5912::{ECDSA_WITH_SHA_256, ID_EC_PUBLIC_KEY, SECP_256_R_1},
		},
		DateTime, Encode as _, Tag,
	},
	name::{Name, RelativeDistinguishedName},
	serial_number::SerialNumber,
	spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned},
	time::{Time, Validity},
	Certificate, TbsCertificate, Version,
};

use litep2p::{crypto::ed25519::SecretKey as Ed25519SecretKey, transport::webrtc::DtlsCertificate};

/// Domain-separation tag used when deriving P-256 key from ed25519 key via HMAC-SHA256.
const CERTIFICATE_KEY_DST: &[u8] = b"substrate-webrtc-p256-v1";

/// Common name used for both subject and issuer, the certificate being self-signed.
const SUBJECT_COMMON_NAME: &str = "polkadot-sdk-webrtc";

/// Deterministically generate a WebRTC DTLS certificate from the node's secret key.
pub fn derive_certificate(
	node_secret_key: Ed25519SecretKey,
) -> Result<DtlsCertificate, litep2p::Error> {
	// NOTE: none of the expects in this function are input-dependent.
	let signing_key = derive_keys(node_secret_key);

	// RFC 5758 §3.2: `parameters` MUST be absent for ECDSA.
	let signature_algorithm =
		AlgorithmIdentifierOwned { oid: ECDSA_WITH_SHA_256, parameters: None };
	let name = common_name();
	// Uncompressed SEC1 form, requested explicitly: the `EncodePublicKey` impl would pick the
	// point form and the curve parameter encoding for us, and RFC 5480 permits several.
	let point = signing_key.verifying_key().as_affine().to_encoded_point(false);

	let tbs_certificate = TbsCertificate {
		// Every other WebRTC stack emits V3, let's do the same, even though it can be V1.
		version: Version::V3,
		serial_number: derive_serial(&point),
		signature: signature_algorithm.clone(),
		issuer: name.clone(),
		validity: validity(),
		subject: name,
		subject_public_key_info: SubjectPublicKeyInfoOwned {
			// RFC 5480 §2.1.1: `namedCurve`, rather than `implicitCurve` or `specifiedCurve`.
			algorithm: AlgorithmIdentifierOwned {
				oid: ID_EC_PUBLIC_KEY,
				parameters: Some(Any::from(SECP_256_R_1)),
			},
			subject_public_key: BitString::new(0, point.as_bytes())
				.expect("a SEC1 point is a whole number of octets; qed"),
		},
		// RFC 5280 §4.1.2.8: conforming CAs MUST NOT generate unique identifiers.
		issuer_unique_id: None,
		subject_unique_id: None,
		extensions: None,
	};

	let tbs_der = tbs_certificate.to_der().expect("the certificate is well-formed; qed");

	// `PrehashSigner` is RFC 6979 deterministic ECDSA. Determinism of this whole module rests on
	// it. If its implementation ever changes and produces different signature than checked in the
	// tests, we will need to include the old implementation in the source code of `sc-network`.
	//
	// Signing fails only if the RFC 6979 nonce or either signature scalar is zero, each with
	// probability 2^-256.
	let signature: DerSignature = signing_key
		.sign_prehash(&Sha256::digest(&tbs_der))
		.expect("the signing key is valid; qed");

	let certificate_der = Certificate {
		tbs_certificate,
		signature_algorithm,
		// RFC 3279 §2.2.3: the DER-encoded `Ecdsa-Sig-Value`, with no unused bits.
		signature: BitString::new(0, signature.to_vec())
			.expect("an ECDSA signature is a whole number of octets; qed"),
	}
	.to_der()
	.expect("a well-formed certificate is DER-encodable; qed");

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
fn derive_serial(point: &EncodedPoint) -> SerialNumber {
	let digest = Sha256::digest(point.as_bytes());

	// A `u64` names exactly one integer, so the DER encoding follows from the spec alone: passing
	// a byte string instead would leave it to `SerialNumber` whether to read it as signed.
	SerialNumber::from(u64::from_be_bytes(digest[..8].try_into().expect("8 of 32 bytes; qed")))
}

/// `CN=polkadot-sdk-webrtc`, built explicitly so the attribute's string type is pinned.
fn common_name() -> Name {
	let attribute = AttributeTypeAndValue {
		oid: COMMON_NAME,
		value: Any::new(Tag::Utf8String, SUBJECT_COMMON_NAME.as_bytes())
			.expect("the common name is a valid `UTF8String`; qed"),
	};
	let rdn = SetOfVec::try_from(vec![attribute]).expect("a single-element set is sorted; qed");

	Name::from(vec![RelativeDistinguishedName::from(rdn)])
}

/// Fixed validity dates, required for determinism. WebRTC peers pin the certificate by
/// certhash and ignore its lifetime.
fn validity() -> Validity {
	// RFC 5280 §4.1.2.5.1: dates through 2049 MUST be encoded as `UTCTime`.
	let not_before = UtcTime::from_date_time(
		DateTime::new(2000, 1, 1, 0, 0, 0).expect("2000-01-01 00:00:00 is a valid date; qed"),
	)
	.expect("2000 is within the `UTCTime` range; qed");
	// RFC 5280 §4.1.2.5: the encoding for certificates with no well-defined expiration date.
	let not_after = GeneralizedTime::from_date_time(
		DateTime::new(9999, 12, 31, 23, 59, 59).expect("9999-12-31 23:59:59 is a valid date; qed"),
	);

	Validity { not_before: Time::UtcTime(not_before), not_after: Time::GeneralTime(not_after) }
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
		use x509_cert::der::Decode;

		let certificate = derive_certificate(node_key(1)).unwrap();
		let (certificate_der, _) = certificate.as_parts();

		let parsed = Certificate::from_der(certificate_der).unwrap();

		// Both fields are optional in the DER, and dropping either changes the certhash: `version`
		// because v1 is the ASN.1 DEFAULT, `extensions` because it is absent rather than empty.
		assert_eq!(parsed.tbs_certificate.version, Version::V3);
		assert!(parsed.tbs_certificate.extensions.is_none());
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
		//
		// Two vectors, because the serial is encoded differently depending on the top bit of its
		// first byte: `node_key(7)` takes the `0x00` sign-byte branch, `node_key(42)` does not.
		assert_eq!(
			certhash(&derive_certificate(node_key(7)).unwrap()),
			"/certhash/uEiAXqXtF_3QIfMcgXwMgneoB4EuSE_EcpGvKhY4yz7HfcA"
		);
		assert_eq!(
			certhash(&derive_certificate(node_key(42)).unwrap()),
			"/certhash/uEiAWsH8V-_VMveqodSJYiAhW5FikqSzBNLV0FyeEb_oetA"
		);
	}

	#[test]
	fn golden_certificate() {
		// Same guarantee as `stable_certhash`, but a diffable artifact: on failure, compare the
		// two encodings field by field to see which one moved.
		//
		//     openssl asn1parse -inform DER -in substrate/client/network/res/webrtc_cert.der
		//
		// Only ever regenerate this alongside a deliberate, documented derivation change: every
		// node's advertised `/certhash/...` changes with it.
		const GOLDEN: &[u8] = include_bytes!("../res/webrtc_cert.der");

		let certificate = derive_certificate(node_key(42)).unwrap();
		assert_eq!(certificate.as_parts().0.as_slice(), GOLDEN);
	}

	#[test]
	fn generated_certificate_is_valid() {
		use p256::ecdsa::{signature::Verifier, VerifyingKey};
		use x509_cert::der::Decode;

		let certificate = derive_certificate(node_key(8)).unwrap();
		let (certificate_der, _) = certificate.as_parts();

		let parsed = Certificate::from_der(certificate_der).unwrap();
		let validity = parsed.tbs_certificate.validity;
		assert_eq!(validity.not_before.to_date_time(), DateTime::new(2000, 1, 1, 0, 0, 0).unwrap());
		assert_eq!(
			validity.not_after.to_date_time(),
			DateTime::new(9999, 12, 31, 23, 59, 59).unwrap()
		);
		// The choice of time type is part of the encoding, not just the instant it denotes.
		assert!(matches!(validity.not_before, Time::UtcTime(_)));
		assert!(matches!(validity.not_after, Time::GeneralTime(_)));

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
