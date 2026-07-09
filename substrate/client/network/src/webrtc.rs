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
//!
//! Only used by the litep2p network backend.

use codec::{DecodeAll, Encode};
use sc_network_types::{
	multiaddr::{Multiaddr, Protocol},
	multihash::Code,
};

use std::path::Path;

pub use litep2p::transport::webrtc::DtlsCertificate;

use crate::LOG_TARGET;

/// Default file name for the persisted WebRTC DTLS certificate.
pub const WEBRTC_CERTIFICATE_FILE: &str = "webrtc_certificate";

/// Generate a fresh WebRTC DTLS certificate.
pub fn generate_certificate() -> Result<DtlsCertificate, String> {
	DtlsCertificate::new().map_err(|err| format!("Failed to generate WebRTC certificate: {err:?}"))
}

/// Encode a certificate for on-disk storage.
pub fn encode_certificate(certificate: &DtlsCertificate) -> Vec<u8> {
	certificate.as_parts().encode()
}

/// Decode a certificate previously encoded with [`encode_certificate`].
///
/// Accepts both the raw encoded bytes and their hex-encoded form, as produced by the
/// `key generate-webrtc-certificate` subcommand.
pub fn decode_certificate(bytes: &[u8]) -> Result<DtlsCertificate, String> {
	let (certificate, private_key) = <(Vec<u8>, Vec<u8>)>::decode_all(&mut &bytes[..])
		.ok()
		.or_else(|| {
			let hex = std::str::from_utf8(bytes).ok()?;
			let raw = array_bytes::hex2bytes(hex.trim()).ok()?;
			<(Vec<u8>, Vec<u8>)>::decode_all(&mut raw.as_slice()).ok()
		})
		.ok_or_else(|| "Failed to decode WebRTC certificate".to_string())?;

	DtlsCertificate::load(certificate, private_key)
		.map_err(|err| format!("Failed to load WebRTC certificate: {err:?}"))
}

/// Compute the `/certhash/<hash>` multiaddress component of a certificate.
pub fn certhash(certificate: &DtlsCertificate) -> String {
	let hash = Code::Sha2_256.digest(certificate.as_parts().0);
	Multiaddr::empty().with(Protocol::Certhash(hash)).to_string()
}

/// Load the encoded WebRTC DTLS certificate from `file`, or generate a new one,
/// persist it (0600 on unix), and return it.
pub fn read_or_generate_certificate(file: &Path) -> Result<DtlsCertificate, String> {
	match std::fs::read(file) {
		Ok(bytes) => {
			log::info!(target: LOG_TARGET, "WebRTC certificate found at {file:?}, using existing one");
			decode_certificate(&bytes)
		},
		Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
			log::info!(target: LOG_TARGET, "No WebRTC certificate found at {file:?}, generating a new one");
			if let Some(parent) = file.parent() {
				std::fs::create_dir_all(parent).map_err(|err| {
					format!("Failed to create WebRTC certificate directory: {err:?}")
				})?;
			}
			let certificate = generate_certificate()?;
			crate::config::write_secret_file(file, &encode_certificate(&certificate)).map_err(
				|err| format!("Failed to persist WebRTC certificate to {file:?}: {err:?}"),
			)?;
			Ok(certificate)
		},
		Err(err) => Err(format!("Failed to read WebRTC certificate at {file:?}: {err:?}")),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn certificate_encode_decode_roundtrip() {
		let certificate = generate_certificate().unwrap();
		let encoded = encode_certificate(&certificate);

		let decoded = decode_certificate(&encoded).unwrap();
		assert_eq!(decoded.as_parts(), certificate.as_parts());

		let hex = array_bytes::bytes2hex("", &encoded);
		let decoded = decode_certificate(hex.as_bytes()).unwrap();
		assert_eq!(decoded.as_parts(), certificate.as_parts());
	}

	#[test]
	fn decode_certificate_rejects_garbage() {
		assert!(decode_certificate(b"not a certificate").is_err());
	}

	#[test]
	fn read_or_generate_persists_certificate() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join(WEBRTC_CERTIFICATE_FILE);

		let generated = read_or_generate_certificate(&file).unwrap();
		let reloaded = read_or_generate_certificate(&file).unwrap();

		assert_eq!(generated.as_parts(), reloaded.as_parts());
		assert_eq!(certhash(&generated), certhash(&reloaded));
	}

	#[test]
	fn read_or_generate_fails_on_corrupt_certificate() {
		let dir = tempfile::tempdir().unwrap();
		let file = dir.path().join(WEBRTC_CERTIFICATE_FILE);

		std::fs::write(&file, b"corrupt").unwrap();
		assert!(read_or_generate_certificate(&file).is_err());
	}
}
