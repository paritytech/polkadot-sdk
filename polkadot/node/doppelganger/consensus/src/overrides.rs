//! Host function overrides for signature verification.

use sp_core::{ecdsa, ed25519, sr25519};
use sp_runtime_interface::runtime_interface;

#[cfg(feature = "std")]
#[runtime_interface]
trait Crypto {
	fn ecdsa_verify(_sig: &ecdsa::Signature, _msg: &[u8], _pub_key: &ecdsa::Public) -> bool {
		true
	}

	#[version(2)]
	fn ecdsa_verify(_sig: &ecdsa::Signature, _msg: &[u8], _pub_key: &ecdsa::Public) -> bool {
		true
	}

	fn ed25519_verify(_sig: &ed25519::Signature, _msg: &[u8], _pub_key: &ed25519::Public) -> bool {
		true
	}

	fn sr25519_verify(_sig: &sr25519::Signature, _msg: &[u8], _pub_key: &sr25519::Public) -> bool {
		true
	}

	#[version(2)]
	fn sr25519_verify(_sig: &sr25519::Signature, _msg: &[u8], _pub_key: &sr25519::Public) -> bool {
		true
	}
}

/// Provides host functions that overrides runtime signature verification
/// to always return true.
pub type SignatureVerificationOverride = crypto::HostFunctions;

// This is here to get rid of the warnings.
#[allow(unused_imports, dead_code)]
use self::crypto::{ecdsa_verify, ed25519_verify, sr25519_verify};
