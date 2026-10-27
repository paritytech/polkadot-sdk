//! DKG authority set: the relay-chain validators' BEEFY paired session keys.
//!
//! The DKG reuses the validators' BEEFY (ECDSA,BLS12-381) session keys
//! (decision: Alistair, 2026-09-29), so the gadget reads them straight from
//! `pallet_beefy`'s `NextAuthorities` storage item — no dedicated runtime API
//! required, and no runtime-specific session key types. `NextAuthorities` holds
//! the keys of the validator set queued for the next session; the worker loop
//! (re)builds the authority set whenever the session rotates.

use std::collections::BTreeSet;

use codec::Decode;
use sc_client_api::{StorageKey, StorageProvider};
use sp_blockchain::Error as BlockchainError;
use sp_consensus_beefy::ecdsa_bls_crypto::Public;
use sp_crypto_hashing::twox_128;
use sp_runtime::traits::Block as BlockT;

use thiserror::Error;

/// Error while fetching the DKG authority set from chain state.
#[derive(Debug, Error)]
pub enum AuthoritySetError {
	/// `Beefy::NextAuthorities` is not present at the given block.
	#[error("Beefy::NextAuthorities not found at block (is pallet_beefy in the runtime?)")]
	MissingNextAuthorities,
	/// A storage item failed to SCALE-decode.
	#[error("failed to decode {0}")]
	DecodeFailed(&'static str),
	/// The queued validator set has no BEEFY keys.
	#[error("no BEEFY keys in Beefy::NextAuthorities")]
	NoDkgKeys,
	/// Backend error.
	#[error(transparent)]
	Blockchain(#[from] BlockchainError),
}

/// Storage key of the given pallet's storage value.
fn storage_value_key(pallet: &[u8], item: &[u8]) -> StorageKey {
	StorageKey([twox_128(pallet), twox_128(item)].concat())
}

/// Reads the current session index (`Session::CurrentIndex`) at `hash`.
///
/// `CurrentIndex` is a value-query storage item that is first written at the
/// first session rotation, so an absent value is the genesis session `0`.
pub fn session_index<Client, Block, Backend>(
	client: &Client,
	hash: Block::Hash,
) -> Result<u32, AuthoritySetError>
where
	Block: BlockT,
	Backend: sc_client_api::backend::Backend<Block>,
	Client: StorageProvider<Block, Backend>,
{
	match client.storage(hash, &storage_value_key(b"Session", b"CurrentIndex"))? {
		Some(data) => u32::decode(&mut &data.0[..])
			.map_err(|_| AuthoritySetError::DecodeFailed("Session::CurrentIndex")),
		None => Ok(0),
	}
}

/// Fetches the queued validators' BEEFY public keys (the DKG authority set)
/// from `Beefy::NextAuthorities` at `hash`.
pub fn next_beefy_authorities<Client, Block, Backend>(
	client: &Client,
	hash: Block::Hash,
) -> Result<BTreeSet<Public>, AuthoritySetError>
where
	Block: BlockT,
	Backend: sc_client_api::backend::Backend<Block>,
	Client: StorageProvider<Block, Backend>,
{
	let data = client
		.storage(hash, &storage_value_key(b"Beefy", b"NextAuthorities"))?
		.ok_or(AuthoritySetError::MissingNextAuthorities)?;
	decode_authorities(&data.0)
}

/// Decodes `Beefy::NextAuthorities` (a `BoundedVec`, encoded like a `Vec`).
fn decode_authorities(mut data: &[u8]) -> Result<BTreeSet<Public>, AuthoritySetError> {
	let authorities = Vec::<Public>::decode(&mut data)
		.map_err(|_| AuthoritySetError::DecodeFailed("Beefy::NextAuthorities"))?;
	if authorities.is_empty() {
		return Err(AuthoritySetError::NoDkgKeys);
	}
	Ok(authorities.into_iter().collect())
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use sp_core::crypto::ByteArray;

	#[test]
	fn decodes_the_queued_beefy_keys() {
		let keys =
			[1u8, 2].map(|b| Public::from_slice(&[b; 177]).expect("177-byte paired BEEFY key"));

		let set = decode_authorities(&keys.to_vec().encode()).unwrap();
		assert_eq!(set, keys.into_iter().collect());
		assert!(matches!(
			decode_authorities(&Vec::<Public>::new().encode()),
			Err(AuthoritySetError::NoDkgKeys)
		));
		assert!(matches!(decode_authorities(&[1, 2, 3]), Err(AuthoritySetError::DecodeFailed(_))));
	}

	#[test]
	fn storage_keys_match_frame_storage_value_layout() {
		// `twox128(pallet) ++ twox128(item)`, as FRAME lays out storage values.
		let key = storage_value_key(b"Beefy", b"NextAuthorities");
		assert_eq!(key.0.len(), 32);
		assert_eq!(&key.0[..16], &twox_128(b"Beefy"));
		assert_eq!(&key.0[16..], &twox_128(b"NextAuthorities"));
	}
}
