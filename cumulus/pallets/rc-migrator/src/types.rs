// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Types

extern crate alloc;

use super::*;
use alloc::string::String;
use pallet_referenda::{ReferendumInfoOf, TrackIdOf};
use sp_runtime::{traits::Zero, FixedU128};
use sp_std::collections::vec_deque::VecDeque;

pub trait ToPolkadotSs58 {
	fn to_polkadot_ss58(&self) -> String;
}

impl ToPolkadotSs58 for AccountId32 {
	fn to_polkadot_ss58(&self) -> String {
		self.to_ss58check_with_version(sp_core::crypto::Ss58AddressFormat::custom(0))
	}
}

pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// Asset Hub Pallet list with indexes.
#[derive(Encode, Decode)]
pub enum AssetHubPalletConfig<T: Config> {
	#[codec(index = 255)]
	AhmController(AhMigratorCall<T>),
}

/// Call encoding for the calls needed from the ah-migrator pallet.
#[derive(Encode, Decode)]
pub enum AhMigratorCall<T: Config> {
	#[codec(index = 0)]
	ReceiveAccounts { accounts: Vec<accounts::AccountFor<T>> },
	#[codec(index = 1)]
	ReceiveMultisigs { multisigs: Vec<multisig::RcMultisigOf<T>> },
	#[codec(index = 2)]
	ReceiveProxyProxies { proxies: Vec<proxy::RcProxyLocalOf<T>> },
	#[codec(index = 3)]
	ReceiveProxyAnnouncements { announcements: Vec<RcProxyAnnouncementOf<T>> },
	#[codec(index = 4)]
	ReceivePreimageChunks { chunks: Vec<preimage::RcPreimageChunk> },
	#[codec(index = 5)]
	ReceivePreimageRequestStatus { request_status: Vec<preimage::RcPreimageRequestStatusOf<T>> },
	#[codec(index = 6)]
	ReceivePreimageLegacyStatus { legacy_status: Vec<preimage::RcPreimageLegacyStatusOf<T>> },
	#[codec(index = 7)]
	ReceiveNomPoolsMessages { messages: Vec<staking::nom_pools::RcNomPoolsMessage<T>> },
	#[codec(index = 8)]
	ReceiveVestingSchedules { messages: Vec<vesting::RcVestingSchedule<T>> },
	#[codec(index = 9)]
	ReceiveFastUnstakeMessages { messages: Vec<staking::fast_unstake::RcFastUnstakeMessage<T>> },
	#[codec(index = 10)]
	ReceiveReferendaValues {
		referendum_count: u32,
		deciding_count: Vec<(TrackIdOf<T, ()>, u32)>,
		track_queue: Vec<(TrackIdOf<T, ()>, Vec<(u32, u128)>)>,
	},
	#[codec(index = 11)]
	ReceiveReferendums { referendums: Vec<(u32, ReferendumInfoOf<T, ()>)> },
	#[cfg(not(feature = "ahm-westend"))]
	#[codec(index = 12)]
	ReceiveClaimsMessages { messages: Vec<claims::RcClaimsMessageOf<T>> },
	#[codec(index = 13)]
	ReceiveBagsListMessages { messages: Vec<staking::bags_list::RcBagsListMessage<T>> },
	#[codec(index = 14)]
	ReceiveSchedulerMessages { messages: Vec<scheduler::RcSchedulerMessageOf<T>> },
	#[codec(index = 15)]
	ReceiveIndices { indices: Vec<indices::RcIndicesIndexOf<T>> },
	#[codec(index = 16)]
	ReceiveConvictionVotingMessages {
		messages: Vec<conviction_voting::RcConvictionVotingMessageOf<T>>,
	},
	#[cfg(not(feature = "ahm-westend"))]
	#[codec(index = 17)]
	ReceiveBountiesMessages { messages: Vec<bounties::RcBountiesMessageOf<T>> },
	#[codec(index = 18)]
	ReceiveAssetRates { asset_rates: Vec<(<T as pallet_asset_rate::Config>::AssetKind, FixedU128)> },
	#[cfg(not(feature = "ahm-westend"))]
	#[codec(index = 19)]
	ReceiveCrowdloanMessages { messages: Vec<crowdloan::RcCrowdloanMessageOf<T>> },
	#[codec(index = 20)]
	ReceiveReferendaMetadata { metadata: Vec<(u32, <T as frame_system::Config>::Hash)> },
	#[cfg(not(feature = "ahm-westend"))]
	#[codec(index = 21)]
	ReceiveTreasuryMessages { messages: Vec<treasury::RcTreasuryMessageOf<T>> },
	#[codec(index = 22)]
	ReceiveSchedulerAgendaMessages {
		messages: Vec<(BlockNumberFor<T>, Vec<Option<scheduler::alias::ScheduledOf<T>>>)>,
	},
	#[codec(index = 30)]
	#[cfg(feature = "ahm-staking-migration")] // Staking migration not yet enabled
	ReceiveStakingMessages { messages: Vec<staking::RcStakingMessageOf<T>> },
	#[codec(index = 101)]
	StartMigration,
	#[codec(index = 110)]
	FinishMigration { data: MigrationFinishedData<BalanceOf<T>> },

	#[codec(index = 255)]
	#[cfg(feature = "runtime-benchmarks")]
	TestCall { data: Vec<Vec<u8>> },
}

/// Further data coming from Relay Chain alongside the signal that migration has finished.
#[derive(Encode, Decode, Clone, Default, RuntimeDebug, TypeInfo, MaxEncodedLen, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub struct MigrationFinishedData<Balance: Default> {
	/// Total native token balance NOT migrated from Relay Chain
	pub rc_balance_kept: Balance,
}

/// Copy of `ParaInfo` type from `paras_registrar` pallet.
///
/// From: https://github.com/paritytech/polkadot-sdk/blob/b7afe48ed0bfef30836e7ca6359c2d8bb594d16e/polkadot/runtime/common/src/paras_registrar/mod.rs#L50-L59
#[derive(Encode, Decode, Clone, PartialEq, Eq, Default, RuntimeDebug, TypeInfo)]
pub struct ParaInfo<AccountId, Balance> {
	/// The account that has placed a deposit for registering this para.
	pub manager: AccountId,
	/// The amount reserved by the `manager` account for the registration.
	pub deposit: Balance,
	/// Whether the para registration should be locked from being controlled by the manager.
	/// None means the lock had not been explicitly set, and should be treated as false.
	pub locked: Option<bool>,
}

pub trait PalletMigration {
	type Key: codec::MaxEncodedLen;
	type Error;

	/// Migrate until the weight is exhausted. The give key is the last one that was migrated.
	///
	/// Should return the last key that was migrated. This will then be passed back into the next
	/// call.
	fn migrate_many(
		last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error>;
}

/// Trait to run some checks on the Relay Chain before and after a pallet migration.
///
/// This needs to be called by the test harness.
pub trait RcMigrationCheck {
	/// Relay Chain payload which is exported for migration checks.
	type RcPrePayload: Clone;

	/// Run some checks on the relay chain before the migration and store intermediate payload.
	/// The expected output should contain the data being transferred out of the relay chain and it
	/// will .
	fn pre_check() -> Self::RcPrePayload;

	/// Run some checks on the relay chain after the migration and use the intermediate payload.
	/// The expected input should contain the data just transferred out of the relay chain, to allow
	/// the check that data has been removed from the relay chain.
	fn post_check(rc_pre_payload: Self::RcPrePayload);
}

#[impl_trait_for_tuples::impl_for_tuples(24)]
impl RcMigrationCheck for Tuple {
	for_tuples! { type RcPrePayload = (#( Tuple::RcPrePayload ),* ); }

	fn pre_check() -> Self::RcPrePayload {
		(for_tuples! { #(
			// Copy&paste `frame_support::hypothetically` since we cannot use macros here
			frame_support::storage::transactional::with_transaction(|| -> sp_runtime::TransactionOutcome<Result<_, sp_runtime::DispatchError>> {
				sp_runtime::TransactionOutcome::Rollback(Ok(Tuple::pre_check()))
			}).expect("Always returning Ok")
		),* })
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload) {
		(for_tuples! { #(
			// Copy&paste `frame_support::hypothetically` since we cannot use macros here
			frame_support::storage::transactional::with_transaction(|| -> sp_runtime::TransactionOutcome<Result<_, sp_runtime::DispatchError>> {
				sp_runtime::TransactionOutcome::Rollback(Ok(Tuple::post_check(rc_pre_payload.Tuple)))
			}).expect("Always returning Ok")
		),* });
	}
}

pub trait MigrationStatus {
	/// Whether the migration is finished.
	///
	/// This is **not** the same as `!self.is_ongoing()` since it may not have started.
	fn is_finished() -> bool;
	/// Whether the migration is ongoing.
	///
	/// This is **not** the same as `!self.is_finished()` since it may not have started.
	fn is_ongoing() -> bool;
}

/// A weight that is zero if the migration is ongoing, otherwise it is the default weight.
pub struct ZeroWeightOr<Status, Default>(PhantomData<(Status, Default)>);
impl<Status: MigrationStatus, Default: Get<Weight>> Get<Weight> for ZeroWeightOr<Status, Default> {
	fn get() -> Weight {
		Status::is_ongoing().then(Weight::zero).unwrap_or_else(Default::get)
	}
}
/// A utility struct for batching XCM messages to stay within size limits.
///
/// This struct manages collections of XCM messages, automatically creating
/// new batches when size limits would be exceeded, ensuring that all batches
/// remain within the maximum allowed XCM size.
pub struct XcmBatch<T: Encode> {
	/// Collection of batches with their sizes and messages
	sized_batches: VecDeque<(u32, Vec<T>)>,
}

impl<T: Encode> XcmBatch<T> {
	/// Creates a new empty batch.
	///
	/// # Returns
	/// A new XcmBatch instance with no messages.
	pub fn new() -> Self {
		Self { sized_batches: VecDeque::new() }
	}

	/// Pushes a message to the batch.
	///
	/// Adds the message to an existing batch if it fits within size limits,
	/// otherwise creates a new batch for the message. Messages that exceed
	/// the maximum XCM size will trigger a defensive assertion.
	///
	/// # Parameters
	/// - `message`: The message to add to the batch
	pub fn push(&mut self, message: T) {
		let message_size = message.encoded_size() as u32;
		if message_size > MAX_XCM_SIZE {
			defensive_assert!(true, "Message is too large to be added to the batch");
		}

		match self.sized_batches.back_mut() {
			Some((size, batch)) if *size + message_size <= MAX_XCM_SIZE => {
				*size += message_size;
				batch.push(message);
			},
			_ => {
				self.sized_batches.push_back((message_size, vec![message]));
			},
		}
	}

	/// Gets the total number of messages across all batches.
	///
	/// # Returns
	/// The total count of messages in all batches.
	pub fn len(&self) -> u32 {
		let mut total: u32 = 0;
		for (_, batch) in &self.sized_batches {
			total += batch.len() as u32;
		}
		total
	}

	/// Gets the number of batches.
	///
	/// # Returns
	/// The count of batches.
	pub fn batch_count(&self) -> u32 {
		self.sized_batches.len() as u32
	}

	/// Checks if the batch is empty.
	///
	/// # Returns
	/// `true` if there are no batches or if the only batch is empty, `false` otherwise.
	pub fn is_empty(&self) -> bool {
		self.sized_batches.is_empty() ||
			(self.sized_batches.len() == 1 &&
				self.sized_batches.front().is_none_or(|(_, batch)| batch.is_empty()))
	}

	/// Takes the first batch of messages.
	///
	/// # Returns
	/// The first batch of messages if available, or `None` if empty.
	pub fn pop_front(&mut self) -> Option<Vec<T>> {
		self.sized_batches.pop_front().map(|(_, batch)| batch)
	}
}

impl<T: Encode> Into<XcmBatch<T>> for XcmBatchAndMeter<T> {
	fn into(self) -> XcmBatch<T> {
		self.batch
	}
}

/// A wrapper around `XcmBatch` that tracks the weight consumed by batches.
///
/// This struct automatically accumulates weight for each new batch created
/// when messages are pushed, making it easier to track and consume weight
/// for batch processing operations.
pub struct XcmBatchAndMeter<T: Encode> {
	/// The underlying batch of XCM messages
	batch: XcmBatch<T>,
	/// The weight cost for processing a single batch
	batch_weight: Weight,
	/// The number of batches that have been accounted for in the accumulated weight
	tracked_batch_count: u32,
	/// The total accumulated weight for all tracked batches
	accumulated_weight: Weight,
}

impl<T: Encode> XcmBatchAndMeter<T> {
	/// Creates a new empty batch with the specified weight per batch.
	///
	/// # Parameters
	/// - `batch_weight`: The weight cost for processing a single batch
	pub fn new(batch_weight: Weight) -> Self {
		Self {
			batch: XcmBatch::new(),
			batch_weight,
			tracked_batch_count: 0,
			accumulated_weight: Weight::zero(),
		}
	}

	/// Creates a new empty batch with the weight from the pallet's configuration.
	///
	/// # Type Parameters
	/// - `C`: The pallet configuration that provides weight information
	pub fn new_from_config<C: Config>() -> Self {
		Self::new(C::RcWeightInfo::send_chunked_xcm_and_track())
	}
}

impl<T: Encode> XcmBatchAndMeter<T> {
	/// Pushes a message to the batch and updates the accumulated weight if a new batch is created.
	///
	/// # Parameters
	/// - `message`: The message to add to the batch
	pub fn push(&mut self, message: T) {
		self.batch.push(message);
		if self.batch.batch_count() > self.tracked_batch_count {
			self.accumulated_weight += self.batch_weight;
			self.tracked_batch_count = self.batch.batch_count();
		}
	}

	/// Consumes and returns the accumulated weight, resetting it to zero.
	///
	/// # Returns
	/// The total accumulated weight that was tracked
	pub fn consume_weight(&mut self) -> Weight {
		if self.accumulated_weight.is_zero() {
			return Weight::zero();
		}
		let weight = self.accumulated_weight;
		self.accumulated_weight = Weight::zero();
		weight
	}

	/// Consumes this wrapper and returns the inner `XcmBatch`.
	///
	/// # Returns
	/// The underlying batch of XCM messages
	pub fn into_inner(self) -> XcmBatch<T> {
		self.batch
	}

	/// Returns the total number of messages in all batches.
	///
	/// # Returns
	/// The count of all messages across all batches
	pub fn len(&self) -> u32 {
		self.batch.len()
	}

	/// Checks if the batch is empty.
	///
	/// # Returns
	/// `true` if there are no messages in any batches, `false` otherwise
	pub fn is_empty(&self) -> bool {
		self.batch.is_empty()
	}

	/// Returns the number of batches.
	///
	/// # Returns
	/// The count of batches
	pub fn batch_count(&self) -> u32 {
		self.batch.batch_count()
	}
}

#[cfg(test)]
mod xcm_batch_tests {
	use super::*;
	use codec::Encode;

	#[derive(Encode)]
	struct TestMessage(Vec<u8>);

	impl TestMessage {
		fn new(size: usize) -> Self {
			Self(vec![0; size])
		}
	}

	#[test]
	fn test_new_creates_empty_batch() {
		let batch: XcmBatch<TestMessage> = XcmBatch::new();
		assert!(batch.is_empty());
		assert_eq!(batch.len(), 0);
	}

	#[test]
	fn test_push_adds_message_to_batch() {
		let mut batch = XcmBatch::new();
		batch.push(TestMessage::new(10));
		assert!(!batch.is_empty());
		assert_eq!(batch.len(), 1);
	}

	#[test]
	fn test_push_creates_new_batch_when_exceeding_size() {
		let mut batch = XcmBatch::new();
		// First message goes into first batch
		batch.push(TestMessage::new(10));
		assert_eq!(batch.len(), 1);

		// Add messages until we exceed MAX_XCM_SIZE for the first batch
		let message_size = (MAX_XCM_SIZE / 2) as usize;
		batch.push(TestMessage::new(message_size));
		batch.push(TestMessage::new(message_size));

		// Should have created a second batch
		assert_eq!(batch.batch_count(), 2);
		assert_eq!(batch.len(), 3);
	}

	#[test]
	fn test_push_adds_to_existing_batch_when_size_permits() {
		let mut batch = XcmBatch::new();
		// Add small messages that should fit in one batch
		for _ in 0..5 {
			batch.push(TestMessage::new(10));
		}

		// Should still be in one batch
		assert_eq!(batch.batch_count(), 1);
		assert_eq!(batch.len(), 5);
	}

	#[test]
	fn test_len_counts_all_messages() {
		let mut batch = XcmBatch::new();

		// Add messages to multiple batches
		batch.push(TestMessage::new(10));
		batch.push(TestMessage::new((MAX_XCM_SIZE - 1) as usize));
		batch.push(TestMessage::new(10));

		assert_eq!(batch.len(), 3);
	}

	#[test]
	fn test_is_empty_with_empty_batch() {
		let batch: XcmBatch<TestMessage> = XcmBatch::new();
		assert!(batch.is_empty());
	}

	#[test]
	fn test_is_empty_with_non_empty_batch() {
		let mut batch = XcmBatch::new();
		batch.push(TestMessage::new(10));
		assert!(!batch.is_empty());
	}

	#[test]
	fn test_take_first_batch_returns_none_when_empty() {
		let mut batch: XcmBatch<TestMessage> = XcmBatch::new();
		assert!(batch.pop_front().is_none());
	}

	#[test]
	fn test_take_first_batch_returns_messages() {
		let mut batch = XcmBatch::new();
		batch.push(TestMessage::new(10));
		batch.push(TestMessage::new(20));

		// Create a second batch
		batch.push(TestMessage::new((MAX_XCM_SIZE - 10) as usize));

		// Take first batch
		let first_batch = batch.pop_front();
		assert!(first_batch.is_some());
		assert_eq!(first_batch.unwrap().len(), 2);

		// Should have one batch left
		assert_eq!(batch.batch_count(), 1);
		assert_eq!(batch.len(), 1);
	}

	#[test]
	fn test_take_first_batch_empties_batch() {
		let mut batch = XcmBatch::new();
		batch.push(TestMessage::new(10));

		let first_batch = batch.pop_front();
		assert!(first_batch.is_some());
		assert_eq!(first_batch.unwrap().len(), 1);

		// Should be empty now
		assert!(batch.is_empty());
		assert_eq!(batch.len(), 0);
	}
}

#[cfg(test)]
mod batch_and_meter_tests {
	use super::*;
	use codec::Encode;
	use frame_support::weights::Weight;

	#[derive(Encode)]
	struct TestMessage(Vec<u8>);

	impl TestMessage {
		fn new(size: usize) -> Self {
			Self(vec![0; size])
		}
	}

	#[test]
	fn test_new_creates_empty_batch_and_meter() {
		let batch_weight = Weight::from_parts(100, 5);
		let meter: XcmBatchAndMeter<TestMessage> = XcmBatchAndMeter::new(batch_weight);

		assert_eq!(meter.batch_weight, batch_weight);
		assert_eq!(meter.tracked_batch_count, 0);
		assert_eq!(meter.accumulated_weight, Weight::zero());
		assert!(meter.batch.is_empty());
	}

	#[test]
	fn test_push_tracks_weight_for_new_batches() {
		let batch_weight = Weight::from_parts(100, 5);
		let mut meter = XcmBatchAndMeter::new(batch_weight);

		// First message creates first batch but doesn't exceed tracked count
		meter.push(TestMessage::new(10));
		assert_eq!(meter.tracked_batch_count, 1);
		assert_eq!(meter.accumulated_weight, batch_weight);

		// Second message in same batch doesn't add weight
		meter.push(TestMessage::new(10));
		assert_eq!(meter.tracked_batch_count, 1);
		assert_eq!(meter.accumulated_weight, batch_weight);

		// Add message that creates a new batch
		let message_size = (MAX_XCM_SIZE / 2) as usize;
		meter.push(TestMessage::new(message_size));
		meter.push(TestMessage::new(message_size));

		// Should have tracked a second batch's weight
		assert_eq!(meter.tracked_batch_count, 2);
		assert_eq!(meter.accumulated_weight, batch_weight.saturating_mul(2));
	}

	#[test]
	fn test_consume_weight_returns_and_resets_accumulated_weight() {
		let batch_weight = Weight::from_parts(100, 5);
		let mut meter = XcmBatchAndMeter::new(batch_weight);

		// Add messages to create two batches
		meter.push(TestMessage::new(10));
		let message_size = (MAX_XCM_SIZE / 2) as usize;
		meter.push(TestMessage::new(message_size));
		meter.push(TestMessage::new(message_size));

		// Should have accumulated weight for two batches
		assert_eq!(meter.accumulated_weight, batch_weight.saturating_mul(2));

		// Consume weight should return accumulated weight and reset it
		let consumed = meter.consume_weight();
		assert_eq!(consumed, batch_weight.saturating_mul(2));
		assert_eq!(meter.accumulated_weight, Weight::zero());

		// Adding another batch should start accumulating from zero
		meter.push(TestMessage::new(1));
		meter.push(TestMessage::new(message_size));
		assert_eq!(meter.accumulated_weight, batch_weight);
	}

	#[test]
	fn test_into_inner_returns_batch() {
		let batch_weight = Weight::from_parts(100, 5);
		let mut meter = XcmBatchAndMeter::new(batch_weight);

		// Add a message to the batch
		meter.push(TestMessage::new(10));

		// Convert to inner batch
		let batch = meter.into_inner();
		assert_eq!(batch.len(), 1);
		assert!(!batch.is_empty());
	}

	#[test]
	fn test_delegated_methods() {
		let batch_weight = Weight::from_parts(100, 5);
		let mut meter = XcmBatchAndMeter::new(batch_weight);

		// Empty batch
		assert_eq!(meter.len(), 0);
		assert!(meter.is_empty());
		assert_eq!(meter.batch_count(), 0);

		// Add a message
		meter.push(TestMessage::new(10));
		assert_eq!(meter.len(), 1);
		assert!(!meter.is_empty());
		assert_eq!(meter.batch_count(), 1);

		// Add message that creates a new batch
		let message_size = (MAX_XCM_SIZE / 2) as usize;
		meter.push(TestMessage::new(message_size));
		meter.push(TestMessage::new(message_size));

		assert_eq!(meter.len(), 3);
		assert_eq!(meter.batch_count(), 2);
	}
}
