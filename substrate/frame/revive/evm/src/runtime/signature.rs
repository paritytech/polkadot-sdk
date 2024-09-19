//! Signature runtime primitives.
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_core::{crypto::ByteArray, ecdsa, ed25519, sr25519};
use sp_runtime::{
	traits::{Lazy, Verify},
	AccountId32, MultiSigner, RuntimeDebug,
};

/// Signature verify that can work with any known signature types.
///
/// This is [`polkadot_sdk::sp_runtime::MultiSignature`], with an extra Ethereum variant.
#[derive(Eq, PartialEq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum MultiSignature {
	/// An Ed25519 signature.
	Ed25519(ed25519::Signature),
	/// An Sr25519 signature.
	Sr25519(sr25519::Signature),
	/// An ECDSA/SECP256k1 signature.
	Ecdsa(ecdsa::Signature),
	/// An Ethereum compatible SECP256k1 signature.
	/// TODO specify hnow it is different from Ecdsa.
	Ethereum(ecdsa::Signature),
}

impl Verify for MultiSignature {
	type Signer = MultiSigner;
	fn verify<L: Lazy<[u8]>>(&self, mut msg: L, signer: &AccountId32) -> bool {
		match (self, signer) {
			(Self::Ed25519(ref sig), who) => ed25519::Public::from_slice(who.as_ref())
				.map_or(false, |signer| sig.verify(msg, &signer)),
			(Self::Sr25519(ref sig), who) => sr25519::Public::from_slice(who.as_ref())
				.map_or(false, |signer| sig.verify(msg, &signer)),
			(Self::Ecdsa(ref sig), who) => {
				let m = sp_io::hashing::blake2_256(msg.get());
				match sp_io::crypto::secp256k1_ecdsa_recover_compressed(sig.as_ref(), &m) {
					Ok(pubkey) =>
						&sp_io::hashing::blake2_256(pubkey.as_ref()) ==
							<dyn AsRef<[u8; 32]>>::as_ref(who),
					_ => false,
				}
			},
			_ => false,
		}
	}
}
