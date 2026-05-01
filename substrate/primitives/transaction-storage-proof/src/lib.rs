// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Storage proof primitives. Contains types and basic code to extract storage
//! proofs for indexed transactions.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod runtime_api;

extern crate alloc;

use core::result::Result;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use scale_info::TypeInfo;
use sp_inherents::{InherentData, InherentIdentifier, IsFatalError};
use sp_runtime::traits::{Block as BlockT, NumberFor};

pub use sp_inherents::Error;

/// The identifier for the proof inherent.
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"tx_proof";
/// Proof trie value size.
pub const CHUNK_SIZE: usize = 256;

/// Type used for counting/tracking chunks.
pub type ChunkIndex = u32;

/// Hash of indexed data; the algorithm is reported in [`HashingAlgorithm`].
pub type ContentHash = [u8; 32];

/// IPFS [multicodec](https://github.com/multiformats/multicodec) content-type
/// identifier for an indexed payload. Full list of values [here](https://github.com/multiformats/multicodec/blob/master/table.csv).
pub type CidCodec = u64;

/// Hashing algorithm used to compute a [`ContentHash`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum HashingAlgorithm {
	/// BLAKE2b-256.
	Blake2b256,
	/// SHA2-256.
	Sha2_256,
	/// Keccak-256.
	Keccak256,
}

/// Metadata for a single indexed transaction.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct IndexedTransactionInfo {
	/// Hash of the indexed data.
	pub content_hash: ContentHash,
	/// Size of the indexed data, in bytes.
	pub size: u32,
	/// Algorithm used to compute `content_hash`.
	pub hashing: HashingAlgorithm,
	/// CID codec for constructing the IPFS CID for the indexed data.
	pub cid_codec: CidCodec,
	/// Extrinsic index that produced this entry via `store` or `renew`.
	///
	/// `u32::MAX` when the producing pallet does not record it.
	pub extrinsic_index: u32,
}

/// Errors that can occur while checking the storage proof.
#[derive(Encode, Debug)]
#[cfg_attr(feature = "std", derive(Decode))]
pub enum InherentError {
	InvalidProof,
	TrieError,
}

impl IsFatalError for InherentError {
	fn is_fatal_error(&self) -> bool {
		true
	}
}

/// Holds a chunk of data retrieved from storage along with
/// a proof that the data was stored at that location in the trie.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, PartialEq, Debug, scale_info::TypeInfo)]
pub struct TransactionStorageProof {
	/// Data chunk that is proved to exist.
	pub chunk: Vec<u8>,
	/// Trie nodes that compose the proof.
	pub proof: Vec<Vec<u8>>,
}

/// Auxiliary trait to extract storage proof.
pub trait TransactionStorageProofInherentData {
	/// Get the proof.
	fn storage_proof(&self) -> Result<Option<TransactionStorageProof>, Error>;
}

impl TransactionStorageProofInherentData for InherentData {
	fn storage_proof(&self) -> Result<Option<TransactionStorageProof>, Error> {
		self.get_data(&INHERENT_IDENTIFIER)
	}
}

/// Provider for inherent data.
#[cfg(feature = "std")]
pub struct InherentDataProvider {
	proof: Option<TransactionStorageProof>,
}

#[cfg(feature = "std")]
impl InherentDataProvider {
	pub fn new(proof: Option<TransactionStorageProof>) -> Self {
		InherentDataProvider { proof }
	}
}

#[cfg(feature = "std")]
#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for InherentDataProvider {
	async fn provide_inherent_data(&self, inherent_data: &mut InherentData) -> Result<(), Error> {
		if let Some(proof) = &self.proof {
			inherent_data.put_data(INHERENT_IDENTIFIER, proof)
		} else {
			Ok(())
		}
	}

	async fn try_handle_error(
		&self,
		identifier: &InherentIdentifier,
		mut error: &[u8],
	) -> Option<Result<(), Error>> {
		if *identifier != INHERENT_IDENTIFIER {
			return None;
		}

		let error = InherentError::decode(&mut error).ok()?;

		Some(Err(Error::Application(Box::from(format!("{:?}", error)))))
	}
}

/// A utility function to extract a chunk index from the source of randomness.
///
/// # Panics
///
/// This function panics if `total_chunks` is `0`.
pub fn random_chunk(random_hash: &[u8], total_chunks: ChunkIndex) -> ChunkIndex {
	let mut buf = [0u8; 8];
	buf.copy_from_slice(&random_hash[0..8]);
	let random_u64 = u64::from_be_bytes(buf);
	(random_u64 % total_chunks as u64) as u32
}

/// A utility function to calculate the number of chunks.
///
/// * `bytes` - number of bytes
pub fn num_chunks(bytes: u32) -> ChunkIndex {
	(bytes as u64).div_ceil(CHUNK_SIZE as u64) as u32
}

/// A utility function to encode the transaction index as a trie key.
///
/// * `index` - chunk index.
pub fn encode_index(index: ChunkIndex) -> Vec<u8> {
	codec::Encode::encode(&codec::Compact(index))
}

/// An interface to request indexed data from the client.
pub trait IndexedBody<B: BlockT> {
	/// Get all indexed transactions for a block,
	/// including renewed transactions.
	///
	/// Note that this will only fetch transactions
	/// that are indexed by the runtime with `storage_index_transaction`.
	fn block_indexed_body(&self, number: NumberFor<B>) -> Result<Option<Vec<Vec<u8>>>, Error>;

	/// Get a block number for a block hash.
	fn number(&self, hash: B::Hash) -> Result<Option<NumberFor<B>>, Error>;
}

#[cfg(feature = "std")]
pub mod registration {
	use super::*;
	use sp_runtime::traits::{Block as BlockT, One, Saturating, Zero};
	use sp_trie::TrieMut;

	type Hasher = sp_core::Blake2Hasher;
	type TrieLayout = sp_trie::LayoutV1<Hasher>;

	/// Create a new inherent data provider instance for a given parent block hash.
	pub fn new_data_provider<B, C>(
		client: &C,
		parent: &B::Hash,
		retention_period: NumberFor<B>,
	) -> Result<InherentDataProvider, Error>
	where
		B: BlockT,
		C: IndexedBody<B>,
	{
		let parent_number = client.number(*parent)?.unwrap_or(Zero::zero());
		let number = parent_number.saturating_add(One::one()).saturating_sub(retention_period);
		if number.is_zero() {
			// Too early to collect proofs.
			return Ok(InherentDataProvider::new(None));
		}

		let proof = match client.block_indexed_body(number)? {
			Some(transactions) => build_proof(parent.as_ref(), transactions)?,
			None => {
				// Nothing was indexed in that block.
				None
			},
		};
		Ok(InherentDataProvider::new(proof))
	}

	/// Build a proof for a given source of randomness and indexed transactions.
	pub fn build_proof(
		random_hash: &[u8],
		transactions: Vec<Vec<u8>>,
	) -> Result<Option<TransactionStorageProof>, Error> {
		// Get total chunks, we will need it to generate a random chunk index.
		let total_chunks: ChunkIndex =
			transactions.iter().map(|t| num_chunks(t.len() as u32)).sum();
		if total_chunks.is_zero() {
			return Ok(None);
		}
		let selected_chunk_index = random_chunk(random_hash, total_chunks);

		// Generate tries for each transaction.
		let mut chunk_index = 0;
		for transaction in transactions {
			let mut selected_chunk_and_key = None;
			let mut db = sp_trie::MemoryDB::<Hasher>::default();
			let mut transaction_root = sp_trie::empty_trie_root::<TrieLayout>();
			{
				let mut trie =
					sp_trie::TrieDBMutBuilder::<TrieLayout>::new(&mut db, &mut transaction_root)
						.build();
				let chunks = transaction.chunks(CHUNK_SIZE).map(|c| c.to_vec());
				for (index, chunk) in chunks.enumerate() {
					let index = encode_index(index as u32);
					trie.insert(&index, &chunk).map_err(|e| Error::Application(Box::new(e)))?;
					if chunk_index == selected_chunk_index {
						selected_chunk_and_key = Some((chunk, index));
					}
					chunk_index += 1;
				}
				trie.commit();
			}
			if let Some((target_chunk, target_chunk_key)) = selected_chunk_and_key {
				let chunk_proof = sp_trie::generate_trie_proof::<TrieLayout, _, _, _>(
					&db,
					transaction_root,
					&[target_chunk_key],
				)
				.map_err(|e| Error::Application(Box::new(e)))?;

				// We found the chunk and computed the proof root for the entire transaction,
				// so there is no need to waste time calculating the subsequent transactions.
				return Ok(Some(TransactionStorageProof {
					proof: chunk_proof,
					chunk: target_chunk,
				}));
			}
		}

		Err(Error::Application(Box::from(format!("No chunk (total_chunks: {total_chunks}) matched the selected_chunk_index: {selected_chunk_index}; logic error!"))))
	}

	#[test]
	fn build_proof_check() {
		use std::str::FromStr;
		let random = [0u8; 32];
		let proof = build_proof(&random, vec![vec![42]]).unwrap().unwrap();
		let root = sp_core::H256::from_str(
			"0xff8611a4d212fc161dae19dd57f0f1ba9309f45d6207da13f2d3eab4c6839e91",
		)
		.unwrap();
		sp_trie::verify_trie_proof::<TrieLayout, _, _, _>(
			&root,
			&proof.proof,
			&[(encode_index(0), Some(proof.chunk))],
		)
		.unwrap();

		// Fail for empty transactions/chunks.
		assert!(build_proof(&random, vec![]).unwrap().is_none());
		assert!(build_proof(&random, vec![vec![]]).unwrap().is_none());
	}

	/// End-to-end proof round-trip across the substrate-host ↔ runtime boundary, with
	/// multiple distinct transactions in the indexed body.
	///
	/// Models the verification contract that any FRAME pallet consuming
	/// transaction-storage proofs must hold:
	///
	/// 1. The off-chain proof provider walks `block_indexed_body(N)` linearly, counting chunks, and
	///    builds a Merkle proof for the chunk at `random_chunk(parent_hash, total_chunks)`.
	/// 2. The runtime (e.g. `pallet_transaction_storage::verify_chunk_proof`) walks a parallel
	///    `Vec<TransactionInfo>` in the SAME order, picks the entry containing the same global
	///    chunk index via binary search on cumulative `block_chunks`, and verifies the proof
	///    against that entry's `chunk_root`.
	///
	/// If the two sides disagree on what's at position N (e.g. because one side sorts
	/// the indexed body and the other doesn't), the runtime verifies the proof
	/// against a different entry's `chunk_root` than the proof was built for, and
	/// `verify_trie_proof` returns an `Err`. This test exercises both halves
	/// against the same input order and asserts they agree.
	#[test]
	fn proof_round_trip_against_parallel_runtime_view() {
		// 4 distinct payloads, each spanning multiple chunks. Distinct payloads
		// produce distinct `chunk_root`s — that's the property that catches
		// position-mismatch bugs (sorting one side of the iteration would route the
		// proof to a different entry, whose `chunk_root` is different).
		let payloads: Vec<Vec<u8>> = (0..4)
			.map(|i: u8| {
				// Each payload is 2 * CHUNK_SIZE bytes filled with a unique pattern,
				// so each yields exactly 2 chunks with payload-specific content.
				let mut p = vec![0u8; 2 * CHUNK_SIZE];
				for (j, byte) in p.iter_mut().enumerate() {
					*byte = i.wrapping_mul(7).wrapping_add(j as u8);
				}
				p
			})
			.collect();

		// Pick an order that's neither sorted ascending nor descending by content,
		// so any directional sort would visibly disturb it. Submission order:
		// [3, 0, 2, 1].
		let submission_order = [3usize, 0, 2, 1];

		// What `block_indexed_body(N)` would return for a block whose substrate-host
		// `MultiRenew::hashes` was populated in this submission order. (The
		// substrate-host fetches col11 by hash — the order of the returned data
		// blobs mirrors the order of hashes in `MultiRenew::hashes`.)
		let from_indexed_body: Vec<Vec<u8>> =
			submission_order.iter().map(|&i| payloads[i].clone()).collect();

		// What a runtime pallet would parallel-store as `Vec<TransactionInfo>`.
		// We compute each entry's `chunk_root` from its actual chunks, with cumulative
		// `block_chunks` matching the submission order.
		struct TxInfo {
			chunk_root: sp_core::H256,
			size: u32,
			block_chunks: ChunkIndex,
		}
		let mut runtime_view: Vec<TxInfo> = Vec::with_capacity(submission_order.len());
		let mut cumulative: ChunkIndex = 0;
		for &i in submission_order.iter() {
			let payload = &payloads[i];
			// Build the same per-tx trie that `build_proof` builds, to capture the
			// chunk_root the runtime will trust.
			let mut db = sp_trie::MemoryDB::<Hasher>::default();
			let mut transaction_root = sp_trie::empty_trie_root::<TrieLayout>();
			{
				let mut trie =
					sp_trie::TrieDBMutBuilder::<TrieLayout>::new(&mut db, &mut transaction_root)
						.build();
				for (idx, chunk) in payload.chunks(CHUNK_SIZE).enumerate() {
					trie.insert(&encode_index(idx as u32), chunk).unwrap();
				}
				trie.commit();
			}
			cumulative += num_chunks(payload.len() as u32);
			runtime_view.push(TxInfo {
				chunk_root: transaction_root,
				size: payload.len() as u32,
				block_chunks: cumulative,
			});
		}

		// We sweep `parent_hash` through several values to make sure the round-trip
		// holds for many different `random_chunk` outputs (each parent_hash picks a
		// different global chunk index). Without this loop, a position bug that only
		// manifests for some chunks could pass by chance.
		for seed in 0u8..16 {
			let parent_hash = [seed; 32];

			// Off-chain side: build the proof exactly the way the inherent provider does.
			let proof = build_proof(&parent_hash, from_indexed_body.clone())
				.unwrap()
				.expect("proof must be built (non-empty input)");

			// On-chain side: pick the entry the runtime would pick.
			let total_chunks = runtime_view.last().unwrap().block_chunks;
			assert_eq!(
				total_chunks,
				submission_order
					.iter()
					.map(|&i| num_chunks(payloads[i].len() as u32))
					.sum::<u32>(),
				"runtime_view's cumulative block_chunks must equal sum of per-tx chunks",
			);
			let selected_chunk_index = random_chunk(&parent_hash, total_chunks);
			// Same binary-search logic that pallet_transaction_storage::verify_chunk_proof uses.
			let tx_index = runtime_view
				.binary_search_by_key(&selected_chunk_index, |info| {
					info.block_chunks.saturating_sub(1)
				})
				.unwrap_or_else(|i| i);
			let tx_info = &runtime_view[tx_index];
			let tx_chunks = num_chunks(tx_info.size);
			let prev_chunks = tx_info.block_chunks - tx_chunks;
			let tx_chunk_index = selected_chunk_index - prev_chunks;

			// Verify the proof against the runtime-side chunk_root, just like the
			// runtime would. If the two sides disagree on what's at position
			// `tx_index`, the proof was built using a DIFFERENT entry's chunks (and
			// thus is rooted at a different chunk_root), and verification fails.
			sp_trie::verify_trie_proof::<TrieLayout, _, _, _>(
				&tx_info.chunk_root,
				&proof.proof,
				&[(encode_index(tx_chunk_index), Some(proof.chunk.clone()))],
			)
			.unwrap_or_else(|e| {
				panic!(
					"proof verification failed for parent_hash seed={seed}: \n\
					 selected_chunk_index = {selected_chunk_index} \n\
					 runtime tx_index     = {tx_index} (within submission order [3, 0, 2, 1]) \n\
					 tx_chunk_index       = {tx_chunk_index} \n\
					 If this fails, build_proof and the runtime's parallel view disagree \
					 on what's at position {tx_index} — this is the cross-side ordering bug \
					 that DbExtrinsic::MultiRenew::hashes guards against. \n\
					 Underlying trie error: {e:?}",
				)
			});

			// Sanity: `proof.chunk` must also equal the actual chunk in the payload at
			// the resolved (tx_index, tx_chunk_index). Catches the case where
			// verification arithmetic happens to succeed for the wrong reason.
			let expected_chunk = {
				let payload_idx = submission_order[tx_index];
				let payload = &payloads[payload_idx];
				payload
					.chunks(CHUNK_SIZE)
					.nth(tx_chunk_index as usize)
					.expect("tx_chunk_index in range")
					.to_vec()
			};
			assert_eq!(
				proof.chunk, expected_chunk,
				"proof.chunk must equal the actual chunk at submission_order[{tx_index}] \
				 chunk #{tx_chunk_index}",
			);
		}
	}
}

#[cfg(all(test, feature = "std"))]
mod position_invariant_tests {
	//! Standalone tests pinning the order-preservation invariant that the
	//! cross-process proof contract depends on, independent of the trie/proof
	//! machinery. These complement [`registration::proof_round_trip_against_parallel_runtime_view`]
	//! by failing fast and with a clearer error if the more fundamental ordering
	//! contract is violated.
	use super::*;

	#[test]
	fn random_chunk_is_deterministic_for_same_inputs() {
		// `random_chunk(random_hash, total_chunks)` is the agreement point between
		// substrate-host's proof builder and the runtime's verifier — both must
		// compute the same chunk index for the same inputs. This pins it.
		for seed in 0u8..16 {
			let h = [seed; 32];
			for n in [1u32, 4, 16, 4096, u32::MAX / 2, u32::MAX - 1] {
				let a = random_chunk(&h, n);
				let b = random_chunk(&h, n);
				assert_eq!(a, b, "random_chunk must be deterministic for seed={seed}, n={n}");
				assert!(a < n, "random_chunk output {a} must be in [0, {n})");
			}
		}
	}

	#[test]
	fn encode_index_round_trip_is_compact() {
		// `encode_index` produces the same bytes both sides use to insert/look up
		// the chunk into/from the per-transaction trie. Drift here (e.g. switching
		// from Compact to fixed-width or big-endian encoding) silently breaks every
		// proof verification.
		use codec::{Compact, Decode, Encode};
		for index in [0u32, 1, 63, 64, 16383, 16384, 1_000_000, u32::MAX] {
			let encoded = encode_index(index);
			assert_eq!(encoded, Compact(index).encode(), "encode_index must use Compact<u32>");
			let decoded =
				Compact::<u32>::decode(&mut &encoded[..]).expect("compact-encoded index decodes");
			assert_eq!(decoded.0, index);
		}
	}
}
