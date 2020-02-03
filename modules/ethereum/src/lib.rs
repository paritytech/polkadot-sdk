// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Parity-Bridge.

// Parity-Bridge is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity-Bridge is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity-Bridge.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{decl_module, decl_storage};
use primitives::{Address, Header, Receipt, H256, U256};
use sp_runtime::RuntimeDebug;
use sp_std::{iter::from_fn, prelude::*};
use validators::{ValidatorsConfiguration, ValidatorsSource};

pub use import::{header_import_requires_receipts, import_header};

mod error;
mod finality;
mod import;
mod validators;
mod verification;

/// Authority round engine configuration parameters.
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
pub struct AuraConfiguration {
	/// Empty step messages transition block.
	pub empty_steps_transition: u64,
	/// Transition block to strict empty steps validation.
	pub strict_empty_steps_transition: u64,
	/// Monotonic step validation transition block.
	pub validate_step_transition: u64,
	/// Chain score validation transition block.
	pub validate_score_transition: u64,
	/// First block for which a 2/3 quorum (instead of 1/2) is required.
	pub two_thirds_majority_transition: u64,
	/// Minimum gas limit.
	pub min_gas_limit: U256,
	/// Maximum gas limit.
	pub max_gas_limit: U256,
	/// Maximum size of extra data.
	pub maximum_extra_data_size: u64,
}

/// Block header as it is stored in the runtime storage.
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
pub struct StoredHeader {
	/// The block header itself.
	pub header: Header,
	/// Total difficulty of the chain.
	pub total_difficulty: U256,
	/// The ID of set of validators that is expected to produce direct descendants of
	/// this block. If header enacts new set, this would be the new set. Otherwise
	/// this is the set that has produced the block itself.
	/// The hash is the hash of block where validators set has been enacted.
	pub next_validators_set_id: u64,
}

/// Header that we're importing.
#[derive(RuntimeDebug)]
#[cfg_attr(test, derive(Clone, PartialEq))]
pub struct HeaderToImport {
	/// Header import context,
	pub context: ImportContext,
	/// Should we consider this header as best?
	pub is_best: bool,
	/// The hash of the header.
	pub hash: H256,
	/// The header itself.
	pub header: Header,
	/// Total chain difficulty at the header.
	pub total_difficulty: U256,
	/// Validators set enacted change, if happened at the header.
	pub enacted_change: Option<Vec<Address>>,
	/// Validators set scheduled change, if happened at the header.
	pub scheduled_change: Option<Vec<Address>>,
}

/// Header import context.
#[derive(RuntimeDebug)]
#[cfg_attr(test, derive(Clone, PartialEq))]
pub struct ImportContext {
	parent_header: Header,
	parent_total_difficulty: U256,
	next_validators_set_id: u64,
	next_validators_set: (H256, Vec<Address>),
}

impl ImportContext {
	/// Create import context using passing parameters;
	pub fn new(
		parent_header: Header,
		parent_total_difficulty: U256,
		next_validators_set_id: u64,
		next_validators_set: (H256, Vec<Address>),
	) -> Self {
		ImportContext {
			parent_header,
			parent_total_difficulty,
			next_validators_set_id,
			next_validators_set,
		}
	}

	/// Returns reference to parent header.
	pub fn parent_header(&self) -> &Header {
		&self.parent_header
	}

	/// Returns total chain difficulty at parent block.
	pub fn total_difficulty(&self) -> &U256 {
		&self.parent_total_difficulty
	}

	/// Returns id of the set of validators.
	pub fn validators_set_id(&self) -> u64 {
		self.next_validators_set_id
	}

	/// Returns block whenre validators set has been enacted.
	pub fn validators_start(&self) -> &H256 {
		&self.next_validators_set.0
	}

	/// Returns reference to the set of validators of the block we're going to import.
	pub fn validators(&self) -> &[Address] {
		&self.next_validators_set.1
	}

	/// Converts import context into header we're going to import.
	pub fn into_import_header(
		self,
		is_best: bool,
		hash: H256,
		header: Header,
		total_difficulty: U256,
		enacted_change: Option<Vec<Address>>,
		scheduled_change: Option<Vec<Address>>,
	) -> HeaderToImport {
		HeaderToImport {
			context: self,
			is_best,
			hash,
			header,
			total_difficulty,
			enacted_change,
			scheduled_change,
		}
	}
}

/// The storage that is used by the client.
///
/// Storage modification must be discarded if block import has failed.
pub trait Storage {
	/// Get best known block.
	fn best_block(&self) -> (u64, H256, U256);
	/// Get last finalized block.
	fn finalized_block(&self) -> (u64, H256);
	/// Get imported header by its hash.
	fn header(&self, hash: &H256) -> Option<Header>;
	/// Get header import context by parent header hash.
	fn import_context(&self, parent_hash: &H256) -> Option<ImportContext>;
	/// Get new validators that are scheduled by given header.
	fn scheduled_change(&self, hash: &H256) -> Option<Vec<Address>>;
	/// Insert imported header.
	fn insert_header(&mut self, header: HeaderToImport);
	/// Finalize given block and prune all headers with number < prune_end.
	/// The headers in the pruning range could be either finalized, or not.
	/// It is the storage duty to ensure that unfinalized headers that have
	/// scheduled changes won't be pruned until they or their competitors
	/// are finalized.
	fn finalize_headers(&mut self, finalized: Option<(u64, H256)>, prune_end: Option<u64>);
}

/// Decides whether the session should be ended.
pub trait OnHeadersSubmitted<AccountId> {
	/// Called when valid headers have been submitted.
	fn on_valid_headers_submitted(submitter: AccountId, useful: u64, useless: u64);
	/// Called when invalid headers have been submitted.
	fn on_invalid_headers_submitted(submitter: AccountId);
}

impl<AccountId> OnHeadersSubmitted<AccountId> for () {
	fn on_valid_headers_submitted(_submitter: AccountId, _useful: u64, _useless: u64) {}
	fn on_invalid_headers_submitted(_submitter: AccountId) {}
}

/// The module configuration trait
pub trait Trait: frame_system::Trait {
	/// Handler for headers submission result.
	type OnHeadersSubmitted: OnHeadersSubmitted<Self::AccountId>;
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		/// Import Aura chain headers. Ignores non-fatal errors (like when known
		/// header is provided), rewards for successful headers import and penalizes
		/// for fatal errors.
		///
		/// This should be used with caution - passing too many headers could lead to
		/// enormous block production/import time.
		pub fn import_headers(origin, headers_with_receipts: Vec<(Header, Option<Vec<Receipt>>)>) {
			let submitter = frame_system::ensure_signed(origin)?;
			let import_result = import::import_headers(
				&mut BridgeStorage,
				&kovan_aura_config(),
				&kovan_validators_config(),
				crate::import::PRUNE_DEPTH,
				headers_with_receipts,
			);

			match import_result {
				Ok((useful, useless)) =>
					T::OnHeadersSubmitted::on_valid_headers_submitted(submitter, useful, useless),
				Err(error) => {
					// even though we may have accept some headers, we do not want to reward someone
					// who provides invalid headers
					T::OnHeadersSubmitted::on_invalid_headers_submitted(submitter);
					return Err(error.msg().into());
				},
			}
		}
	}
}

decl_storage! {
	trait Store for Module<T: Trait> as Bridge {
		/// Best known block.
		BestBlock: (u64, H256, U256);
		/// Best finalized block.
		FinalizedBlock: (u64, H256);
		/// Oldest unpruned block(s) number.
		OldestUnprunedBlock: u64;
		/// Map of imported headers by hash.
		Headers: map hasher(blake2_256) H256 => Option<StoredHeader>;
		/// Map of imported header hashes by number.
		HeadersByNumber: map hasher(blake2_256) u64 => Option<Vec<H256>>;
		/// The ID of next validator set.
		NextValidatorsSetId: u64;
		/// Map of validators sets by their id.
		ValidatorsSets: map hasher(blake2_256) u64 => Option<(H256, Vec<Address>)>;
		/// Validators sets reference count. Each header that is authored by this set increases
		/// the reference count. When we prune this header, we decrease the reference count.
		/// When it reaches zero, we are free to prune validator set as well.
		ValidatorsSetsRc: map hasher(blake2_256) u64 => Option<u64>;
		/// Map of validators set changes scheduled by given header.
		ScheduledChanges: map hasher(blake2_256) H256 => Option<Vec<Address>>;
	}
	add_extra_genesis {
		config(initial_header): Header;
		config(initial_difficulty): U256;
		config(initial_validators): Vec<Address>;
		build(|config| {
			// the initial blocks should be selected so that:
			// 1) it doesn't signal validators changes;
			// 2) there are no scheduled validators changes from previous blocks;
			// 3) (implied) all direct children of initial block are authred by the same validators set.

			assert!(
				!config.initial_validators.is_empty(),
				"Initial validators set can't be empty",
			);

			let initial_hash = config.initial_header.hash();
			BestBlock::put((config.initial_header.number, initial_hash, config.initial_difficulty));
			FinalizedBlock::put((config.initial_header.number, initial_hash));
			OldestUnprunedBlock::put(config.initial_header.number);
			HeadersByNumber::insert(config.initial_header.number, vec![initial_hash]);
			Headers::insert(initial_hash, StoredHeader {
				header: config.initial_header.clone(),
				total_difficulty: config.initial_difficulty,
				next_validators_set_id: 0,
			});
			NextValidatorsSetId::put(1);
			ValidatorsSets::insert(0, (initial_hash, config.initial_validators.clone()));
			ValidatorsSetsRc::insert(0, 1);
		})
	}
}

impl<T: Trait> Module<T> {
	/// Returns number and hash of the best block known to the bridge module.
	/// The caller should only submit `import_header` transaction that makes
	/// (or leads to making) other header the best one.
	pub fn best_block() -> (u64, H256) {
		let (number, hash, _) = BridgeStorage.best_block();
		(number, hash)
	}

	/// Returns true if the import of given block requires transactions receipts.
	pub fn is_import_requires_receipts(header: Header) -> bool {
		import::header_import_requires_receipts(&BridgeStorage, &kovan_validators_config(), &header)
	}

	/// Returns true if header is known to the runtime.
	pub fn is_known_block(hash: H256) -> bool {
		BridgeStorage.header(&hash).is_some()
	}
}

/// Runtime bridge storage.
struct BridgeStorage;

impl Storage for BridgeStorage {
	fn best_block(&self) -> (u64, H256, U256) {
		BestBlock::get()
	}

	fn finalized_block(&self) -> (u64, H256) {
		FinalizedBlock::get()
	}

	fn header(&self, hash: &H256) -> Option<Header> {
		Headers::get(hash).map(|header| header.header)
	}

	fn import_context(&self, parent_hash: &H256) -> Option<ImportContext> {
		Headers::get(parent_hash).map(|parent_header| {
			let (next_validators_set_start, next_validators) =
				ValidatorsSets::get(parent_header.next_validators_set_id)
					.expect("validators set is only pruned when last ref is pruned; there is a ref; qed");
			ImportContext {
				parent_header: parent_header.header,
				parent_total_difficulty: parent_header.total_difficulty,
				next_validators_set_id: parent_header.next_validators_set_id,
				next_validators_set: (next_validators_set_start, next_validators),
			}
		})
	}

	fn scheduled_change(&self, hash: &H256) -> Option<Vec<Address>> {
		ScheduledChanges::get(hash)
	}

	fn insert_header(&mut self, header: HeaderToImport) {
		if header.is_best {
			BestBlock::put((header.header.number, header.hash, header.total_difficulty));
		}
		if let Some(scheduled_change) = header.scheduled_change {
			ScheduledChanges::insert(&header.hash, scheduled_change);
		}
		let next_validators_set_id = match header.enacted_change {
			Some(enacted_change) => {
				let next_validators_set_id = NextValidatorsSetId::mutate(|set_id| {
					let next_set_id = *set_id;
					*set_id += 1;
					next_set_id
				});
				ValidatorsSets::insert(next_validators_set_id, (header.hash, enacted_change));
				ValidatorsSetsRc::insert(next_validators_set_id, 1);
				next_validators_set_id
			}
			None => {
				ValidatorsSetsRc::mutate(header.context.next_validators_set_id, |rc| {
					*rc = Some(rc.map(|rc| rc + 1).unwrap_or(1));
					*rc
				});
				header.context.next_validators_set_id
			}
		};

		HeadersByNumber::append_or_insert(header.header.number, vec![header.hash]);
		Headers::insert(
			&header.hash,
			StoredHeader {
				header: header.header,
				total_difficulty: header.total_difficulty,
				next_validators_set_id,
			},
		);
	}

	fn finalize_headers(&mut self, finalized: Option<(u64, H256)>, prune_end: Option<u64>) {
		// remember just finalized block
		let finalized_number = finalized
			.as_ref()
			.map(|f| f.0)
			.unwrap_or_else(|| FinalizedBlock::get().0);
		if let Some(finalized) = finalized {
			FinalizedBlock::put(finalized);
		}

		if let Some(prune_end) = prune_end {
			let prune_begin = OldestUnprunedBlock::get();

			for number in prune_begin..prune_end {
				let blocks_at_number = HeadersByNumber::take(number);

				// ensure that unfinalized headers we want to prune do not have scheduled changes
				if number > finalized_number {
					if let Some(ref blocks_at_number) = blocks_at_number {
						if blocks_at_number.iter().any(|block| ScheduledChanges::exists(block)) {
							HeadersByNumber::insert(number, blocks_at_number);
							OldestUnprunedBlock::put(number);
							return;
						}
					}
				}

				// physically remove headers and (probably) obsolete validators sets
				for hash in blocks_at_number.into_iter().flat_map(|x| x) {
					let header = Headers::take(&hash);
					ScheduledChanges::remove(hash);
					if let Some(header) = header {
						ValidatorsSetsRc::mutate(header.next_validators_set_id, |rc| match *rc {
							Some(rc) if rc > 1 => Some(rc - 1),
							_ => None,
						});
					}
				}
			}

			OldestUnprunedBlock::put(prune_end);
		}
	}
}

/// Aura engine configuration for Kovan chain.
pub fn kovan_aura_config() -> AuraConfiguration {
	AuraConfiguration {
		empty_steps_transition: u64::max_value(),
		strict_empty_steps_transition: 0,
		validate_step_transition: 0x16e360,
		validate_score_transition: 0x41a3c4,
		two_thirds_majority_transition: u64::max_value(),
		min_gas_limit: 0x1388.into(),
		max_gas_limit: U256::max_value(),
		maximum_extra_data_size: 0x20,
	}
}

/// Validators configuration for Kovan chain.
pub fn kovan_validators_config() -> ValidatorsConfiguration {
	ValidatorsConfiguration::Multi(vec![
		(
			0,
			ValidatorsSource::List(vec![
				[
					0x00, 0xD6, 0xCc, 0x1B, 0xA9, 0xcf, 0x89, 0xBD, 0x2e, 0x58, 0x00, 0x97, 0x41, 0xf4, 0xF7, 0x32,
					0x5B, 0xAd, 0xc0, 0xED,
				]
				.into(),
				[
					0x00, 0x42, 0x7f, 0xea, 0xe2, 0x41, 0x9c, 0x15, 0xb8, 0x9d, 0x1c, 0x21, 0xaf, 0x10, 0xd1, 0xb6,
					0x65, 0x0a, 0x4d, 0x3d,
				]
				.into(),
				[
					0x4E, 0xd9, 0xB0, 0x8e, 0x63, 0x54, 0xC7, 0x0f, 0xE6, 0xF8, 0xCB, 0x04, 0x11, 0xb0, 0xd3, 0x24,
					0x6b, 0x42, 0x4d, 0x6c,
				]
				.into(),
				[
					0x00, 0x20, 0xee, 0x4B, 0xe0, 0xe2, 0x02, 0x7d, 0x76, 0x60, 0x3c, 0xB7, 0x51, 0xeE, 0x06, 0x95,
					0x19, 0xbA, 0x81, 0xA1,
				]
				.into(),
				[
					0x00, 0x10, 0xf9, 0x4b, 0x29, 0x6a, 0x85, 0x2a, 0xaa, 0xc5, 0x2e, 0xa6, 0xc5, 0xac, 0x72, 0xe0,
					0x3a, 0xfd, 0x03, 0x2d,
				]
				.into(),
				[
					0x00, 0x77, 0x33, 0xa1, 0xFE, 0x69, 0xCF, 0x3f, 0x2C, 0xF9, 0x89, 0xF8, 0x1C, 0x7b, 0x4c, 0xAc,
					0x16, 0x93, 0x38, 0x7A,
				]
				.into(),
				[
					0x00, 0xE6, 0xd2, 0xb9, 0x31, 0xF5, 0x5a, 0x3f, 0x17, 0x01, 0xc7, 0x38, 0x9d, 0x59, 0x2a, 0x77,
					0x78, 0x89, 0x78, 0x79,
				]
				.into(),
				[
					0x00, 0xe4, 0xa1, 0x06, 0x50, 0xe5, 0xa6, 0xD6, 0x00, 0x1C, 0x38, 0xff, 0x8E, 0x64, 0xF9, 0x70,
					0x16, 0xa1, 0x64, 0x5c,
				]
				.into(),
				[
					0x00, 0xa0, 0xa2, 0x4b, 0x9f, 0x0e, 0x5e, 0xc7, 0xaa, 0x4c, 0x73, 0x89, 0xb8, 0x30, 0x2f, 0xd0,
					0x12, 0x31, 0x94, 0xde,
				]
				.into(),
			]),
		),
		(
			10960440,
			ValidatorsSource::List(vec![
				[
					0x00, 0xD6, 0xCc, 0x1B, 0xA9, 0xcf, 0x89, 0xBD, 0x2e, 0x58, 0x00, 0x97, 0x41, 0xf4, 0xF7, 0x32,
					0x5B, 0xAd, 0xc0, 0xED,
				]
				.into(),
				[
					0x00, 0x10, 0xf9, 0x4b, 0x29, 0x6a, 0x85, 0x2a, 0xaa, 0xc5, 0x2e, 0xa6, 0xc5, 0xac, 0x72, 0xe0,
					0x3a, 0xfd, 0x03, 0x2d,
				]
				.into(),
				[
					0x00, 0xa0, 0xa2, 0x4b, 0x9f, 0x0e, 0x5e, 0xc7, 0xaa, 0x4c, 0x73, 0x89, 0xb8, 0x30, 0x2f, 0xd0,
					0x12, 0x31, 0x94, 0xde,
				]
				.into(),
			]),
		),
		(
			10960500,
			ValidatorsSource::Contract(
				[
					0xaE, 0x71, 0x80, 0x7C, 0x1B, 0x0a, 0x09, 0x3c, 0xB1, 0x54, 0x7b, 0x68, 0x2D, 0xC7, 0x83, 0x16,
					0xD9, 0x45, 0xc9, 0xB8,
				]
				.into(),
				vec![
					[
						0xd0, 0x5f, 0x74, 0x78, 0xc6, 0xaa, 0x10, 0x78, 0x12, 0x58, 0xc5, 0xcc, 0x8b, 0x4f, 0x38, 0x5f,
						0xc8, 0xfa, 0x98, 0x9c,
					]
					.into(),
					[
						0x03, 0x80, 0x1e, 0xfb, 0x0e, 0xfe, 0x2a, 0x25, 0xed, 0xe5, 0xdd, 0x3a, 0x00, 0x3a, 0xe8, 0x80,
						0xc0, 0x29, 0x2e, 0x4d,
					]
					.into(),
					[
						0xa4, 0xdf, 0x25, 0x5e, 0xcf, 0x08, 0xbb, 0xf2, 0xc2, 0x80, 0x55, 0xc6, 0x52, 0x25, 0xc9, 0xa9,
						0x84, 0x7a, 0xbd, 0x94,
					]
					.into(),
					[
						0x59, 0x6e, 0x82, 0x21, 0xa3, 0x0b, 0xfe, 0x6e, 0x7e, 0xff, 0x67, 0xfe, 0xe6, 0x64, 0xa0, 0x1c,
						0x73, 0xba, 0x3c, 0x56,
					]
					.into(),
					[
						0xfa, 0xad, 0xfa, 0xce, 0x3f, 0xbd, 0x81, 0xce, 0x37, 0xb0, 0xe1, 0x9c, 0x0b, 0x65, 0xff, 0x42,
						0x34, 0x14, 0x81, 0x32,
					]
					.into(),
				],
			),
		),
	])
}

/// Return iterator of given header ancestors.
pub(crate) fn ancestry<'a, S: Storage>(storage: &'a S, header: &Header) -> impl Iterator<Item = (H256, Header)> + 'a {
	let mut parent_hash = header.parent_hash.clone();
	from_fn(move || {
		let header = storage.header(&parent_hash);
		match header {
			Some(header) => {
				if header.number == 0 {
					return None;
				}

				let hash = parent_hash.clone();
				parent_hash = header.parent_hash.clone();
				Some((hash, header))
			}
			None => None,
		}
	})
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use parity_crypto::publickey::{sign, KeyPair, Secret};
	use primitives::{rlp_encode, H520};
	use std::collections::{hash_map::Entry, HashMap};

	pub fn genesis() -> Header {
		Header {
			seal: vec![vec![42].into(), vec![].into()],
			..Default::default()
		}
	}

	pub fn block_i(storage: &InMemoryStorage, number: u64, validators: &[KeyPair]) -> Header {
		custom_block_i(storage, number, validators, |_| {})
	}

	pub fn custom_block_i(
		storage: &InMemoryStorage,
		number: u64,
		validators: &[KeyPair],
		customize: impl FnOnce(&mut Header),
	) -> Header {
		let validator_index: u8 = (number % (validators.len() as u64)) as _;
		let mut header = Header {
			number,
			parent_hash: storage.headers_by_number[&(number - 1)][0].clone(),
			gas_limit: 0x2000.into(),
			author: validator(validator_index).address().to_fixed_bytes().into(),
			seal: vec![vec![number as u8 + 42].into(), vec![].into()],
			difficulty: number.into(),
			..Default::default()
		};
		customize(&mut header);
		signed_header(validators, header, number + 42)
	}

	pub fn signed_header(validators: &[KeyPair], mut header: Header, step: u64) -> Header {
		let message = header.seal_hash(false).unwrap();
		let validator_index = (step % validators.len() as u64) as usize;
		let signature = sign(validators[validator_index].secret(), &message.as_fixed_bytes().into()).unwrap();
		let signature: [u8; 65] = signature.into();
		let signature = H520::from(signature);
		header.seal[1] = rlp_encode(&signature);
		header
	}

	pub fn validator(index: u8) -> KeyPair {
		KeyPair::from_secret(Secret::from([index + 1; 32])).unwrap()
	}

	pub fn validators_addresses(count: u8) -> Vec<Address> {
		(0..count as usize)
			.map(|i| validator(i as u8).address().as_fixed_bytes().into())
			.collect()
	}

	pub struct InMemoryStorage {
		best_block: (u64, H256, U256),
		finalized_block: (u64, H256),
		oldest_unpruned_block: u64,
		headers: HashMap<H256, StoredHeader>,
		headers_by_number: HashMap<u64, Vec<H256>>,
		next_validators_set_id: u64,
		validators_sets: HashMap<u64, (H256, Vec<Address>)>,
		validators_sets_rc: HashMap<u64, u64>,
		scheduled_changes: HashMap<H256, Vec<Address>>,
	}

	impl InMemoryStorage {
		pub fn new(initial_header: Header, initial_validators: Vec<Address>) -> Self {
			let hash = initial_header.hash();
			InMemoryStorage {
				best_block: (initial_header.number, hash, 0.into()),
				finalized_block: (initial_header.number, hash),
				oldest_unpruned_block: initial_header.number,
				headers_by_number: vec![(initial_header.number, vec![hash])].into_iter().collect(),
				headers: vec![(
					hash,
					StoredHeader {
						header: initial_header,
						total_difficulty: 0.into(),
						next_validators_set_id: 0,
					},
				)]
				.into_iter()
				.collect(),
				next_validators_set_id: 1,
				validators_sets: vec![(0, (hash, initial_validators))].into_iter().collect(),
				validators_sets_rc: vec![(0, 1)].into_iter().collect(),
				scheduled_changes: HashMap::new(),
			}
		}

		pub(crate) fn oldest_unpruned_block(&self) -> u64 {
			self.oldest_unpruned_block
		}

		pub(crate) fn stored_header(&self, hash: &H256) -> Option<&StoredHeader> {
			self.headers.get(hash)
		}
	}

	impl Storage for InMemoryStorage {
		fn best_block(&self) -> (u64, H256, U256) {
			self.best_block.clone()
		}

		fn finalized_block(&self) -> (u64, H256) {
			self.finalized_block.clone()
		}

		fn header(&self, hash: &H256) -> Option<Header> {
			self.headers.get(hash).map(|header| header.header.clone())
		}

		fn import_context(&self, parent_hash: &H256) -> Option<ImportContext> {
			self.headers.get(parent_hash).map(|parent_header| {
				let (next_validators_set_start, next_validators) =
					self.validators_sets.get(&parent_header.next_validators_set_id).unwrap();
				ImportContext {
					parent_header: parent_header.header.clone(),
					parent_total_difficulty: parent_header.total_difficulty,
					next_validators_set_id: parent_header.next_validators_set_id,
					next_validators_set: (*next_validators_set_start, next_validators.clone()),
				}
			})
		}

		fn scheduled_change(&self, hash: &H256) -> Option<Vec<Address>> {
			self.scheduled_changes.get(hash).cloned()
		}

		fn insert_header(&mut self, header: HeaderToImport) {
			if header.is_best {
				self.best_block = (header.header.number, header.hash, header.total_difficulty);
			}
			if let Some(scheduled_change) = header.scheduled_change {
				self.scheduled_changes.insert(header.hash, scheduled_change);
			}
			let next_validators_set_id = match header.enacted_change {
				Some(enacted_change) => {
					let next_validators_set_id = self.next_validators_set_id;
					self.next_validators_set_id += 1;
					self.validators_sets
						.insert(next_validators_set_id, (header.hash, enacted_change));
					self.validators_sets_rc.insert(next_validators_set_id, 1);
					next_validators_set_id
				}
				None => {
					*self
						.validators_sets_rc
						.entry(header.context.next_validators_set_id)
						.or_default() += 1;
					header.context.next_validators_set_id
				}
			};

			self.headers_by_number
				.entry(header.header.number)
				.or_default()
				.push(header.hash);
			self.headers.insert(
				header.hash,
				StoredHeader {
					header: header.header,
					total_difficulty: header.total_difficulty,
					next_validators_set_id,
				},
			);
		}

		fn finalize_headers(&mut self, finalized: Option<(u64, H256)>, prune_end: Option<u64>) {
			let finalized_number = finalized
				.as_ref()
				.map(|f| f.0)
				.unwrap_or_else(|| self.finalized_block.0);
			if let Some(finalized) = finalized {
				self.finalized_block = finalized;
			}

			if let Some(prune_end) = prune_end {
				let prune_begin = self.oldest_unpruned_block;

				for number in prune_begin..prune_end {
					let blocks_at_number = self.headers_by_number.remove(&number);

					// ensure that unfinalized headers we want to prune do not have scheduled changes
					if number > finalized_number {
						if let Some(ref blocks_at_number) = blocks_at_number {
							if blocks_at_number
								.iter()
								.any(|block| self.scheduled_changes.contains_key(block))
							{
								self.headers_by_number.insert(number, blocks_at_number.clone());
								self.oldest_unpruned_block = number;
								return;
							}
						}
					}

					// physically remove headers and (probably) obsolete validators sets
					for hash in blocks_at_number.into_iter().flat_map(|x| x) {
						let header = self.headers.remove(&hash);
						self.scheduled_changes.remove(&hash);
						if let Some(header) = header {
							match self.validators_sets_rc.entry(header.next_validators_set_id) {
								Entry::Occupied(mut entry) => {
									if *entry.get() == 1 {
										entry.remove();
									} else {
										*entry.get_mut() -= 1;
									}
								}
								Entry::Vacant(_) => unreachable!("there's entry for each header"),
							};
						}
					}
				}

				self.oldest_unpruned_block = prune_end;
			}
		}
	}
}
