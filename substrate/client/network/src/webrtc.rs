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

use codec::{DecodeAll, Encode};
use hash2curve::{hash_to_scalar, ExpandMsgXmd};
use p256::{
	ecdsa::{DerSignature, SigningKey},
	elliptic_curve::consts::U48,
	pkcs8::EncodePrivateKey,
	NistP256, NonZeroScalar, SecretKey,
};
use sc_network_types::{
	multiaddr::{Multiaddr, Protocol},
	multihash::Code,
};
use sha2::Sha256;
use x509_cert::{
	builder::{Builder, CertificateBuilder, Profile},
	der::{
		asn1::{GeneralizedTime, UtcTime},
		DateTime, Encode as _,
	},
	ext::Extension,
	name::Name,
	serial_number::SerialNumber,
	spki::{SubjectPublicKeyInfoOwned, SubjectPublicKeyInfoRef},
	time::{Time, Validity},
	TbsCertificate,
};

use litep2p::transport::webrtc::DtlsCertificate;
use std::{fmt::Error, path::Path, str::FromStr};
use crate::LOG_TARGET;

/// RFC 9380 hash-to-field domain-separation tag (DST)
/// for key derivation in [`generate_certificate_from_seed`].
///
/// Changing this changes the derived key, and therefore the certificate and its certhash.
const CERTIFICATE_KEY_DST: &[u8] = b"substrate-webrtc-dtls-p256-v1";

/// Minimal certificate profile: self-issued with no extensions.
struct SelfSignedNoExtensions {
	name: Name,
}

impl Profile for SelfSignedNoExtensions {
	fn get_issuer(&self, _subject: &Name) -> Name {
		self.name.clone()
	}

	fn get_subject(&self) -> Name {
		self.name.clone()
	}

	fn build_extensions(
		&self,
		_spk: SubjectPublicKeyInfoRef<'_>,
		_issuer_spk: SubjectPublicKeyInfoRef<'_>,
		_tbs: &TbsCertificate,
	) -> Result<Vec<Extension>, x509_cert::builder::Error> {
		Ok(Vec::new())
	}
}

/// Errors that can occur while deterministically deriving a WebRTC DTLS certificate
/// from a node's secret key seed.
#[derive(Debug, thiserror::Error)]
pub enum CertificateError {
   /// Failed to derive the P-256 signing key from the seed via RFC 9380 hash-to-field.
   #[error("failed to derive a P-256 scalar from the seed: {0}")]
   ScalarDerivation(#[from] hash2curve::ExpandMsgXmdError),
   /// The RFC 9380 derivation produced a zero scalar.
   #[error("derived a zero P-256 scalar from the seed")]
   ZeroScalar,
   /// Failed to construct the certificate serial number.
   #[error("invalid certificate serial number: {0}")]
   InvalidSerialNumber(#[source] x509_cert::der::Error),
   /// Failed to construct the certificate validity period.
   #[error("invalid certificate validity period: {0}")]
   InvalidValidity(#[source] x509_cert::der::Error),
   /// Failed to construct the certificate subject name.
   #[error("invalid certificate subject name: {0}")]
   InvalidSubjectName(#[source] x509_cert::der::Error),
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
/// The DTLS key is derived from the seed via RFC 9380 `hash_to_field`,
/// so the certificate does not reveal the node key.
pub fn generate_certificate_from_seed(seed: &[u8; 32]) -> Result<DtlsCertificate, CertificateError> {
	// Derive the P-256 signing key via RFC 9380 hash-to-field.
	// Thi stretches the seed into pseudorandom bytes via `expand_message_xmd`,
	// with field reduction for producing an EC private key.
	let scalar =
		hash_to_scalar::<NistP256, ExpandMsgXmd<Sha256>, U48>(&[&seed[..]], &[CERTIFICATE_KEY_DST])?;
	let secret_scalar = Option::<NonZeroScalar>::from(NonZeroScalar::new(scalar))
		.ok_or(CertificateError::ZeroScalar)?;
	let secret = SecretKey::from(secret_scalar);
	let signing_key = SigningKey::from(&secret);

	// Fixed serial number. WebRTC peers pin the certificate by certhash and never inspect the
	// serial, a constant positive integer keeps it a valid X.509 while removing a derivation step.
	// Must stay fixed to keep the certhash stable.
	let serial =
		SerialNumber::new(&[0x01]).map_err(CertificateError::InvalidSerialNumber)?;

	// Fixed validity dates, required for determinism. WebRTC peers pin the certificate by
	// certhash and ignore its lifetime, 99991231235959Z is RFC 5280's "no well-defined
	// expiration date" value.
	let not_before = DateTime::new(2000, 1, 1, 0, 0, 0)
		.and_then(UtcTime::from_date_time)
		.map_err(CertificateError::InvalidValidity)?;
	let not_after = DateTime::new(9999, 12, 31, 23, 59, 59)
		.map(GeneralizedTime::from_date_time)
		.map_err(CertificateError::InvalidValidity)?;
	let validity = Validity::new(Time::UtcTime(not_before), Time::GeneralTime(not_after));

	// `SelfSignedNoExtensions` makes the certificate self-issued and adds no extensions,
	//  WebRTC peers only check the fingerprint.
	let name =
		Name::from_str("CN=WebRTC").map_err(CertificateError::InvalidSubjectName)?;
	let public_key = SubjectPublicKeyInfoOwned::from_key(signing_key.verifying_key())?;

	let certificate =
		CertificateBuilder::new(SelfSignedNoExtensions { name }, serial, validity, public_key)
			.and_then(|builder| builder.build::<_, DerSignature>(&signing_key))
			.map_err(CertificateError::CertificateBuild)?
			.to_der()
			.map_err(CertificateError::CertificateEncoding)?;

	// PKCS#8, loadable by both OpenSSL and rust-native DTLS backends.
	let private_key = secret
		.to_pkcs8_der()?
		.as_bytes()
		.to_vec();

	DtlsCertificate::load(certificate, private_key).map_err(CertificateError::CertificateLoad)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Compute the `/certhash/<hash>` multiaddress component of a certificate.
	fn certhash(certificate: &DtlsCertificate) -> String {
		let hash = Code::Sha2_256.digest(certificate.as_parts().0);
		Multiaddr::empty().with(Protocol::Certhash(hash)).to_string()
	}

	#[test]
	fn deterministic_certificate_generation() {
		let seed = [7u8; 32];
		let first = generate_certificate_from_seed(&seed).unwrap();
		let second = generate_certificate_from_seed(&seed).unwrap();

		assert_eq!(first.as_parts(), second.as_parts());
		assert_eq!(certhash(&first), certhash(&second));
	}

	#[test]
	fn different_seeds_produce_different_certificates() {
		let first = generate_certificate_from_seed(&[1u8; 32]).unwrap();
		let second = generate_certificate_from_seed(&[2u8; 32]).unwrap();

		assert_ne!(first.as_parts().0, second.as_parts().0);
		assert_ne!(first.as_parts().1, second.as_parts().1);
		assert_ne!(certhash(&first), certhash(&second));
	}

	#[test]
	fn stable_certhash() {
		// Pins the seed -> certificate derivation. If this test ever fails, the derivation
		// changed and the certhash published by every node relying on it breaks.
		let certificate = generate_certificate_from_seed(&[42u8; 32]).unwrap();
		assert_eq!(
			certhash(&certificate),
			"/certhash/uEiBO7A95OfR-dhec0pG9YEgtdG51mtLzYQm_aH3GEW0xcQ"
		);
	}

	#[test]
	fn generated_certificate_is_valid() {
		use p256::ecdsa::{signature::Verifier, VerifyingKey};
		use x509_cert::{der::Decode, Certificate};

		let certificate = generate_certificate_from_seed(&[8u8; 32]).unwrap();
		let (certificate_der, _) = certificate.as_parts();

		let parsed = Certificate::from_der(certificate_der).unwrap();
		let validity = parsed.tbs_certificate().validity();
		assert_eq!(validity.not_before.to_date_time(), DateTime::new(2000, 1, 1, 0, 0, 0).unwrap());
		assert_eq!(
			validity.not_after.to_date_time(),
			DateTime::new(9999, 12, 31, 23, 59, 59).unwrap()
		);

		// The self-signature is an ordinary `ecdsa-with-SHA256` signature, verifiable with
		// the certificate's own public key.
		let public_key = VerifyingKey::from_sec1_bytes(
			parsed
				.tbs_certificate()
				.subject_public_key_info()
				.subject_public_key
				.as_bytes()
				.unwrap(),
		)
		.unwrap();
		let tbs = parsed.tbs_certificate().to_der().unwrap();
		let signature = DerSignature::try_from(parsed.signature().as_bytes().unwrap()).unwrap();
		public_key.verify(&tbs, &signature).unwrap();
	}
}
