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

use crate::{
	pool::HopDataPool,
	primitives::HopHash,
	types::{Alias, HopError, PoolStatus, SubmitResult, HOP_CONTEXT},
};
use jsonrpsee::{
	core::{async_trait, RpcResult},
	proc_macros::rpc,
	types::ErrorObjectOwned,
};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_core::{hashing::blake2_256, Bytes, H256};
use sp_runtime::{traits::Block as BlockT, SaturatedConversion};
use std::{marker::PhantomData, sync::Arc};

/// Trait for verifying personhood ring proofs.
/// Implemented by the node using the individuality system's ring-VRF verification.
pub trait PersonhoodVerifier: Send + Sync + 'static {
	/// Verify a proof and return the prover's alias.
	/// - `proof`: raw proof bytes (SCALE-encoded ring-VRF proof)
	/// - `context`: application context (HOP_CONTEXT)
	/// - `msg`: message the proof is bound to (data hash)
	fn verify(&self, proof: &[u8], context: &[u8], msg: &[u8]) -> Result<Alias, HopError>;
}

/// No-op verifier that accepts any proof and returns a zero alias.
/// Use this when personhood verification is not needed.
pub struct NoopVerifier;

impl PersonhoodVerifier for NoopVerifier {
	fn verify(&self, _proof: &[u8], _context: &[u8], _msg: &[u8]) -> Result<Alias, HopError> {
		Ok([0u8; 32])
	}
}

/// HOP RPC methods.
#[rpc(client, server)]
pub trait HopApi<BlockHash> {
	/// Submit data to the data pool
	///
	/// # Arguments
	/// * `data`: The data to store, in bytes
	/// * `recipients`: List of ephemeral ed25519 public keys (32 bytes each)
	/// * `proof`: Personhood ring proof bytes (can be empty when using NoopVerifier)
	///
	/// # Returns
	/// The content hash and current pool status
	#[method(name = "hop_submit")]
	fn submit(&self, data: Bytes, recipients: Vec<Bytes>, proof: Bytes) -> RpcResult<SubmitResult>;

	/// Claim data from the data pool by hash
	///
	/// Requires a signature over the hash using the ephemeral private key
	/// corresponding to one of the recipient public keys.
	///
	/// # Arguments
	/// * `hash`: The hash of the data, in bytes (32 bytes)
	/// * `signature`: Ed25519 signature over the hash (64 bytes)
	///
	/// # Returns
	/// The data if the signature matches an unclaimed recipient
	#[method(name = "hop_claim")]
	fn claim(&self, hash: Bytes, signature: Bytes) -> RpcResult<Bytes>;

	/// Get data pool status
	///
	/// # Returns
	/// Pool statistics including entry count and size
	#[method(name = "hop_poolStatus")]
	fn pool_status(&self) -> RpcResult<PoolStatus>;
}

/// HOP RPC server implementation.
pub struct HopRpcServer<C, Block, V: PersonhoodVerifier> {
	pool: Arc<HopDataPool>,
	client: Arc<C>,
	verifier: Arc<V>,
	_phantom: PhantomData<Block>,
}

impl<C, Block, V: PersonhoodVerifier> HopRpcServer<C, Block, V> {
	/// Create a new HOP RPC server.
	pub fn new(pool: Arc<HopDataPool>, client: Arc<C>, verifier: Arc<V>) -> Self {
		Self { pool, client, verifier, _phantom: Default::default() }
	}

	/// Convert Bytes to Hash with validation
	fn bytes_to_hash(bytes: Bytes) -> RpcResult<HopHash> {
		let hash_bytes: [u8; 32] = bytes.0.as_slice().try_into().map_err(|_| {
			ErrorObjectOwned::owned(
				1008,
				format!("Invalid hash length: expected 32 bytes, got {}", bytes.0.len()),
				None::<()>,
			)
		})?;
		Ok(HopHash::from(hash_bytes))
	}
}

#[async_trait]
impl<C, Block, V> HopApiServer<<Block as BlockT>::Hash> for HopRpcServer<C, Block, V>
where
	Block: BlockT,
	C: HeaderBackend<Block> + ProvideRuntimeApi<Block> + Send + Sync + 'static,
	V: PersonhoodVerifier,
{
	fn submit(&self, data: Bytes, recipients: Vec<Bytes>, proof: Bytes) -> RpcResult<SubmitResult> {
		// Parse and validate recipient keys
		let recipient_keys: Vec<[u8; 32]> = recipients
			.into_iter()
			.map(|r| {
				let bytes: [u8; 32] = r.0.as_slice().try_into().map_err(|_| {
					ErrorObjectOwned::from(HopError::InvalidRecipientKey(r.0.len()))
				})?;
				Ok(bytes)
			})
			.collect::<RpcResult<Vec<_>>>()?;

		// Compute data hash and verify personhood proof
		let hash = H256(blake2_256(&data.0));
		let alias = self.verifier.verify(&proof.0, &HOP_CONTEXT, hash.as_bytes())?;

		// We need the current block number to know when the timeout is reached.
		let current_block = self.client.info().best_number.saturated_into::<u32>();
		let hash = self.pool.insert(data.0, current_block, recipient_keys, alias)?;
		let pool_status = self.pool.status();
		Ok(SubmitResult { hash: Bytes(hash.0.to_vec()), pool_status })
	}

	fn claim(&self, hash: Bytes, signature: Bytes) -> RpcResult<Bytes> {
		let hash = Self::bytes_to_hash(hash)?;
		let data = self.pool.claim(&hash, &signature.0)?;
		Ok(Bytes(data))
	}

	fn pool_status(&self) -> RpcResult<PoolStatus> {
		Ok(self.pool.status())
	}
}
