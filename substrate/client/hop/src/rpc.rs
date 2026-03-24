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
use codec::Decode;
use jsonrpsee::{
	core::{async_trait, RpcResult},
	proc_macros::rpc,
	types::ErrorObjectOwned,
};
use sp_blockchain::HeaderBackend;
use sp_core::{hashing::blake2_256, Bytes, H256};
use sp_runtime::{traits::Block as BlockT, MultiSigner, SaturatedConversion};
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

/// No-op verifier that accepts any proof and returns a zero alias (`[0u8; 32]`).
///
/// **WARNING:** When using `NoopVerifier`, all submissions share the same alias,
/// making per-user quota enforcement ineffective — every submitter appears to be
/// the same "user" and they collectively share a single quota bucket.
///
/// Use this only for development/testing or when personhood verification is not
/// available. Production deployments should implement a real [`PersonhoodVerifier`].
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
	/// * `recipients`: List of SCALE-encoded `MultiSigner` (ed25519, sr25519, or ecdsa)
	/// * `proof`: Personhood ring proof bytes (can be empty when using NoopVerifier)
	///
	/// # Returns
	/// The content hash and current pool status
	#[method(name = "hop_submit")]
	fn submit(&self, data: Bytes, recipients: Vec<Bytes>, proof: Bytes) -> RpcResult<SubmitResult>;

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
	C: HeaderBackend<Block> + Send + Sync + 'static,
	V: PersonhoodVerifier,
{
	fn submit(&self, data: Bytes, recipients: Vec<Bytes>, proof: Bytes) -> RpcResult<SubmitResult> {
		// SCALE-decode each recipient as MultiSigner
		let recipient_keys: Vec<MultiSigner> = recipients
			.into_iter()
			.map(|r| {
				MultiSigner::decode(&mut &r.0[..])
					.map_err(|_| ErrorObjectOwned::from(HopError::InvalidRecipientKey))
			})
			.collect::<RpcResult<Vec<_>>>()?;

		// Compute data hash and verify personhood proof
		let hash = H256(blake2_256(&data.0));
		let alias = self.verifier.verify(&proof.0, &HOP_CONTEXT, hash.as_bytes())?;

		// We need the current block number to know when the timeout is reached.
		let current_block = self.client.info().best_number.saturated_into::<u32>();
		let _hash =
			self.pool.insert_prehashed(data.0, current_block, recipient_keys, alias, hash)?;
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
