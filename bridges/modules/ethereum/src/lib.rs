// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{decl_module, decl_storage};
use primitives::{Address, Header, Receipt, H256, U256};
use sp_runtime::{
	transaction_validity::{
		InvalidTransaction, TransactionLongevity, TransactionPriority, TransactionSource, TransactionValidity,
		UnknownTransaction, ValidTransaction,
	},
	RuntimeDebug,
};
use sp_std::{cmp::Ord, collections::btree_map::BTreeMap, prelude::*};
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

/// Transaction pool configuration.
///
/// This is used to limit number of unsigned headers transactions in
/// the pool. We never use it to verify signed transactions.
pub struct PoolConfiguration {
	/// Maximal difference between number of header from unsigned transaction
	/// and current best block. This must be selected with caution - the more
	/// is the difference, the more (potentially invalid) transactions could be
	/// accepted to the pool and mined later (filling blocks with spam).
	pub max_future_number_difference: u64,
}

/// Block header as it is stored in the runtime storage.
#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug)]
pub struct StoredHeader<Submitter> {
	/// Submitter of this header. May be `None` if header has been submitted
	/// using unsigned transaction.
	pub submitter: Option<Submitter>,
	/// The block header itself.
	pub header: Header,
	/// Total difficulty of the chain.
	pub total_difficulty: U256,
	/// The ID of set of validators that is expected to produce direct descendants of
	/// this block. If header enacts new set, this would be the new set. Otherwise
	/// this is the set that has produced the block itself.
	/// The hash is the hash of block where validators set has been enacted.
	pub next_validators_set_id: u64,
	/// Hash of the last block which has **SCHEDULED** validators set change.
	/// Note that signal doesn't mean that the set has been (or ever will be) enacted.
	/// Note that the header may already be pruned.
	pub last_signal_block: Option<H256>,
}

/// Validators set as it is stored in the runtime storage.
#[derive(Encode, Decode, PartialEq, RuntimeDebug)]
#[cfg_attr(test, derive(Clone))]
pub struct ValidatorsSet {
	/// Validators of this set.
	pub validators: Vec<Address>,
	/// Hash of the block where this set has been signalled. None if this is the first set.
	pub signal_block: Option<H256>,
	/// Hash of the block where this set has been enacted.
	pub enact_block: H256,
}

/// Validators set change as it is stored in the runtime storage.
#[derive(Encode, Decode, PartialEq, RuntimeDebug)]
#[cfg_attr(test, derive(Clone))]
pub struct ScheduledChange {
	/// Validators of this set.
	pub validators: Vec<Address>,
	/// Hash of the block which has emitted previous validators change signal.
	pub prev_signal_block: Option<H256>,
}

/// Header that we're importing.
#[derive(RuntimeDebug)]
#[cfg_attr(test, derive(Clone, PartialEq))]
pub struct HeaderToImport<Submitter> {
	/// Header import context,
	pub context: ImportContext<Submitter>,
	/// Should we consider this header as best?
	pub is_best: bool,
	/// The hash of the header.
	pub hash: H256,
	/// The header itself.
	pub header: Header,
	/// Total chain difficulty at the header.
	pub total_difficulty: U256,
	/// New validators set and the hash of block where it has been scheduled (if applicable).
	/// Some if set is is enacted by this header.
	pub enacted_change: Option<ChangeToEnact>,
	/// Validators set scheduled change, if happened at the header.
	pub scheduled_change: Option<Vec<Address>>,
}

/// Header that we're importing.
#[derive(RuntimeDebug)]
#[cfg_attr(test, derive(Clone, PartialEq))]
pub struct ChangeToEnact {
	/// The hash of the header where change has been scheduled.
	/// None if it is a first set within current `ValidatorsSource`.
	pub signal_block: Option<H256>,
	/// Validators set that is enacted.
	pub validators: Vec<Address>,
}

/// Header import context.
///
/// The import context contains information needed by the header verification
/// pipeline which is not directly part of the header being imported. This includes
/// information relating to its parent, and the current validator set (which
/// provide _context_ for the current header).
#[derive(RuntimeDebug)]
#[cfg_attr(test, derive(Clone, PartialEq))]
pub struct ImportContext<Submitter> {
	submitter: Option<Submitter>,
	parent_hash: H256,
	parent_header: Header,
	parent_total_difficulty: U256,
	parent_scheduled_change: Option<ScheduledChange>,
	validators_set_id: u64,
	validators_set: ValidatorsSet,
	last_signal_block: Option<H256>,
}

impl<Submitter> ImportContext<Submitter> {
	/// Returns reference to header submitter (if known).
	pub fn submitter(&self) -> Option<&Submitter> {
		self.submitter.as_ref()
	}

	/// Returns reference to parent header.
	pub fn parent_header(&self) -> &Header {
		&self.parent_header
	}

	/// Returns total chain difficulty at parent block.
	pub fn total_difficulty(&self) -> &U256 {
		&self.parent_total_difficulty
	}

	/// Returns the validator set change if the parent header has signaled a change.
	pub fn parent_scheduled_change(&self) -> Option<&ScheduledChange> {
		self.parent_scheduled_change.as_ref()
	}

	/// Returns id of the set of validators.
	pub fn validators_set_id(&self) -> u64 {
		self.validators_set_id
	}

	/// Returns reference to validators set for the block we're going to import.
	pub fn validators_set(&self) -> &ValidatorsSet {
		&self.validators_set
	}

	/// Returns reference to the latest block which has signalled change of validators set.
	/// This may point to parent if parent has signalled change.
	pub fn last_signal_block(&self) -> Option<&H256> {
		match self.parent_scheduled_change {
			Some(_) => Some(&self.parent_hash),
			None => self.last_signal_block.as_ref(),
		}
	}

	/// Converts import context into header we're going to import.
	pub fn into_import_header(
		self,
		is_best: bool,
		hash: H256,
		header: Header,
		total_difficulty: U256,
		enacted_change: Option<ChangeToEnact>,
		scheduled_change: Option<Vec<Address>>,
	) -> HeaderToImport<Submitter> {
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
	/// Header submitter identifier.
	type Submitter: Clone + Ord;

	/// Get best known block.
	fn best_block(&self) -> (u64, H256, U256);
	/// Get last finalized block.
	fn finalized_block(&self) -> (u64, H256);
	/// Get imported header by its hash.
	///
	/// Returns header and its submitter (if known).
	fn header(&self, hash: &H256) -> Option<(Header, Option<Self::Submitter>)>;
	/// Get header import context by parent header hash.
	fn import_context(
		&self,
		submitter: Option<Self::Submitter>,
		parent_hash: &H256,
	) -> Option<ImportContext<Self::Submitter>>;
	/// Get new validators that are scheduled by given header and hash of the previous
	/// block that has scheduled change.
	fn scheduled_change(&self, hash: &H256) -> Option<ScheduledChange>;
	/// Insert imported header.
	fn insert_header(&mut self, header: HeaderToImport<Self::Submitter>);
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
	///
	/// The submitter **must not** be rewarded for submitting valid headers, because greedy authority
	/// could produce and submit multiple valid headers (without relaying them to other peers) and
	/// get rewarded. Instead, the provider could track submitters and stop rewarding if too many
	/// headers have been submitted without finalization.
	fn on_valid_headers_submitted(submitter: AccountId, useful: u64, useless: u64);
	/// Called when invalid headers have been submitted.
	fn on_invalid_headers_submitted(submitter: AccountId);
	/// Called when earlier submitted headers have been finalized.
	///
	/// finalized is the number of headers that submitter has submitted and which
	/// have been finalized.
	fn on_valid_headers_finalized(submitter: AccountId, finalized: u64);
}

impl<AccountId> OnHeadersSubmitted<AccountId> for () {
	fn on_valid_headers_submitted(_submitter: AccountId, _useful: u64, _useless: u64) {}
	fn on_invalid_headers_submitted(_submitter: AccountId) {}
	fn on_valid_headers_finalized(_submitter: AccountId, _finalized: u64) {}
}

/// The module configuration trait
pub trait Trait: frame_system::Trait {
	/// Handler for headers submission result.
	type OnHeadersSubmitted: OnHeadersSubmitted<Self::AccountId>;
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		/// Import single Aura header. Requires transaction to be **UNSIGNED**.
		#[weight = 0] // TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
		pub fn import_unsigned_header(origin, header: Header, receipts: Option<Vec<Receipt>>) {
			frame_system::ensure_none(origin)?;

			import_header(
				&mut BridgeStorage::<T>::new(),
				&kovan_aura_config(),
				&kovan_validators_config(),
				crate::import::PRUNE_DEPTH,
				None,
				header,
				receipts,
			).map_err(|e| e.msg())?;
		}

		/// Import Aura chain headers in a single **SIGNED** transaction.
		/// Ignores non-fatal errors (like when known header is provided), rewards
		/// for successful headers import and penalizes for fatal errors.
		///
		/// This should be used with caution - passing too many headers could lead to
		/// enormous block production/import time.
		#[weight = 0] // TODO: update me (https://github.com/paritytech/parity-bridges-common/issues/78)
		pub fn import_signed_headers(origin, headers_with_receipts: Vec<(Header, Option<Vec<Receipt>>)>) {
			let submitter = frame_system::ensure_signed(origin)?;
			let mut finalized_headers = BTreeMap::new();
			let import_result = import::import_headers(
				&mut BridgeStorage::<T>::new(),
				&kovan_aura_config(),
				&kovan_validators_config(),
				crate::import::PRUNE_DEPTH,
				Some(submitter.clone()),
				headers_with_receipts,
				&mut finalized_headers,
			);

			// if we have finalized some headers, we will reward their submitters even
			// if current submitter has provided some invalid headers
			for (f_submitter, f_count) in finalized_headers {
				T::OnHeadersSubmitted::on_valid_headers_finalized(
					f_submitter,
					f_count,
				);
			}

			// now track/penalize current submitter for providing new headers
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
		Headers: map hasher(identity) H256 => Option<StoredHeader<T::AccountId>>;
		/// Map of imported header hashes by number.
		HeadersByNumber: map hasher(blake2_128_concat) u64 => Option<Vec<H256>>;
		/// The ID of next validator set.
		NextValidatorsSetId: u64;
		/// Map of validators sets by their id.
		ValidatorsSets: map hasher(twox_64_concat) u64 => Option<ValidatorsSet>;
		/// Validators sets reference count. Each header that is authored by this set increases
		/// the reference count. When we prune this header, we decrease the reference count.
		/// When it reaches zero, we are free to prune validator set as well.
		ValidatorsSetsRc: map hasher(twox_64_concat) u64 => Option<u64>;
		/// Map of validators set changes scheduled by given header.
		ScheduledChanges: map hasher(identity) H256 => Option<ScheduledChange>;
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
			Headers::<T>::insert(initial_hash, StoredHeader {
				submitter: None,
				header: config.initial_header.clone(),
				total_difficulty: config.initial_difficulty,
				next_validators_set_id: 0,
				last_signal_block: None,
			});
			NextValidatorsSetId::put(1);
			ValidatorsSets::insert(0, ValidatorsSet {
				validators: config.initial_validators.clone(),
				signal_block: None,
				enact_block: initial_hash,
			});
			ValidatorsSetsRc::insert(0, 1);
		})
	}
}

impl<T: Trait> Module<T> {
	/// Returns number and hash of the best block known to the bridge module.
	/// The caller should only submit `import_header` transaction that makes
	/// (or leads to making) other header the best one.
	pub fn best_block() -> (u64, H256) {
		let (number, hash, _) = BridgeStorage::<T>::new().best_block();
		(number, hash)
	}

	/// Returns true if the import of given block requires transactions receipts.
	pub fn is_import_requires_receipts(header: Header) -> bool {
		import::header_import_requires_receipts(&BridgeStorage::<T>::new(), &kovan_validators_config(), &header)
	}

	/// Returns true if header is known to the runtime.
	pub fn is_known_block(hash: H256) -> bool {
		BridgeStorage::<T>::new().header(&hash).is_some()
	}
}

impl<T: Trait> frame_support::unsigned::ValidateUnsigned for Module<T> {
	type Call = Call<T>;

	fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
		match *call {
			Self::Call::import_unsigned_header(ref header, ref receipts) => {
				let accept_result = verification::accept_aura_header_into_pool(
					&BridgeStorage::<T>::new(),
					&kovan_aura_config(),
					&kovan_validators_config(),
					&pool_configuration(),
					header,
					receipts.as_ref(),
				);

				match accept_result {
					Ok((requires, provides)) => Ok(ValidTransaction {
						priority: TransactionPriority::max_value(),
						requires,
						provides,
						longevity: TransactionLongevity::max_value(),
						propagate: true,
					}),
					// UnsignedTooFarInTheFuture is the special error code used to limit
					// number of transactions in the pool - we do not want to ban transaction
					// in this case (see verification.rs for details)
					Err(error::Error::UnsignedTooFarInTheFuture) => {
						UnknownTransaction::Custom(error::Error::UnsignedTooFarInTheFuture.code()).into()
					}
					Err(error) => InvalidTransaction::Custom(error.code()).into(),
				}
			}
			_ => InvalidTransaction::Call.into(),
		}
	}
}

/// Runtime bridge storage.
#[derive(Default)]
struct BridgeStorage<T>(sp_std::marker::PhantomData<T>);

impl<T> BridgeStorage<T> {
	pub fn new() -> Self {
		BridgeStorage(sp_std::marker::PhantomData::<T>::default())
	}
}

impl<T: Trait> Storage for BridgeStorage<T> {
	type Submitter = T::AccountId;

	fn best_block(&self) -> (u64, H256, U256) {
		BestBlock::get()
	}

	fn finalized_block(&self) -> (u64, H256) {
		FinalizedBlock::get()
	}

	fn header(&self, hash: &H256) -> Option<(Header, Option<Self::Submitter>)> {
		Headers::<T>::get(hash).map(|header| (header.header, header.submitter))
	}

	fn import_context(
		&self,
		submitter: Option<Self::Submitter>,
		parent_hash: &H256,
	) -> Option<ImportContext<Self::Submitter>> {
		Headers::<T>::get(parent_hash).map(|parent_header| {
			let validators_set = ValidatorsSets::get(parent_header.next_validators_set_id)
				.expect("validators set is only pruned when last ref is pruned; there is a ref; qed");
			let parent_scheduled_change = ScheduledChanges::get(parent_hash);
			ImportContext {
				submitter,
				parent_hash: *parent_hash,
				parent_header: parent_header.header,
				parent_total_difficulty: parent_header.total_difficulty,
				parent_scheduled_change,
				validators_set_id: parent_header.next_validators_set_id,
				validators_set,
				last_signal_block: parent_header.last_signal_block,
			}
		})
	}

	fn scheduled_change(&self, hash: &H256) -> Option<ScheduledChange> {
		ScheduledChanges::get(hash)
	}

	fn insert_header(&mut self, header: HeaderToImport<Self::Submitter>) {
		if header.is_best {
			BestBlock::put((header.header.number, header.hash, header.total_difficulty));
		}
		if let Some(scheduled_change) = header.scheduled_change {
			ScheduledChanges::insert(
				&header.hash,
				ScheduledChange {
					validators: scheduled_change,
					prev_signal_block: header.context.last_signal_block,
				},
			);
		}
		let next_validators_set_id = match header.enacted_change {
			Some(enacted_change) => {
				let next_validators_set_id = NextValidatorsSetId::mutate(|set_id| {
					let next_set_id = *set_id;
					*set_id += 1;
					next_set_id
				});
				ValidatorsSets::insert(
					next_validators_set_id,
					ValidatorsSet {
						validators: enacted_change.validators,
						enact_block: header.hash,
						signal_block: enacted_change.signal_block,
					},
				);
				ValidatorsSetsRc::insert(next_validators_set_id, 1);
				next_validators_set_id
			}
			None => {
				ValidatorsSetsRc::mutate(header.context.validators_set_id, |rc| {
					*rc = Some(rc.map(|rc| rc + 1).unwrap_or(1));
					*rc
				});
				header.context.validators_set_id
			}
		};

		let last_signal_block = header.context.last_signal_block().cloned();
		HeadersByNumber::append_or_insert(header.header.number, vec![header.hash]);
		Headers::<T>::insert(
			&header.hash,
			StoredHeader {
				submitter: header.context.submitter,
				header: header.header,
				total_difficulty: header.total_difficulty,
				next_validators_set_id,
				last_signal_block,
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
						if blocks_at_number
							.iter()
							.any(|block| ScheduledChanges::contains_key(block))
						{
							HeadersByNumber::insert(number, blocks_at_number);
							OldestUnprunedBlock::put(number);
							return;
						}
					}
				}

				// physically remove headers and (probably) obsolete validators sets
				for hash in blocks_at_number.into_iter().flat_map(|x| x) {
					let header = Headers::<T>::take(&hash);
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

/// Transaction pool configuration.
fn pool_configuration() -> PoolConfiguration {
	PoolConfiguration {
		max_future_number_difference: 10,
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use parity_crypto::publickey::{sign, KeyPair, Secret};
	use primitives::{rlp_encode, H520};
	use std::collections::{hash_map::Entry, HashMap};

	pub type AccountId = u64;

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
		headers: HashMap<H256, StoredHeader<AccountId>>,
		headers_by_number: HashMap<u64, Vec<H256>>,
		next_validators_set_id: u64,
		validators_sets: HashMap<u64, ValidatorsSet>,
		validators_sets_rc: HashMap<u64, u64>,
		scheduled_changes: HashMap<H256, ScheduledChange>,
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
						submitter: None,
						header: initial_header,
						total_difficulty: 0.into(),
						next_validators_set_id: 0,
						last_signal_block: None,
					},
				)]
				.into_iter()
				.collect(),
				next_validators_set_id: 1,
				validators_sets: vec![(
					0,
					ValidatorsSet {
						validators: initial_validators,
						signal_block: None,
						enact_block: hash,
					},
				)]
				.into_iter()
				.collect(),
				validators_sets_rc: vec![(0, 1)].into_iter().collect(),
				scheduled_changes: HashMap::new(),
			}
		}

		pub(crate) fn insert(&mut self, header: Header) {
			let hash = header.hash();
			self.headers_by_number.entry(header.number).or_default().push(hash);
			self.headers.insert(
				hash,
				StoredHeader {
					submitter: None,
					header,
					total_difficulty: 0.into(),
					next_validators_set_id: 0,
					last_signal_block: None,
				},
			);
		}

		pub(crate) fn change_validators_set_at(
			&mut self,
			number: u64,
			finalized_set: Vec<Address>,
			signalled_set: Option<Vec<Address>>,
		) {
			let set_id = self.next_validators_set_id;
			self.next_validators_set_id += 1;
			self.validators_sets.insert(
				set_id,
				ValidatorsSet {
					validators: finalized_set,
					signal_block: None,
					enact_block: self.headers_by_number[&0][0],
				},
			);

			let mut header = self.headers.get_mut(&self.headers_by_number[&number][0]).unwrap();
			header.next_validators_set_id = set_id;
			if let Some(signalled_set) = signalled_set {
				header.last_signal_block = Some(self.headers_by_number[&(number - 1)][0]);
				self.scheduled_changes.insert(
					self.headers_by_number[&(number - 1)][0],
					ScheduledChange {
						validators: signalled_set,
						prev_signal_block: None,
					},
				);
			}
		}

		pub(crate) fn set_best_block(&mut self, best_block: (u64, H256)) {
			self.best_block.0 = best_block.0;
			self.best_block.1 = best_block.1;
		}

		pub(crate) fn set_finalized_block(&mut self, finalized_block: (u64, H256)) {
			self.finalized_block = finalized_block;
		}

		pub(crate) fn oldest_unpruned_block(&self) -> u64 {
			self.oldest_unpruned_block
		}

		pub(crate) fn stored_header(&self, hash: &H256) -> Option<&StoredHeader<AccountId>> {
			self.headers.get(hash)
		}
	}

	impl Storage for InMemoryStorage {
		type Submitter = AccountId;

		fn best_block(&self) -> (u64, H256, U256) {
			self.best_block.clone()
		}

		fn finalized_block(&self) -> (u64, H256) {
			self.finalized_block.clone()
		}

		fn header(&self, hash: &H256) -> Option<(Header, Option<Self::Submitter>)> {
			self.headers
				.get(hash)
				.map(|header| (header.header.clone(), header.submitter.clone()))
		}

		fn import_context(
			&self,
			submitter: Option<Self::Submitter>,
			parent_hash: &H256,
		) -> Option<ImportContext<Self::Submitter>> {
			self.headers.get(parent_hash).map(|parent_header| {
				let validators_set = self
					.validators_sets
					.get(&parent_header.next_validators_set_id)
					.unwrap()
					.clone();
				let parent_scheduled_change = self.scheduled_changes.get(parent_hash).cloned();
				ImportContext {
					submitter,
					parent_hash: *parent_hash,
					parent_header: parent_header.header.clone(),
					parent_total_difficulty: parent_header.total_difficulty,
					parent_scheduled_change,
					validators_set_id: parent_header.next_validators_set_id,
					validators_set,
					last_signal_block: parent_header.last_signal_block,
				}
			})
		}

		fn scheduled_change(&self, hash: &H256) -> Option<ScheduledChange> {
			self.scheduled_changes.get(hash).cloned()
		}

		fn insert_header(&mut self, header: HeaderToImport<Self::Submitter>) {
			if header.is_best {
				self.best_block = (header.header.number, header.hash, header.total_difficulty);
			}
			if let Some(scheduled_change) = header.scheduled_change {
				self.scheduled_changes.insert(
					header.hash,
					ScheduledChange {
						validators: scheduled_change,
						prev_signal_block: header.context.last_signal_block,
					},
				);
			}
			let next_validators_set_id = match header.enacted_change {
				Some(enacted_change) => {
					let next_validators_set_id = self.next_validators_set_id;
					self.next_validators_set_id += 1;
					self.validators_sets.insert(
						next_validators_set_id,
						ValidatorsSet {
							validators: enacted_change.validators,
							enact_block: header.hash,
							signal_block: enacted_change.signal_block,
						},
					);
					self.validators_sets_rc.insert(next_validators_set_id, 1);
					next_validators_set_id
				}
				None => {
					*self
						.validators_sets_rc
						.entry(header.context.validators_set_id)
						.or_default() += 1;
					header.context.validators_set_id
				}
			};

			let last_signal_block = header.context.last_signal_block().cloned();
			self.headers_by_number
				.entry(header.header.number)
				.or_default()
				.push(header.hash);
			self.headers.insert(
				header.hash,
				StoredHeader {
					submitter: header.context.submitter,
					header: header.header,
					total_difficulty: header.total_difficulty,
					next_validators_set_id,
					last_signal_block,
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
