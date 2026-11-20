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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Encode, Decode, TypeInfo)]
pub enum HashingAlgorithm {
	/// BLAKE2b-256.
	Blake2b256,
	/// SHA2-256.
	Sha2_256,
	/// Keccak-256.
	Keccak256,
}

impl HashingAlgorithm {
	/// IPFS multihash code identifying this algorithm for CID construction.
	pub const fn multihash_code(self) -> u64 {
		match self {
			Self::Blake2b256 => 0xb220,
			Self::Sha2_256 => 0x12,
			Self::Keccak256 => 0x1b,
		}
	}

	/// Compute the 32-byte content hash of `data` using this algorithm.
	pub fn hash(self, data: &[u8]) -> ContentHash {
		match self {
			Self::Blake2b256 => sp_crypto_hashing::blake2_256(data),
			Self::Sha2_256 => sp_crypto_hashing::sha2_256(data),
			Self::Keccak256 => sp_crypto_hashing::keccak_256(data),
		}
	}
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
	/// Must point into the block body; consumers reject entries whose index does not.
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

	/// Get one indexed transaction by its content hash.
	///
	/// Only [`registration::new_data_provider_from_indexed_transactions`] calls this. Errors by
	/// default, so existing implementors keep compiling.
	#[cfg(feature = "std")]
	fn indexed_transaction(&self, _hash: &ContentHash) -> Result<Option<Vec<u8>>, Error> {
		Err(Error::Application(Box::from("indexed_transaction is not supported")))
	}
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

	/// Like [`new_data_provider`], but reads only the transaction holding the selected chunk.
	///
	/// `indexed_transactions` returns the target block's transactions in runtime order, e.g.
	/// from `TransactionStorageApi::indexed_transactions` (v2). The proof is the same as
	/// [`new_data_provider`] gives, without reading the whole indexed body.
	pub fn new_data_provider_from_indexed_transactions<B, C>(
		client: &C,
		parent: &B::Hash,
		retention_period: NumberFor<B>,
		indexed_transactions: impl FnOnce(NumberFor<B>) -> Result<Vec<IndexedTransactionInfo>, Error>,
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

		let infos = indexed_transactions(number)?;
		let sizes: Vec<u32> = infos.iter().map(|info| info.size).collect();
		let proof = build_proof_from_sizes(parent.as_ref(), &sizes, |index| {
			let hash = &infos[index].content_hash;
			client.indexed_transaction(hash)?.ok_or_else(|| {
				Error::Application(Box::from(format!("Missing indexed transaction {hash:?}")))
			})
		})?;
		Ok(InherentDataProvider::new(proof))
	}

	/// Build a proof from the transaction sizes, loading only the selected transaction.
	///
	/// `load` is called once, with the index of the transaction that holds the selected chunk.
	pub fn build_proof_from_sizes(
		random_hash: &[u8],
		sizes: &[u32],
		load: impl FnOnce(usize) -> Result<Vec<u8>, Error>,
	) -> Result<Option<TransactionStorageProof>, Error> {
		let total_chunks: ChunkIndex = sizes.iter().map(|size| num_chunks(*size)).sum();
		if total_chunks.is_zero() {
			return Ok(None);
		}
		let selected_chunk_index = random_chunk(random_hash, total_chunks);

		let mut first_chunk: ChunkIndex = 0;
		for (index, size) in sizes.iter().enumerate() {
			let chunks = num_chunks(*size);
			if selected_chunk_index < first_chunk + chunks {
				let transaction = load(index)?;
				if transaction.len() != *size as usize {
					return Err(Error::Application(Box::from(format!(
						"Indexed transaction {index} has {} bytes, expected {size}",
						transaction.len()
					))));
				}
				return transaction_proof(&transaction, selected_chunk_index - first_chunk)
					.map(Some);
			}
			first_chunk += chunks;
		}

		Err(Error::Application(Box::from(format!("No chunk (total_chunks: {total_chunks}) matched the selected_chunk_index: {selected_chunk_index}; logic error!"))))
	}

	/// Prove one chunk of a single transaction.
	fn transaction_proof(
		transaction: &[u8],
		chunk_index: ChunkIndex,
	) -> Result<TransactionStorageProof, Error> {
		let chunk = transaction
			.chunks(CHUNK_SIZE)
			.nth(chunk_index as usize)
			.ok_or_else(|| {
				Error::Application(Box::from(format!("Chunk {chunk_index} is out of range")))
			})?
			.to_vec();

		let mut db = sp_trie::MemoryDB::<Hasher>::default();
		let mut transaction_root = sp_trie::empty_trie_root::<TrieLayout>();
		{
			let mut trie =
				sp_trie::TrieDBMutBuilder::<TrieLayout>::new(&mut db, &mut transaction_root)
					.build();
			for (index, chunk) in transaction.chunks(CHUNK_SIZE).enumerate() {
				trie.insert(&encode_index(index as u32), chunk)
					.map_err(|e| Error::Application(Box::new(e)))?;
			}
			trie.commit();
		}
		let proof = sp_trie::generate_trie_proof::<TrieLayout, _, _, _>(
			&db,
			transaction_root,
			&[encode_index(chunk_index)],
		)
		.map_err(|e| Error::Application(Box::new(e)))?;
		Ok(TransactionStorageProof { proof, chunk })
	}

	/// Build a proof for a given source of randomness and indexed transactions.
	pub fn build_proof(
		random_hash: &[u8],
		mut transactions: Vec<Vec<u8>>,
	) -> Result<Option<TransactionStorageProof>, Error> {
		let sizes: Vec<u32> = transactions.iter().map(|t| t.len() as u32).collect();
		build_proof_from_sizes(random_hash, &sizes, |index| Ok(transactions.swap_remove(index)))
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

	/// Transactions of mixed sizes: empty, partial last chunk, several chunks.
	#[cfg(test)]
	fn mixed_transactions() -> Vec<Vec<u8>> {
		[3 * CHUNK_SIZE + 17, 0, 1, CHUNK_SIZE, 5 * CHUNK_SIZE - 1, 2 * CHUNK_SIZE]
			.iter()
			.enumerate()
			.map(|(i, len)| (0..*len).map(|j| (i * 31 + j) as u8).collect())
			.collect()
	}

	#[test]
	fn build_proof_from_sizes_proves_selected_chunk() {
		use sp_trie::TrieConfiguration;

		let transactions = mixed_transactions();
		let sizes: Vec<u32> = transactions.iter().map(|t| t.len() as u32).collect();
		// Every chunk of the block as (transaction, chunk index in it, data).
		let chunks: Vec<(usize, ChunkIndex, &[u8])> = transactions
			.iter()
			.enumerate()
			.flat_map(|(tx, data)| {
				data.chunks(CHUNK_SIZE).enumerate().map(move |(i, chunk)| (tx, i as u32, chunk))
			})
			.collect();

		for seed in 0u8..=255 {
			let random = [seed; 32];
			let mut loads = Vec::new();
			let proof = build_proof_from_sizes(&random, &sizes, |index| {
				loads.push(index);
				Ok(transactions[index].clone())
			})
			.unwrap()
			.unwrap();

			let (tx, chunk_index, chunk) =
				chunks[random_chunk(&random, chunks.len() as u32) as usize];
			assert_eq!(loads, vec![tx], "seed={seed}: loads only the selected transaction");
			assert_eq!(proof.chunk, chunk, "seed={seed}");
			// The root the runtime keeps for the transaction.
			let root = TrieLayout::ordered_trie_root(transactions[tx].chunks(CHUNK_SIZE));
			sp_trie::verify_trie_proof::<TrieLayout, _, _, _>(
				&root,
				&proof.proof,
				&[(encode_index(chunk_index), Some(chunk))],
			)
			.unwrap_or_else(|e| panic!("seed={seed}: {e:?}"));

			assert_eq!(build_proof(&random, transactions.clone()).unwrap(), Some(proof));
		}
	}

	#[test]
	fn build_proof_from_sizes_edge_cases() {
		let random = [7u8; 32];
		let no_load = |_| -> Result<Vec<u8>, Error> { panic!("nothing to load") };

		assert!(build_proof_from_sizes(&random, &[], no_load).unwrap().is_none());
		assert!(build_proof_from_sizes(&random, &[0, 0], no_load).unwrap().is_none());

		// The loaded data must match the size the runtime reported.
		let wrong_size = build_proof_from_sizes(&random, &[10], |_| Ok(vec![0u8; 9]));
		assert!(wrong_size.is_err());

		// Load errors are passed through.
		let missing = build_proof_from_sizes(&random, &[10], |_| {
			Err(Error::Application(Box::from("missing")))
		});
		assert!(missing.is_err());
	}

	#[test]
	fn data_provider_from_indexed_transactions_matches_whole_body() {
		use sp_runtime::{generic, traits::BlakeTwo256, OpaqueExtrinsic};
		use std::{cell::RefCell, collections::HashMap};

		type TestBlock = generic::Block<generic::Header<u64, BlakeTwo256>, OpaqueExtrinsic>;

		struct Client {
			parent_number: u64,
			target_number: u64,
			body: Vec<Vec<u8>>,
			by_hash: HashMap<ContentHash, Vec<u8>>,
			loads: RefCell<usize>,
		}

		impl IndexedBody<TestBlock> for Client {
			fn block_indexed_body(&self, number: u64) -> Result<Option<Vec<Vec<u8>>>, Error> {
				assert_eq!(number, self.target_number);
				Ok(Some(self.body.clone()))
			}

			fn number(&self, _hash: sp_core::H256) -> Result<Option<u64>, Error> {
				Ok(Some(self.parent_number))
			}

			fn indexed_transaction(&self, hash: &ContentHash) -> Result<Option<Vec<u8>>, Error> {
				*self.loads.borrow_mut() += 1;
				Ok(self.by_hash.get(hash).cloned())
			}
		}

		let body = mixed_transactions();
		let infos: Vec<IndexedTransactionInfo> = body
			.iter()
			.enumerate()
			.map(|(i, data)| IndexedTransactionInfo {
				content_hash: HashingAlgorithm::Blake2b256.hash(data),
				size: data.len() as u32,
				hashing: HashingAlgorithm::Blake2b256,
				cid_codec: 0x55,
				extrinsic_index: i as u32,
			})
			.collect();
		let client = Client {
			parent_number: 110,
			target_number: 11,
			by_hash: infos.iter().zip(&body).map(|(i, d)| (i.content_hash, d.clone())).collect(),
			body,
			loads: RefCell::new(0),
		};

		for seed in 0u8..32 {
			let parent = sp_core::H256::repeat_byte(seed);
			let whole_body = new_data_provider::<TestBlock, _>(&client, &parent, 100).unwrap();
			*client.loads.borrow_mut() = 0;
			let one_transaction = new_data_provider_from_indexed_transactions::<TestBlock, _>(
				&client,
				&parent,
				100,
				|number| {
					assert_eq!(number, client.target_number);
					Ok(infos.clone())
				},
			)
			.unwrap();

			assert!(whole_body.proof.is_some());
			assert_eq!(one_transaction.proof, whole_body.proof, "seed={seed}");
			assert_eq!(*client.loads.borrow(), 1, "seed={seed}");
		}

		// Too early: no proof and no runtime call.
		let early = Client { parent_number: 50, ..client };
		let provider = new_data_provider_from_indexed_transactions::<TestBlock, _>(
			&early,
			&sp_core::H256::zero(),
			100,
			|_| panic!("too early to ask the runtime"),
		)
		.unwrap();
		assert!(provider.proof.is_none());
	}

	/// Round-trip: build a proof off-chain, verify it against a runtime-side
	/// parallel view computed from the same input order. Catches position-mismatch
	/// bugs where one side reorders the indexed body relative to the other.
	#[test]
	fn proof_round_trip_against_parallel_runtime_view() {
		let payloads: Vec<Vec<u8>> = (0..4)
			.map(|i: u8| {
				let mut p = vec![0u8; 2 * CHUNK_SIZE];
				for (j, byte) in p.iter_mut().enumerate() {
					*byte = i.wrapping_mul(7).wrapping_add(j as u8);
				}
				p
			})
			.collect();

		// Non-monotonic submission order so any sort would visibly disturb it.
		let submission_order = [3usize, 0, 2, 1];

		let from_indexed_body: Vec<Vec<u8>> =
			submission_order.iter().map(|&i| payloads[i].clone()).collect();

		struct TxInfo {
			chunk_root: sp_core::H256,
			size: u32,
			block_chunks: ChunkIndex,
		}
		let mut runtime_view: Vec<TxInfo> = Vec::with_capacity(submission_order.len());
		let mut cumulative: ChunkIndex = 0;
		for &i in submission_order.iter() {
			let payload = &payloads[i];
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

		// Sweep parent_hash so a position bug doesn't pass by chance for some chunks.
		for seed in 0u8..16 {
			let parent_hash = [seed; 32];

			let proof = build_proof(&parent_hash, from_indexed_body.clone()).unwrap().unwrap();

			let total_chunks = runtime_view.last().unwrap().block_chunks;
			let selected_chunk_index = random_chunk(&parent_hash, total_chunks);
			let tx_index = runtime_view
				.binary_search_by_key(&selected_chunk_index, |info| {
					info.block_chunks.saturating_sub(1)
				})
				.unwrap_or_else(|i| i);
			let tx_info = &runtime_view[tx_index];
			let tx_chunks = num_chunks(tx_info.size);
			let prev_chunks = tx_info.block_chunks - tx_chunks;
			let tx_chunk_index = selected_chunk_index - prev_chunks;

			sp_trie::verify_trie_proof::<TrieLayout, _, _, _>(
				&tx_info.chunk_root,
				&proof.proof,
				&[(encode_index(tx_chunk_index), Some(proof.chunk.clone()))],
			)
			.unwrap_or_else(|e| panic!("seed={seed}: {e:?}"));

			let expected_chunk = payloads[submission_order[tx_index]]
				.chunks(CHUNK_SIZE)
				.nth(tx_chunk_index as usize)
				.unwrap()
				.to_vec();
			assert_eq!(proof.chunk, expected_chunk);
		}
	}
}

#[cfg(test)]
mod hashing_algorithm_tests {
	use super::HashingAlgorithm;

	#[test]
	fn multihash_codes_are_iana_assigned() {
		assert_eq!(HashingAlgorithm::Blake2b256.multihash_code(), 0xb220);
		assert_eq!(HashingAlgorithm::Sha2_256.multihash_code(), 0x12);
		assert_eq!(HashingAlgorithm::Keccak256.multihash_code(), 0x1b);
	}

	#[test]
	fn hash_dispatches_to_correct_algorithm() {
		let data = b"polkadot storage chain";
		assert_eq!(HashingAlgorithm::Blake2b256.hash(data), sp_crypto_hashing::blake2_256(data));
		assert_eq!(HashingAlgorithm::Sha2_256.hash(data), sp_crypto_hashing::sha2_256(data));
		assert_eq!(HashingAlgorithm::Keccak256.hash(data), sp_crypto_hashing::keccak_256(data));
	}
}
