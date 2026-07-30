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

//! WebRTC DTLS certificate utilities.
//!
//! The DTLS certificate determines the node's WebRTC `/certhash` and therefore its
//! advertised WebRTC multiaddresses. Reusing a certificate across restarts keeps the
//! certhash stable, similarly to how reusing the node secret key keeps the peer id stable.

use p256::{
	ecdsa::{DerSignature, SigningKey},
	elliptic_curve::hash2curve::{ExpandMsgXmd, GroupDigest},
	pkcs8::EncodePrivateKey,
	NistP256, NonZeroScalar, SecretKey,
};
use sha2::{Digest, Sha256};
use x509_cert::{
	builder::{Builder, CertificateBuilder, Profile},
	der::{
		self,
		asn1::{GeneralizedTime, ObjectIdentifier, UtcTime},
		oid::AssociatedOid,
		DateTime, Encode as _, EncodeValue, FixedTag, Length, Tag, Writer,
	},
	ext::{pkix::constraints::BasicConstraints, AsExtension, Extension},
	name::Name,
	serial_number::SerialNumber,
	spki::SubjectPublicKeyInfoOwned,
	time::{Time, Validity},
};

use litep2p::transport::webrtc::DtlsCertificate;
use std::str::FromStr;

/// RFC 9380 hash-to-field domain-separation tag (DST)
/// for key derivation in [`derive_certificate`].
///
/// Changing this changes the derived key, and therefore the certificate and its certhash.
const CERTIFICATE_KEY_DST: &[u8] = b"substrate-webrtc-dtls-p256-v1";

/// Domain-separation context for deriving the certificate's serial number from the seed
/// in [`derive_certificate`].
///
/// Distinct from [`CERTIFICATE_KEY_DST`] so the derived key and the derived serial number
/// have no accidental structural relationship. Changing this changes the derived serial
/// number, and therefore the certificate and its certhash.
const CERTIFICATE_SERIAL_DST: &[u8] = b"substrate-webrtc-dtls-certificate-serial-v1";

/// The certificate needs at least one extension to stay at v3,
/// and `BasicConstraints` is the minimal end-entity choice.
struct BasicExtension(BasicConstraints);

impl AssociatedOid for BasicExtension {
	const OID: ObjectIdentifier = BasicConstraints::OID;
}

impl FixedTag for BasicExtension {
	const TAG: Tag = Tag::Sequence;
}

impl EncodeValue for BasicExtension {
	fn value_len(&self) -> der::Result<Length> {
		self.0.value_len()
	}

	fn encode_value(&self, writer: &mut impl Writer) -> der::Result<()> {
		self.0.encode_value(writer)
	}
}

impl AsExtension for BasicExtension {
	fn critical(&self, _subject: &Name, _extensions: &[Extension]) -> bool {
		false
	}
}

/// Errors that can occur while deterministically deriving a WebRTC DTLS certificate
/// from a node's secret key seed.
#[derive(Debug, thiserror::Error)]
pub enum CertificateError {
	/// Failed to derive the P-256 signing key from the seed via RFC 9380 hash-to-field.
	#[error("failed to derive a P-256 scalar from the seed: {0}")]
	ScalarDerivation(#[from] p256::elliptic_curve::Error),
	/// The RFC 9380 derivation produced a zero scalar.
	#[error("derived a zero P-256 scalar from the seed")]
	ZeroScalar,
	/// Failed to encode the certificate public key.
	#[error("failed to encode certificate public key: {0}")]
	PublicKeyEncoding(#[from] x509_cert::spki::Error),
	/// Failed to build the certificate.
	#[error("failed to build certificate: {0}")]
	CertificateBuild(#[source] x509_cert::builder::Error),
	/// Failed to DER-encode the certificate.
	#[error("failed to encode certificate: {0}")]
	CertificateEncoding(#[source] x509_cert::der::Error),
	/// Failed to PKCS#8-encode the certificate private key.
	#[error("failed to encode certificate private key: {0}")]
	PrivateKeyEncoding(#[from] p256::pkcs8::Error),
	/// Failed to load the generated certificate/key pair into a WebRTC DTLS certificate.
	#[error("failed to load WebRTC certificate: {0}")]
	CertificateLoad(#[from] litep2p::Error),
}

/// Deterministically generate a WebRTC DTLS certificate from a 32-byte seed, typically the
/// node's Ed25519 secret key bytes.
///
/// The DTLS key is derived from the seed via RFC 9380 `hash_to_field`, and the certificate's
/// serial number via a domain-separated SHA-256 hash of the seed, neither derivation reveals
/// the node key.
///
/// `webrtc_seed` rotates the derived key, and therefore the certificate and its certhash,
/// while leaving the node key and peer id untouched.
/// Every value, including `Some(0)`, derives a distinct certificate from `None`.
pub fn derive_certificate(
	seed: &[u8; 32],
	webrtc_seed: Option<u64>,
) -> Result<DtlsCertificate, CertificateError> {
	let (signing_key, private_key) = derive_keys(seed, webrtc_seed)?;
	let public_key = SubjectPublicKeyInfoOwned::from_key(*signing_key.verifying_key())?;
	let serial = derive_serial(seed)?;
	let validity = generate_validity();
	let name = Name::from_str("CN=polkadot-sdk-webrtc")
		.expect("polkadot-sdk-webrtc is a valid RDN string; qed");

	// `Profile::Manual` opts out of the builder's default extension set,
	//  and a `None` issuer makes the certificate self-issued.
	//  WebRTC peers only check the fingerprint.
	let mut builder = CertificateBuilder::new(
		Profile::Manual { issuer: None },
		serial,
		validity,
		name,
		public_key,
		&signing_key,
	)
	.map_err(CertificateError::CertificateBuild)?;

	// The builder downgrades the certificate to v1 whenever the extension list is empty.
	// One end-entity constraint keeps us on v3.
	builder
		.add_extension(&BasicExtension(BasicConstraints {
			ca: false,
			path_len_constraint: None,
		}))
		.map_err(CertificateError::CertificateBuild)?;

	let certificate = builder
		.build::<DerSignature>()
		.map_err(CertificateError::CertificateBuild)?
		.to_der()
		.map_err(CertificateError::CertificateEncoding)?;

	DtlsCertificate::load(certificate, private_key).map_err(CertificateError::CertificateLoad)
}

/// Returns the derivated signing and private keys.
fn derive_keys(
	seed: &[u8],
	webrtc_seed: Option<u64>,
) -> Result<(SigningKey, Vec<u8>), CertificateError> {
	// The DST parts are concatenated, so an absent seed leaves the tag unchanged.
	// A present one always contributes 8 bytes, keeping `Some(0)` distinct from `None`.
	let webrtc_seed = webrtc_seed.map(u64::to_be_bytes);
	let dst_suffix: &[u8] = webrtc_seed.as_ref().map_or(&[], |bytes| bytes);

	// Derive the P-256 signing key via RFC 9380 hash-to-field.
	// This stretches the seed into pseudorandom bytes via `expand_message_xmd`,
	// with field reduction for producing an EC private key.
	let scalar = NistP256::hash_to_scalar::<ExpandMsgXmd<Sha256>>(
		&[&seed],
		&[CERTIFICATE_KEY_DST, dst_suffix],
	)?;
	let secret_scalar =
		NonZeroScalar::new(scalar).into_option().ok_or(CertificateError::ZeroScalar)?;
	let secret = SecretKey::from(secret_scalar);
	let signing_key = SigningKey::from(&secret);

	// PKCS#8, loadable by both OpenSSL and rust-native DTLS backends.
	let private_key = secret.to_pkcs8_der()?.as_bytes().to_vec();

	Ok((signing_key, private_key))
}

/// Serial number derived from the seed.
fn derive_serial(seed: &[u8]) -> Result<SerialNumber, CertificateError> {
	// WebRTC peers pin the certificate by certhash
	// and never inspect the serial, so this isn't a security requirement, it just keeps
	// `(Issuer, Serial)` meaningful for X.509 tooling that assumes that pair identifies a
	// certificate, since every node otherwise shares the same hardcoded issuer name.
	let serial_digest =
		Sha256::new().chain_update(CERTIFICATE_SERIAL_DST).chain_update(seed).finalize();
	Ok(SerialNumber::new(&serial_digest[..16]).expect("below the 20-byte serial number limit; qed"))
}

/// Fixed validity dates, required for determinism. WebRTC peers pin the certificate by
/// certhash and ignore its lifetime.
fn generate_validity() -> Validity {
	let not_before = DateTime::new(2000, 1, 1, 0, 0, 0)
		.and_then(UtcTime::from_date_time)
		.expect("2000-01-01 00:00:00 is a valid date within the UTCTime range; qed");
	let not_after = DateTime::new(9999, 12, 31, 23, 59, 59)
		.map(GeneralizedTime::from_date_time)
		.expect("9999-12-31 23:59:59 is a valid date, the maximum DER DateTime supports; qed");
	Validity { not_before: Time::UtcTime(not_before), not_after: Time::GeneralTime(not_after) }
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_network_types::{
		multiaddr::{Multiaddr, Protocol},
		multihash::Code,
	};

	/// Compute the `/certhash/<hash>` multiaddress component of a certificate.
	fn certhash(certificate: &DtlsCertificate) -> String {
		let hash = Code::Sha2_256.digest(certificate.as_parts().0);
		Multiaddr::empty().with(Protocol::Certhash(hash)).to_string()
	}

	#[test]
	fn deterministic_certificate_generation() {
		let seed = [7u8; 32];
		let first = derive_certificate(&seed, None).unwrap();
		let second = derive_certificate(&seed, None).unwrap();

		assert_eq!(first.as_parts(), second.as_parts());
		assert_eq!(certhash(&first), certhash(&second));
	}

	#[test]
	fn derive_certificate_uses_version3() {
		use x509_cert::{der::Decode, Certificate, Version};

		let seed = [1u8; 32];
		let certificate = derive_certificate(&seed, None).unwrap();
		let (certificate_der, _) = certificate.as_parts();

		let parsed = Certificate::from_der(certificate_der).unwrap();

		// The builder silently downgrades to v1 when the extension list is empty, which drops
		// the `[0] EXPLICIT version` field from the DER and changes the certhash.
		assert_eq!(parsed.tbs_certificate.version, Version::V3);
		assert!(parsed.tbs_certificate.extensions.as_ref().is_some_and(|ext| !ext.is_empty()));
	}

	#[test]
	fn different_seeds_produce_different_certificates() {
		let first = derive_certificate(&[1u8; 32], None).unwrap();
		let second = derive_certificate(&[2u8; 32], None).unwrap();

		assert_ne!(first.as_parts().0, second.as_parts().0);
		assert_ne!(first.as_parts().1, second.as_parts().1);
		assert_ne!(certhash(&first), certhash(&second));
	}

	#[test]
	fn stable_certhash() {
		// Pins the seed -> certificate derivation. If this test ever fails, the derivation
		// changed and the certhash published by every node relying on it breaks.
		let certificate = derive_certificate(&[42u8; 32], None).unwrap();
		assert_eq!(
			certhash(&certificate),
			"/certhash/uEiBU1jyJxUmj0eUBMpzL8lhUYvBy5UdXdhEAH_GbC7Kxcg"
		);
	}

	#[test]
	fn webrtc_seed_rotates_certificate() {
		use x509_cert::{der::Decode, Certificate};

		let seed = [3u8; 32];
		let unseeded = derive_certificate(&seed, None).unwrap();
		let first = derive_certificate(&seed, Some(1)).unwrap();
		let second = derive_certificate(&seed, Some(2)).unwrap();

		// Every seed yields a distinct certificate, key and certhash, for the same node key.
		let certificates = [&unseeded, &first, &second];
		for (index, left) in certificates.iter().enumerate() {
			for right in certificates.iter().skip(index + 1) {
				assert_ne!(left.as_parts().0, right.as_parts().0);
				assert_ne!(left.as_parts().1, right.as_parts().1);
				assert_ne!(certhash(left), certhash(right));
			}
		}

		// The serial number is derived from the node key alone, so rotation leaves it alone.
		let serial = |certificate: &DtlsCertificate| {
			Certificate::from_der(certificate.as_parts().0)
				.unwrap()
				.tbs_certificate
				.serial_number
				.clone()
		};
		assert_eq!(serial(&unseeded), serial(&first));
		assert_eq!(serial(&unseeded), serial(&second));
	}

	#[test]
	fn webrtc_seed_derivation_is_deterministic() {
		let seed = [9u8; 32];
		let first = derive_certificate(&seed, Some(1)).unwrap();
		let second = derive_certificate(&seed, Some(1)).unwrap();

		assert_eq!(first.as_parts(), second.as_parts());
		assert_eq!(certhash(&first), certhash(&second));
	}

	#[test]
	fn boundary_webrtc_seeds_are_distinct() {
		// A present seed always contributes its 8 bytes to the DST, so seed `0` is a rotation
		// rather than an alias of "no seed".
		let seed = [11u8; 32];
		let unseeded = derive_certificate(&seed, None).unwrap();
		let zero = derive_certificate(&seed, Some(0)).unwrap();
		let max = derive_certificate(&seed, Some(u64::MAX)).unwrap();

		assert_ne!(certhash(&unseeded), certhash(&zero));
		assert_ne!(certhash(&unseeded), certhash(&max));
		assert_ne!(certhash(&zero), certhash(&max));
	}

	#[test]
	fn stable_certhash_with_webrtc_seed() {
		let certificate = derive_certificate(&[42u8; 32], Some(1)).unwrap();
		assert_eq!(
			certhash(&certificate),
			"/certhash/uEiCSnl2zX3egog3X8nATV2RcJXCXHMjF5GTVUtrKFbcGjQ"
		);
	}

	#[test]
	fn generated_certificate_is_valid() {
		use p256::ecdsa::{signature::Verifier, VerifyingKey};
		use x509_cert::{der::Decode, Certificate};

		let certificate = derive_certificate(&[8u8; 32], None).unwrap();
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
