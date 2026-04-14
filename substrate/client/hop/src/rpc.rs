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

//! HOP (Hand-Off protocol) RPC interface implementation.
//!
//! All HOP RPC methods are subject to the node's global rate limit configured via
//! `--rpc-rate-limit` (calls per minute per connection). No HOP-specific rate limiting
//! is needed.

use crate::{
	pool::HopDataPool,
	primitives::HopHash,
	types::{HopError, PoolStatus, SubmitResult},
};
use codec::Decode;
use jsonrpsee::{
	core::{async_trait, RpcResult},
	proc_macros::rpc,
	types::ErrorObjectOwned,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::{hashing::blake2_256, Bytes, H256};
use sp_hop::HopApi;
use sp_runtime::{
	traits::{Block as BlockT, IdentifyAccount, Verify},
	AccountId32, MultiSignature, MultiSigner, SaturatedConversion,
};
use std::{marker::PhantomData, sync::Arc};

/// HOP RPC methods.
#[rpc(client, server)]
pub trait HopApi<BlockHash> {
	/// Submit data to the data pool.
	///
	/// # Arguments
	/// * `data`: The data to store, in bytes
	/// * `recipients`: List of SCALE-encoded `MultiSigner` (ed25519, sr25519, or ecdsa)
	/// * `signature`: SCALE-encoded `MultiSignature` over the blake2_256 hash of `data`
	/// * `signer`: SCALE-encoded `MultiSigner` of the account signing the submission
	///
	/// The signer must have an active Bulletin Chain authorization.
	///
	/// # Returns
	/// The current pool status
	#[method(name = "hop_submit")]
	fn submit(
		&self,
		data: Bytes,
		recipients: Vec<Bytes>,
		signature: Bytes,
		signer: Bytes,
	) -> RpcResult<SubmitResult>;

	/// Claim data from the data pool by hash (read-only download).
	///
	/// This does NOT mark the recipient as claimed. After receiving the data,
	/// call `hop_ack` with the same arguments to confirm receipt.
	///
	/// Requires a SCALE-encoded `MultiSignature` over the hash using the ephemeral
	/// private key corresponding to one of the recipient public keys.
	///
	/// # Arguments
	/// * `hash`: The hash of the data, in bytes (32 bytes)
	/// * `signature`: SCALE-encoded `MultiSignature` over the hash
	///
	/// # Returns
	/// The data if the signature matches a recipient that hasn't yet acked
	#[method(name = "hop_claim")]
	fn claim(&self, hash: Bytes, signature: Bytes) -> RpcResult<Bytes>;

	/// Acknowledge receipt of claimed data.
	///
	/// Marks the recipient as claimed and triggers cleanup when all recipients
	/// have acknowledged. Idempotent: acking twice succeeds silently.
	///
	/// # Arguments
	/// * `hash`: The hash of the data, in bytes (32 bytes)
	/// * `signature`: SCALE-encoded `MultiSignature` over the hash
	#[method(name = "hop_ack")]
	fn ack(&self, hash: Bytes, signature: Bytes) -> RpcResult<()>;

	/// Get data pool status
	///
	/// # Returns
	/// Pool statistics including entry count and size
	#[method(name = "hop_poolStatus")]
	fn pool_status(&self) -> RpcResult<PoolStatus>;
}

/// HOP RPC server implementation.
pub struct HopRpcServer<C, Block> {
	pool: Arc<HopDataPool>,
	client: Arc<C>,
	_phantom: PhantomData<Block>,
}

impl<C, Block> HopRpcServer<C, Block> {
	/// Create a new HOP RPC server.
	pub fn new(pool: Arc<HopDataPool>, client: Arc<C>) -> Self {
		Self { pool, client, _phantom: Default::default() }
	}

	/// Convert Bytes to Hash with validation
	fn bytes_to_hash(bytes: Bytes) -> RpcResult<HopHash> {
		let hash_bytes: [u8; 32] = bytes
			.0
			.as_slice()
			.try_into()
			.map_err(|_| ErrorObjectOwned::from(HopError::InvalidHashLength(bytes.0.len())))?;
		Ok(HopHash::from(hash_bytes))
	}
}

#[async_trait]
impl<C, Block> HopApiServer<<Block as BlockT>::Hash> for HopRpcServer<C, Block>
where
	Block: BlockT,
	C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	C::Api: sp_hop::HopApi<Block, AccountId32>,
{
	fn submit(
		&self,
		data: Bytes,
		recipients: Vec<Bytes>,
		signature: Bytes,
		signer: Bytes,
	) -> RpcResult<SubmitResult> {
		// SCALE-decode signer
		let signer = MultiSigner::decode(&mut &signer.0[..])
			.map_err(|_| ErrorObjectOwned::from(HopError::InvalidSigner))?;

		// SCALE-decode signature
		let multi_sig = MultiSignature::decode(&mut &signature.0[..])
			.map_err(|_| ErrorObjectOwned::from(HopError::InvalidSignature))?;

		// SCALE-decode each recipient as MultiSigner
		let recipient_keys: Vec<MultiSigner> = recipients
			.into_iter()
			.map(|r| {
				MultiSigner::decode(&mut &r.0[..])
					.map_err(|_| ErrorObjectOwned::from(HopError::InvalidRecipientKey))
			})
			.collect::<RpcResult<Vec<_>>>()?;

		// Compute data hash and verify signature
		let hash = H256(blake2_256(&data.0));
		let account_id: AccountId32 = signer.into_account();
		if !multi_sig.verify(hash.as_bytes(), &account_id) {
			return Err(ErrorObjectOwned::from(HopError::InvalidSignature));
		}

		// Snapshot chain state once for consistent authorization + block number.
		let chain_info = self.client.info();
		let best_hash = chain_info.best_hash;
		let current_block = chain_info.best_number.saturated_into::<u32>();

		// Check authorization via runtime API
		let authorized = self
			.client
			.runtime_api()
			.is_account_authorized(best_hash, account_id.clone())
			.map_err(|e| ErrorObjectOwned::from(HopError::RuntimeApiError(e.to_string())))?;
		if !authorized {
			return Err(ErrorObjectOwned::from(HopError::NotAuthorized));
		}

		let sender_id: [u8; 32] = account_id.into();
		let _hash = self.pool.insert(data.0, current_block, recipient_keys, sender_id)?;
		let pool_status = self.pool.status();
		Ok(SubmitResult { pool_status })
	}

	fn claim(&self, hash: Bytes, signature: Bytes) -> RpcResult<Bytes> {
		let hash = Self::bytes_to_hash(hash)?;
		let data = self.pool.claim(&hash, &signature.0)?;
		Ok(Bytes(data))
	}

	fn ack(&self, hash: Bytes, signature: Bytes) -> RpcResult<()> {
		let hash = Self::bytes_to_hash(hash)?;
		self.pool.ack(&hash, &signature.0)?;
		Ok(())
	}

	fn pool_status(&self) -> RpcResult<PoolStatus> {
		Ok(self.pool.status())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::pool::HopDataPool;
	use codec::Encode;
	use sp_blockchain::{self, Info};
	use sp_core::{crypto::Pair, ed25519};
	use sp_runtime::{traits::NumberFor, MultiSigner};
	use sp_test_primitives::Block;
	use std::sync::atomic::{AtomicBool, Ordering};
	use tempfile::TempDir;

	struct MockClient {
		authorized: AtomicBool,
	}

	impl MockClient {
		fn new(authorized: bool) -> Self {
			Self { authorized: AtomicBool::new(authorized) }
		}
	}

	impl HeaderBackend<Block> for MockClient {
		fn header(
			&self,
			_hash: <Block as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<<Block as BlockT>::Header>> {
			Ok(None)
		}

		fn info(&self) -> Info<Block> {
			Info {
				best_hash: Default::default(),
				best_number: 0u64,
				genesis_hash: Default::default(),
				finalized_hash: Default::default(),
				finalized_number: 0u64,
				finalized_state: None,
				number_leaves: 0,
				block_gap: None,
			}
		}

		fn status(
			&self,
			_hash: <Block as BlockT>::Hash,
		) -> sp_blockchain::Result<sp_blockchain::BlockStatus> {
			Ok(sp_blockchain::BlockStatus::Unknown)
		}

		fn number(
			&self,
			_hash: <Block as BlockT>::Hash,
		) -> sp_blockchain::Result<Option<NumberFor<Block>>> {
			Ok(None)
		}

		fn hash(
			&self,
			_number: NumberFor<Block>,
		) -> sp_blockchain::Result<Option<<Block as BlockT>::Hash>> {
			Ok(None)
		}
	}

	/// Mock runtime API that delegates to MockClient.
	struct MockRuntimeApi {
		authorized: bool,
	}

	sp_api::mock_impl_runtime_apis! {
		impl sp_hop::HopApi<Block, AccountId32> for MockRuntimeApi {
			fn is_account_authorized(_who: AccountId32) -> bool {
				self.authorized
			}

			fn create_promotion_extrinsic(_data: Vec<u8>) -> <Block as BlockT>::Extrinsic {
				unimplemented!()
			}

			fn max_promotion_size() -> u32 {
				64 * 1024 * 1024
			}
		}
	}

	impl sp_api::ProvideRuntimeApi<Block> for MockClient {
		type Api = MockRuntimeApi;
		fn runtime_api(&self) -> sp_api::ApiRef<MockRuntimeApi> {
			MockRuntimeApi { authorized: self.authorized.load(Ordering::Relaxed) }.into()
		}
	}

	fn setup(authorized: bool) -> (HopRpcServer<MockClient, Block>, Arc<HopDataPool>, TempDir) {
		let dir = TempDir::new().unwrap();
		let pool = Arc::new(HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap());
		let client = Arc::new(MockClient::new(authorized));
		let rpc = HopRpcServer::new(pool.clone(), client);
		(rpc, pool, dir)
	}

	fn make_keypair() -> (ed25519::Pair, MultiSigner) {
		let pair = ed25519::Pair::from_seed(&[1u8; 32]);
		let signer = MultiSigner::Ed25519(pair.public());
		(pair, signer)
	}

	fn sign_data(pair: &ed25519::Pair, data: &[u8]) -> (Bytes, Bytes) {
		let hash = H256(blake2_256(data));
		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		(Bytes(multi_sig.encode()), Bytes(hash.0.to_vec()))
	}

	#[test]
	fn submit_invalid_scale_signer_returns_error() {
		let (rpc, _, _dir) = setup(true);
		let result = rpc.submit(
			Bytes(vec![1, 2, 3]),
			vec![],
			Bytes(vec![0u8; 3]), // invalid signature SCALE
			Bytes(vec![0u8; 3]), // invalid signer SCALE
		);
		// Signer is decoded first, so InvalidSigner error
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err.message().contains("SCALE-decode MultiSigner"), "got: {}", err.message());
	}

	#[test]
	fn submit_invalid_scale_signature_returns_error() {
		let (rpc, _, _dir) = setup(true);
		let (_, signer) = make_keypair();
		let result = rpc.submit(
			Bytes(vec![1, 2, 3]),
			vec![],
			Bytes(vec![0u8; 3]), // invalid signature SCALE
			Bytes(signer.encode()),
		);
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err.message().contains("Invalid signature"), "got: {}", err.message());
	}

	#[test]
	fn submit_bad_signature_returns_error() {
		let (rpc, _, _dir) = setup(true);
		let (_, signer) = make_keypair();
		// Sign with a different key
		let wrong_pair = ed25519::Pair::from_seed(&[99u8; 32]);
		let data = vec![1, 2, 3];
		let hash = H256(blake2_256(&data));
		let sig = wrong_pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);

		let result = rpc.submit(
			Bytes(data),
			vec![Bytes(signer.encode())],
			Bytes(multi_sig.encode()),
			Bytes(signer.encode()),
		);
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err.message().contains("Invalid signature"), "got: {}", err.message());
	}

	#[test]
	fn submit_unauthorized_account_returns_error() {
		let (rpc, _, _dir) = setup(false); // not authorized
		let (pair, signer) = make_keypair();
		let data = vec![1, 2, 3];
		let (sig, _) = sign_data(&pair, &data);

		let result =
			rpc.submit(Bytes(data), vec![Bytes(signer.encode())], sig, Bytes(signer.encode()));
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err.message().contains("authorization"), "got: {}", err.message());
	}

	#[test]
	fn submit_success() {
		let (rpc, pool, _dir) = setup(true);
		let (pair, signer) = make_keypair();
		let data = vec![1, 2, 3, 4, 5];
		let (sig, _) = sign_data(&pair, &data);

		let result =
			rpc.submit(Bytes(data), vec![Bytes(signer.encode())], sig, Bytes(signer.encode()));
		assert!(result.is_ok(), "submit failed: {:?}", result.err());
		let submit_result = result.unwrap();
		assert_eq!(submit_result.pool_status.entry_count, 1);
		assert_eq!(submit_result.pool_status.total_bytes, 5);
		assert_eq!(pool.status().entry_count, 1);
	}

	#[test]
	fn claim_invalid_hash_length() {
		let (rpc, _, _dir) = setup(true);
		let result = rpc.claim(Bytes(vec![0u8; 31]), Bytes(vec![0u8; 64]));
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err.message().contains("expected 32 bytes"), "got: {}", err.message());
	}

	#[test]
	fn claim_and_ack_through_rpc() {
		let (rpc, _, _dir) = setup(true);
		let (pair, signer) = make_keypair();
		let data = vec![10, 20, 30];
		let (sig, _) = sign_data(&pair, &data);

		// Submit
		rpc.submit(Bytes(data.clone()), vec![Bytes(signer.encode())], sig, Bytes(signer.encode()))
			.unwrap();

		// Claim
		let hash = H256(blake2_256(&data));
		let claim_sig = pair.sign(hash.as_bytes());
		let multi_claim_sig = MultiSignature::Ed25519(claim_sig);
		let encoded_claim_sig = Bytes(multi_claim_sig.encode());

		let claimed = rpc.claim(Bytes(hash.0.to_vec()), encoded_claim_sig.clone()).unwrap();
		assert_eq!(claimed.0, data);

		// Ack
		rpc.ack(Bytes(hash.0.to_vec()), encoded_claim_sig).unwrap();

		// Pool should be empty
		let status = rpc.pool_status().unwrap();
		assert_eq!(status.entry_count, 0);
	}

	#[test]
	fn pool_status_returns_correct_values() {
		let (rpc, _, _dir) = setup(true);
		let status = rpc.pool_status().unwrap();
		assert_eq!(status.entry_count, 0);
		assert_eq!(status.total_bytes, 0);
		assert_eq!(status.max_bytes, 1024 * 1024);
	}
}
