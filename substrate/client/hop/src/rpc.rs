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

		// Check authorization via runtime API
		let best_hash = self.client.info().best_hash;
		let authorized = self
			.client
			.runtime_api()
			.is_account_authorized(best_hash, account_id.clone())
			.map_err(|e| ErrorObjectOwned::from(HopError::RuntimeApiError(e.to_string())))?;
		if !authorized {
			return Err(ErrorObjectOwned::from(HopError::NotAuthorized));
		}

		// Use account ID as sender identity for rate limiting
		let sender_id: [u8; 32] = account_id.into();
		let current_block = self.client.info().best_number.saturated_into::<u32>();
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
