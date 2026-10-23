//! DKG authority set: the validator accounts and their BEEFY paired session keys.
//!
//! The DKG reuses the validators' BEEFY (ECDSA,BLS12-381) session keys
//! (decision: Alistair, 2026-09-29), so the gadget reads them straight from the
//! session pallet's `QueuedKeys` storage item — no dedicated runtime API
//! required. `QueuedKeys` holds the keys of the upcoming session; the worker
//! loop (re)builds the authority set whenever the session rotates.

use std::collections::{BTreeMap, BTreeSet};

use codec::Decode;
use sc_client_api::{StorageKey, StorageProvider};
use sp_blockchain::Error as BlockchainError;
use sp_consensus_beefy::ecdsa_bls_crypto::Public;
use sp_core::crypto::key_types::BEEFY as BEEFY_KEY_TYPE;
use sp_core::crypto::ByteArray;
use sp_crypto_hashing::twox_128;
use sp_runtime::traits::{Block as BlockT, OpaqueKeys};

use thiserror::Error;

/// Error while fetching the DKG authority set from chain state.
#[derive(Debug, Error)]
pub enum AuthoritySetError {
	/// `Session::QueuedKeys` is not present at the given block.
	#[error("Session::QueuedKeys not found at block")]
	MissingQueuedKeys,
	/// `Session::QueuedKeys` failed to SCALE-decode.
	#[error("failed to decode Session::QueuedKeys")]
	DecodeFailed,
	/// No validator has a registered DKG session key.
	#[error("no BEEFY session keys registered (is the 'beef' key in the runtime SessionKeys?)")]
	NoDkgKeys,
	/// Backend error.
	#[error(transparent)]
	Blockchain(#[from] BlockchainError),
}

/// The DKG authority set: validator account id -> BEEFY paired public key.
pub type AuthoritySet<AccountId> = BTreeMap<AccountId, Public>;

/// Fetches the queued validators' BEEFY public keys (the DKG authority set).
///
/// Same as [`queued_dkg_keys`], but dropping the account ids — the gadget only
/// needs the set of keys.
pub fn queued_dkg_public_keys<Client, Block, Backend, AccountId, Keys>(
	client: &Client,
	hash: Block::Hash,
) -> Result<BTreeSet<Public>, AuthoritySetError>
where
	Block: BlockT,
	Backend: sc_client_api::backend::Backend<Block>,
	Client: StorageProvider<Block, Backend>,
	AccountId: Decode + Ord,
	Keys: OpaqueKeys + Decode,
{
	Ok(queued_dkg_keys::<_, _, _, AccountId, Keys>(client, hash)?.into_values().collect())
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
	let mut storage_key = twox_128(b"Session").to_vec();
	storage_key.extend_from_slice(&twox_128(b"CurrentIndex"));
	match client.storage(hash, &StorageKey(storage_key))? {
		Some(data) => u32::decode(&mut &data.0[..]).map_err(|_| AuthoritySetError::DecodeFailed),
		None => Ok(0),
	}
}

///
/// Fetches the queued validators' BEEFY keys from `Session::QueuedKeys` at `hash`.
///
/// `AccountId` and `Keys` are the runtime's session types, supplied by the caller
/// integrating the gadget (e.g. `cumulus_test_runtime::SessionKeys`). Validators
/// without a registered BEEFY key are skipped.
pub fn queued_dkg_keys<Client, Block, Backend, AccountId, Keys>(
	client: &Client,
	hash: Block::Hash,
) -> Result<AuthoritySet<AccountId>, AuthoritySetError>
where
	Block: BlockT,
	Backend: sc_client_api::backend::Backend<Block>,
	Client: StorageProvider<Block, Backend>,
	AccountId: Decode + Ord,
	Keys: OpaqueKeys + Decode,
{
	let mut storage_key = twox_128(b"Session").to_vec();
	storage_key.extend_from_slice(&twox_128(b"QueuedKeys"));
	let data = client
		.storage(hash, &StorageKey(storage_key))?
		.ok_or(AuthoritySetError::MissingQueuedKeys)?;

	let queued: Vec<(AccountId, Keys)> =
		Decode::decode(&mut &data.0[..]).map_err(|_| AuthoritySetError::DecodeFailed)?;

	let set: AuthoritySet<AccountId> = queued
		.into_iter()
		.filter_map(|(who, keys)| {
			Public::from_slice(keys.get_raw(BEEFY_KEY_TYPE))
				.ok()
				.map(|public| (who, public))
		})
		.collect();

	if set.is_empty() {
		return Err(AuthoritySetError::NoDkgKeys)
	}
	Ok(set)
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;

	mod test_aura {
		use sp_application_crypto::{app_crypto, sr25519};
		use sp_core::crypto::KeyTypeId;

		pub const TEST_AURA: KeyTypeId = KeyTypeId(*b"aura");

		app_crypto!(sr25519, TEST_AURA);
	}

	mod test_beefy {
		use sp_application_crypto::{app_crypto, ecdsa_bls381};
		use sp_core::crypto::KeyTypeId;

		pub const TEST_BEEFY: KeyTypeId = KeyTypeId(*b"beef");

		app_crypto!(ecdsa_bls381, TEST_BEEFY);
	}

	sp_runtime::impl_opaque_keys! {
		pub struct TestSessionKeys {
			pub aura: test_aura::Public,
			pub beefy: test_beefy::Public,
		}
	}

	#[test]
	fn extracts_beefy_key_from_opaque_session_keys() {
		let beefy_raw = [7u8; 177];
		let keys = TestSessionKeys {
			aura: test_aura::Public::from_slice(&[1u8; 32]).unwrap(),
			beefy: test_beefy::Public::from_slice(&beefy_raw).unwrap(),
		};

		// The full struct must round-trip through SCALE (this is how it is stored
		// inside `Session::QueuedKeys`), and the `beef` key must land at the right
		// offset, next to the other session keys.
		let encoded = keys.encode();
		let decoded = TestSessionKeys::decode(&mut &encoded[..]).unwrap();

		let extracted = Public::from_slice(decoded.get_raw(BEEFY_KEY_TYPE)).unwrap();
		assert_eq!(AsRef::<[u8]>::as_ref(&extracted), &beefy_raw[..]);
		// Sanity: the aura key must not collide with the beefy offset.
		assert_eq!(decoded.get_raw(test_aura::TEST_AURA), &[1u8; 32][..]);
	}
}
