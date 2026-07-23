// This file is part of Substrate.

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

#![doc = include_str!("../docs/overview.md")]
#![doc = include_str!("../docs/usage.md")]
//! # Implementation notes
//!
//! This crate contains a disk-backed implementation of `sp_statement_store::StatementStore`.
//!
//! ## Constraint management
//!
//! The statement store validates statements using node-side signature verification and
//! static runtime allowance limits.
//! The following constraints are then checked:
//! * For a given account id, there may be at most `max_count` statements with `max_size` total data
//!   size. To satisfy this, statements for this account ID are removed from the store starting with
//!   the lowest priority until a constraint is satisfied.
//! * There may not be more than `MAX_TOTAL_STATEMENTS` total statements with `MAX_TOTAL_SIZE` size.
//!   To satisfy this, statements are removed from the store starting with the lowest
//!   `global_priority` until a constraint is satisfied.
//!
//! When a new statement is inserted that would not satisfy constraints in the first place, no
//! statements are deleted and a `Rejected` result is returned.
//! The order in which statements with the same priority are deleted is unspecified.
//!
//! ## Statement expiration
//!
//! Each time a statement is removed from the store (Either evicted by higher priority statement or
//! explicitly with the `remove` function) the statement is marked as expired. Expired statements
//! can't be added to the store for `Options::purge_after_sec` seconds. This is to prevent old
//! statements from being propagated on the network.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

mod metrics;
mod subscription;

#[cfg(feature = "test-helpers")]
pub mod subxt_client;
#[cfg(feature = "test-helpers")]
pub mod test_utils;

use crate::subscription::{SubscriptionStatementsStream, SubscriptionsHandle};
use futures::FutureExt;
use metrics::MetricsLink as PrometheusMetrics;
use parking_lot::{lock_api::RwLockUpgradableReadGuard, RwLock};
use prometheus_endpoint::Registry as PrometheusRegistry;
use sc_client_api::{backend::StorageProvider, Backend, StorageKey};
use sc_keystore::LocalKeystore;
use sp_blockchain::HeaderBackend;
use sp_core::{crypto::UncheckedFrom, hexdisplay::HexDisplay, traits::SpawnNamed, Decode, Encode};
use sp_runtime::traits::Block as BlockT;
use sp_statement_store::{
	runtime_api::{StatementSource, StatementStoreExt},
	AccountId, BlockHash, Channel, DecryptionKey, FilterDecision, Hash, InvalidReason,
	OptimizedTopicFilter, RejectionReason, Result, SignatureVerificationResult, Statement,
	StatementAllowance, StatementEvent, SubmitResult, Topic,
};
pub use sp_statement_store::{Error, StatementStore, MAX_TOPICS};
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	sync::{Arc, Weak},
	time::{Duration, Instant},
};
use subscription::ReplaySnapshotProvider;
pub use subscription::{
	AddFilterError, MultiFilterEventStream, MultiFilterSubscriptionApi,
	MultiFilterSubscriptionEvent, StatementStoreSubscriptionApi, SubscriptionHandle,
	MAX_FILTERS_PER_SUBSCRIPTION,
};

const KEY_VERSION: &[u8] = b"version".as_slice();
const CURRENT_VERSION: u32 = 2;

const LOG_TARGET: &str = "statement-store";

/// The amount of time an expired statement is kept before it is removed from the store entirely.
pub const DEFAULT_PURGE_AFTER_SEC: u64 = 2 * 24 * 60 * 60; // 48h
/// The maximum number of statements the statement store can hold.
pub const DEFAULT_MAX_TOTAL_STATEMENTS: usize = 4 * 1024 * 1024; // ~4 million
/// The maximum amount of data the statement store can hold, regardless of the number of
/// statements from which the data originates.
pub const DEFAULT_MAX_TOTAL_SIZE: usize = 2 * 1024 * 1024 * 1024; // 2GiB
/// The maximum size of a single statement in bytes.
/// Accounts for the 1-byte vector length prefix when statements are gossiped as `Vec<Statement>`.
pub const MAX_STATEMENT_SIZE: usize =
	sc_network_statement::config::MAX_STATEMENT_NOTIFICATION_SIZE as usize - 1;

/// Maximum number of statements to expire in a single iteration.
const MAX_EXPIRY_STATEMENTS_PER_ITERATION: usize = 10_000;
/// Maximum number of accounts to check for expiry in a single iteration.
const MAX_EXPIRY_ACCOUNTS_PER_ITERATION: usize = 10_000;
/// Maximum time in milliseconds to spend checking for expiry in a single iteration.
const MAX_EXPIRY_TIME_PER_ITERATION: Duration = Duration::from_millis(100);

/// Number of subscription filter worker tasks.
const NUM_FILTER_WORKERS: usize = 1;

const MAINTENANCE_PERIOD: std::time::Duration = std::time::Duration::from_secs(29);

/// Specifies which block hash to use when reading statement allowances.
enum AllowanceBlock {
	/// Use the best (latest) block hash.
	Best,
	/// Use the finalized block hash.
	Finalized,
}

// Period between enforcing limits (checking for expired statements and making sure statements stay
// within allowances). Different from maintenance period to avoid keeping the lock for too long for
// maintenance tasks.
const ENFORCE_LIMITS_PERIOD: std::time::Duration = std::time::Duration::from_secs(31);

mod col {
	pub const META: u8 = 0;
	pub const STATEMENTS: u8 = 1;
	pub const EXPIRED: u8 = 2;
	pub const INDEX_BY_TOPIC: u8 = 3;
	pub const INDEX_BY_DEC_KEY: u8 = 4;
	pub const INDEX_EVICTED: u8 = 5;

	pub const COUNT: u8 = 6;
}

const INDEX_EMPTY_VALUE: &[u8] = &[];
const DEC_KEY_TAG_NONE: u8 = 0;
const DEC_KEY_TAG_SOME: u8 = 1;

fn topic_index_key(topic: &Topic, hash: &Hash) -> Vec<u8> {
	let mut key = Vec::with_capacity(topic.len() + hash.len());
	key.extend_from_slice(&topic[..]);
	key.extend_from_slice(&hash[..]);
	key
}

fn dec_key_index_prefix(dec_key: &Option<DecryptionKey>) -> Vec<u8> {
	match dec_key {
		None => vec![DEC_KEY_TAG_NONE],
		Some(dec_key) => {
			let mut prefix = Vec::with_capacity(1 + dec_key.len());
			prefix.push(DEC_KEY_TAG_SOME);
			prefix.extend_from_slice(&dec_key[..]);
			prefix
		},
	}
}

fn dec_key_index_key(dec_key: &Option<DecryptionKey>, hash: &Hash) -> Vec<u8> {
	let mut key = dec_key_index_prefix(dec_key);
	key.extend_from_slice(&hash[..]);
	key
}

fn evicted_index_key(purge_at: u64, hash: &Hash) -> Vec<u8> {
	let mut key = Vec::with_capacity(8 + hash.len());
	key.extend_from_slice(&purge_at.to_be_bytes());
	key.extend_from_slice(&hash[..]);
	key
}

/// Extracts the trailing 32-byte hash from a composite index key, if it is long enough.
fn hash_from_index_key(key: &[u8]) -> Option<Hash> {
	let len = key.len();
	if len < 32 {
		return None;
	}
	let mut hash = Hash::default();
	hash.copy_from_slice(&key[len - 32..]);
	Some(hash)
}

/// Builds the index-column operations for a statement's topic and decryption-key entries. With
/// `insert == true` the entries are written; otherwise they are deleted. Designed to be folded
/// into the same atomic [`parity_db::Db::commit`] as the statement body.
fn statement_index_ops(
	hash: &Hash,
	statement: &Statement,
	insert: bool,
) -> Vec<(u8, Vec<u8>, Option<Vec<u8>>)> {
	let value = insert.then(|| INDEX_EMPTY_VALUE.to_vec());
	let mut ops = Vec::new();
	let mut nt = 0;
	while let Some(topic) = statement.topic(nt) {
		ops.push((col::INDEX_BY_TOPIC, topic_index_key(&topic, hash), value.clone()));
		nt += 1;
	}
	let dec_key = statement.decryption_key();
	ops.push((col::INDEX_BY_DEC_KEY, dec_key_index_key(&dec_key, hash), value));
	ops
}

#[derive(Eq, PartialEq, Debug, Ord, PartialOrd, Clone, Copy)]
struct Expiry(u64);

impl Expiry {
	/// Returns the expiration timestamp in seconds
	fn get_expiration_timestamp_secs(self) -> u64 {
		self.0 >> 32
	}
}

#[derive(PartialEq, Eq)]
struct PriorityKey {
	hash: Hash,
	expiry: Expiry,
}

impl PartialOrd for PriorityKey {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for PriorityKey {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.expiry.cmp(&other.expiry).then_with(|| self.hash.cmp(&other.hash))
	}
}

#[derive(PartialEq, Eq)]
struct ChannelEntry {
	hash: Hash,
	expiry: Expiry,
}

#[derive(Default)]
struct StatementsForAccount {
	// Statements ordered by priority.
	by_priority: BTreeMap<PriorityKey, (Option<Channel>, usize)>,
	// Channel to statement map. Only one statement per channel is allowed.
	channels: HashMap<Channel, ChannelEntry>,
	// Sum of all `Data` field sizes.
	data_size: usize,
}

impl StatementsForAccount {
	/// Returns an iterator over statements that have expired by `current_time`.
	fn expired_by_iter(
		&self,
		current_time: u64,
	) -> impl Iterator<Item = (&PriorityKey, &(Option<Channel>, usize))> {
		let range = PriorityKey { hash: Hash::default(), expiry: Expiry(0) }..PriorityKey {
			hash: Hash::default(),
			expiry: Expiry(current_time << 32),
		};
		self.by_priority.range(range)
	}
}

/// Default number of concurrent workers for statement validation.
pub const DEFAULT_NETWORK_WORKERS: usize = 1;

/// Default maximum statements per second per peer before rate limiting kicks in.
pub use sc_network_statement::config::DEFAULT_STATEMENTS_PER_SECOND as DEFAULT_RATE_LIMIT;

/// Statement store and network handler configuration.
#[derive(Debug, Clone, Copy)]
pub struct Config {
	/// Maximum statements allowed in the store. Once this limit is reached lower-priority
	/// statements may be evicted.
	pub max_total_statements: usize,
	/// Maximum total data size allowed in the store. Once this limit is reached lower-priority
	/// statements may be evicted.
	pub max_total_size: usize,
	/// Number of seconds for which removed statements won't be allowed to be added back in.
	pub purge_after_sec: u64,
	/// Number of concurrent workers for statement validation from the network.
	pub network_workers: usize,
	/// Maximum statements per second per peer before rate limiting kicks in.
	pub rate_limit: u32,
}

impl Config {
	/// Validate the configuration, returning an error if any values are invalid.
	pub fn validate(&self) -> Result<()> {
		if self.max_total_statements == 0 {
			return Err(Error::InvalidConfig(
				"max_total_statements must be greater than zero".into(),
			));
		}
		if self.max_total_size == 0 {
			return Err(Error::InvalidConfig("max_total_size must be greater than zero".into()));
		}
		if self.network_workers == 0 {
			return Err(Error::InvalidConfig("network_workers must be greater than zero".into()));
		}
		Ok(())
	}
}

impl Default for Config {
	fn default() -> Self {
		Config {
			max_total_statements: DEFAULT_MAX_TOTAL_STATEMENTS,
			max_total_size: DEFAULT_MAX_TOTAL_SIZE,
			purge_after_sec: DEFAULT_PURGE_AFTER_SEC,
			network_workers: DEFAULT_NETWORK_WORKERS,
			rate_limit: DEFAULT_RATE_LIMIT,
		}
	}
}

/// In-memory part of the read index.
struct QueryIndex {
	// TODO: Remove counters; replace them with a merge-join/leapfrog1
	topic_counts: HashMap<Topic, usize>,
	dec_key_counts: HashMap<Option<DecryptionKey>, usize>,
	recent: HashSet<Hash>,
}

impl QueryIndex {
	fn new() -> Self {
		QueryIndex {
			topic_counts: HashMap::new(),
			dec_key_counts: HashMap::new(),
			recent: HashSet::new(),
		}
	}

	/// Bumps cardinality counters for a statement seen at startup (no cache, not marked recent).
	fn note_initial(&mut self, statement: &Statement) {
		let mut nt = 0;
		while let Some(topic) = statement.topic(nt) {
			*self.topic_counts.entry(topic).or_insert(0) += 1;
			nt += 1;
		}
		*self.dec_key_counts.entry(statement.decryption_key()).or_insert(0) += 1;
	}

	/// Records a newly inserted statement: bumps cardinalities and marks the hash as recent.
	fn note_insert(&mut self, hash: Hash, statement: &Statement) {
		let mut nt = 0;
		while let Some(topic) = statement.topic(nt) {
			*self.topic_counts.entry(topic).or_insert(0) += 1;
			nt += 1;
		}
		let dec_key = statement.decryption_key();
		*self.dec_key_counts.entry(dec_key).or_insert(0) += 1;
		self.recent.insert(hash);
	}

	/// Records a removed statement: decrements cardinalities and drops the hash from `recent`.
	fn note_remove(&mut self, hash: &Hash, statement: &Statement) {
		let mut nt = 0;
		while let Some(topic) = statement.topic(nt) {
			if let Some(count) = self.topic_counts.get_mut(&topic) {
				*count = count.saturating_sub(1);
				if *count == 0 {
					self.topic_counts.remove(&topic);
				}
			}
			nt += 1;
		}
		let dec_key = statement.decryption_key();
		if let Some(count) = self.dec_key_counts.get_mut(&dec_key) {
			*count = count.saturating_sub(1);
			if *count == 0 {
				self.dec_key_counts.remove(&dec_key);
			}
		}
		self.recent.remove(hash);
	}

	/// Takes and clears the set of recently added hashes.
	fn take_recent(&mut self) -> HashSet<Hash> {
		std::mem::take(&mut self.recent)
	}
}

/// Index for submit operations (constraint checking, entries, accounts).
#[derive(Default)]
struct SubmitIndex {
	/// Statement hash → (account, expiry/priority, data size); the authoritative set of stored
	/// statements.
	entries: HashMap<Hash, (AccountId, Expiry, usize)>,
	/// Per-account tracking (priority-ordered hashes, channels, size) for quota enforcement.
	accounts: HashMap<AccountId, StatementsForAccount>,
	/// Accounts still pending an expiry/limit check by `enforce_limits`.
	accounts_to_check_for_expiry_stmts: Vec<AccountId>,
	/// Store configuration (global limits, purge period).
	config: Config,
	/// Running total of data size across all stored statements.
	total_size: usize,
	evicted_count: usize,
	// Monotonic sequence number assigned to each statement as it is inserted.
	next_seq: u64,
	// Sequence numbers of recently inserted statements, kept only while at least one subscription
	// snapshot scan is in progress and only back to the oldest such scan's watermark.
	recent_seqs: HashMap<Hash, u64>,
	// Watermarks of the currently running snapshot scans.
	active_scan_floors: BTreeMap<u64, usize>,
}

struct ClientWrapper<Block, Client, BE> {
	client: Arc<Client>,
	_block: std::marker::PhantomData<Block>,
	_backend: std::marker::PhantomData<BE>,
}

impl<Block, Client, BE> ClientWrapper<Block, Client, BE>
where
	Block: BlockT,
	Block::Hash: From<BlockHash>,
	BE: Backend<Block> + 'static,
	Client: HeaderBackend<Block> + StorageProvider<Block, BE> + Send + Sync + 'static,
{
	fn read_allowance(
		&self,
		account_id: &AccountId,
		allowance_block: AllowanceBlock,
	) -> Result<Option<StatementAllowance>> {
		use sp_statement_store::{statement_allowance_key, StatementAllowance};

		let block_hash = match allowance_block {
			AllowanceBlock::Best => self.client.info().best_hash,
			AllowanceBlock::Finalized => self.client.info().finalized_hash,
		};
		let key = statement_allowance_key(account_id);
		let storage_key = StorageKey(key);
		self.client
			.storage(block_hash, &storage_key)
			.map_err(|e| Error::Storage(format!("Failed to read allowance: {:?}", e)))?
			.map(|value| {
				StatementAllowance::decode(&mut &value.0[..])
					.map_err(|e| Error::Decode(format!("Failed to decode allowance: {:?}", e)))
			})
			.transpose()
	}
}

/// Statement store.
pub struct Store {
	db: parity_db::Db,
	submit_index: RwLock<SubmitIndex>,
	query_index: RwLock<QueryIndex>,
	read_allowance_fn:
		Box<dyn Fn(&AccountId, AllowanceBlock) -> Result<Option<StatementAllowance>> + Send + Sync>,
	subscription_manager: SubscriptionsHandle,
	keystore: Arc<LocalKeystore>,
	// Used for testing
	time_override: Option<u64>,
	metrics: PrometheusMetrics,
}

/// Outcome of [`SubmitIndex::make_expired`].
enum Eviction {
	/// The statement was removed; it had already reached its natural expiry, so it is not banned
	/// from re-acceptance.
	Removed,
	/// The statement was removed and is banned from re-acceptance until this timestamp.
	Banned(u64),
}

impl ReplaySnapshotProvider for Weak<Store> {
	fn with_snapshot_hashes(
		&self,
		filter: &OptimizedTopicFilter,
		enqueue: &mut dyn FnMut(Vec<Hash>),
	) -> Result<()> {
		let Some(store) = self.upgrade() else {
			return Err(Error::InvalidConfig("statement store is closed".into()));
		};
		store.with_snapshot_hashes(filter, enqueue)
	}

	fn statement_by_hash(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
		let Some(store) = self.upgrade() else {
			return Err(Error::InvalidConfig("statement store is closed".into()));
		};
		store.statement_by_hash(hash)
	}
}

/// What [`SubmitIndex::insert`] evicted to make room for a new statement.
struct InsertOutcome {
	/// All hashes removed from the index. Their bodies must be deleted and their read-index
	/// entries cleared.
	evicted: HashSet<Hash>,
	/// The subset of `evicted` that is banned from re-acceptance, with its purge deadline. These
	/// must be recorded in the on-disk evicted journal.
	banned: Vec<(Hash, u64)>,
}

/// A single on-disk index set referenced during a query: either the set of hashes carrying a
/// topic, or the set of hashes for a decryption key.
#[derive(Clone)]
enum IndexSet {
	Topic(Topic),
	DecKey(Option<DecryptionKey>),
}

impl IndexSet {
	fn column(&self) -> u8 {
		match self {
			IndexSet::Topic(_) => col::INDEX_BY_TOPIC,
			IndexSet::DecKey(_) => col::INDEX_BY_DEC_KEY,
		}
	}

	/// Prefix selecting every entry of this set within its column.
	fn prefix(&self) -> Vec<u8> {
		match self {
			IndexSet::Topic(t) => t[..].to_vec(),
			IndexSet::DecKey(k) => dec_key_index_prefix(k),
		}
	}

	/// Full key of `hash` within this set's column, for point membership lookups.
	fn member_key(&self, hash: &Hash) -> Vec<u8> {
		match self {
			IndexSet::Topic(t) => topic_index_key(t, hash),
			IndexSet::DecKey(k) => dec_key_index_key(k, hash),
		}
	}

	/// Cardinality of this set, read from the in-memory cardinality counters.
	fn len(&self, read_index: &QueryIndex) -> usize {
		match self {
			IndexSet::Topic(t) => read_index.topic_counts.get(t).copied().unwrap_or(0),
			IndexSet::DecKey(k) => read_index.dec_key_counts.get(k).copied().unwrap_or(0),
		}
	}
}

impl SubmitIndex {
	fn new(config: Config) -> SubmitIndex {
		SubmitIndex { config, ..Default::default() }
	}

	/// Assigns and returns the next store sequence number for `hash`, recording it in `recent_seqs`
	/// while a snapshot scan is active.
	fn note_seq(&mut self, hash: Hash) -> u64 {
		let seq = self.next_seq;
		self.next_seq += 1;
		if !self.active_scan_floors.is_empty() {
			self.recent_seqs.insert(hash, seq);
		}
		seq
	}

	/// Registers a subscription snapshot scan and returns its watermark.
	fn begin_scan(&mut self) -> u64 {
		let watermark = self.next_seq;
		*self.active_scan_floors.entry(watermark).or_insert(0) += 1;
		watermark
	}

	/// Deregisters a snapshot scan previously registered with [`Self::begin_scan`] and prunes
	/// `recent_seqs` down to the smallest still-active watermark.
	fn end_scan(&mut self, watermark: u64) {
		if let Some(count) = self.active_scan_floors.get_mut(&watermark) {
			*count -= 1;
			if *count == 0 {
				self.active_scan_floors.remove(&watermark);
			}
		}
		match self.active_scan_floors.keys().next() {
			Some(&floor) => self.recent_seqs.retain(|_, seq| *seq >= floor),
			None => self.recent_seqs.clear(),
		}
	}

	/// Whether the statement `hash` belongs in the snapshot of a scan with the given `watermark`.
	fn seq_covered_by_snapshot(&self, hash: &Hash, watermark: u64) -> bool {
		match self.recent_seqs.get(hash) {
			Some(&seq) => seq < watermark,
			None => true,
		}
	}

	fn insert_new(&mut self, hash: Hash, account: AccountId, statement: &Statement) {
		let expiry = Expiry(statement.expiry());
		self.entries.insert(hash, (account, expiry, statement.data_len()));
		self.total_size += statement.data_len();
		let account_info = self.accounts.entry(account).or_default();
		account_info.data_size += statement.data_len();
		if let Some(channel) = statement.channel() {
			account_info.channels.insert(channel, ChannelEntry { hash, expiry });
		}
		account_info
			.by_priority
			.insert(PriorityKey { hash, expiry }, (statement.channel(), statement.data_len()));
	}

	fn make_expired(&mut self, hash: &Hash, current_time: u64) -> Option<Eviction> {
		if let Some((account, expiry, len)) = self.entries.remove(hash) {
			self.total_size -= len;
			let eviction = if current_time < expiry.get_expiration_timestamp_secs() {
				let purge_at = expiry
					.get_expiration_timestamp_secs()
					.min(current_time.saturating_add(self.config.purge_after_sec));
				self.evicted_count += 1;
				Eviction::Banned(purge_at)
			} else {
				Eviction::Removed
			};
			if let std::collections::hash_map::Entry::Occupied(mut account_rec) =
				self.accounts.entry(account)
			{
				let key = PriorityKey { hash: *hash, expiry };
				if let Some((channel, len)) = account_rec.get_mut().by_priority.remove(&key) {
					account_rec.get_mut().data_size -= len;
					if let Some(channel) = channel {
						account_rec.get_mut().channels.remove(&channel);
					}
				}
				if account_rec.get().by_priority.is_empty() {
					account_rec.remove_entry();
				}
			}
			log::trace!(target: LOG_TARGET, "Expired statement {:?}", HexDisplay::from(hash));
			Some(eviction)
		} else {
			None
		}
	}

	fn insert(
		&mut self,
		hash: Hash,
		statement: &Statement,
		account: &AccountId,
		validation: &StatementAllowance,
		current_time: u64,
	) -> std::result::Result<InsertOutcome, RejectionReason> {
		let statement_len = statement.data_len();
		if statement_len > validation.max_size as usize {
			log::debug!(
				target: LOG_TARGET,
				"Ignored oversize message from account {}: {:?} ({} bytes)",
				HexDisplay::from(account),
				HexDisplay::from(&hash),
				statement_len,
			);
			return Err(RejectionReason::DataTooLarge {
				submitted_size: statement_len,
				available_size: validation.max_size as usize,
			});
		}

		let mut evicted = HashSet::new();
		let mut would_free_size = 0;
		let expiry = Expiry(statement.expiry());
		let (max_size, max_count) = (validation.max_size as usize, validation.max_count as usize);
		// It may happen that we can't delete enough lower priority messages
		// to satisfy size constraints. We check for that before deleting anything,
		// taking into account channel message replacement.
		if let Some(account_rec) = self.accounts.get(account) {
			if let Some(channel) = statement.channel() {
				if let Some(channel_record) = account_rec.channels.get(&channel) {
					if expiry <= channel_record.expiry {
						// Trying to replace channel message with lower expiry.
						log::debug!(
							target: LOG_TARGET,
							"Ignored lower priority channel message from account {}: {:?} {:?} <= {:?}",
							HexDisplay::from(account),
							HexDisplay::from(&hash),
							expiry,
							channel_record.expiry,
						);
						return Err(RejectionReason::ChannelPriorityTooLow {
							submitted_expiry: expiry.0,
							min_expiry: channel_record.expiry.0,
						});
					} else {
						// Would replace channel message. Still need to check for size constraints
						// below.
						log::debug!(
							target: LOG_TARGET,
							"Replacing higher priority channel message from account {}: {:?} ({:?}) > {:?} ({:?})",
							HexDisplay::from(account),
							HexDisplay::from(&hash),
							expiry,
							HexDisplay::from(&channel_record.hash),
							channel_record.expiry,
						);
						let key = PriorityKey {
							hash: channel_record.hash,
							expiry: channel_record.expiry,
						};
						if let Some((_channel, len)) = account_rec.by_priority.get(&key) {
							would_free_size += *len;
							evicted.insert(channel_record.hash);
						}
					}
				}
			}
			// Check if we can evict enough lower priority statements to satisfy constraints
			for (entry, (_, len)) in account_rec.by_priority.iter() {
				if (account_rec.data_size - would_free_size + statement_len <= max_size) &&
					account_rec.by_priority.len() + 1 - evicted.len() <= max_count
				{
					// Satisfied
					break;
				}
				if evicted.contains(&entry.hash) {
					// Already accounted for above
					continue;
				}
				if entry.expiry >= expiry {
					log::debug!(
						target: LOG_TARGET,
						"Ignored message from account {} due to constraints {:?} {:?} < {:?}",
						HexDisplay::from(account),
						HexDisplay::from(&hash),
						expiry,
						entry.expiry,
					);
					return Err(RejectionReason::AccountFull {
						submitted_expiry: expiry.0,
						min_expiry: entry.expiry.0,
					});
				}
				evicted.insert(entry.hash);
				would_free_size += len;
			}
		}
		// Now check global constraints as well.
		if !((self.total_size - would_free_size + statement_len <= self.config.max_total_size) &&
			self.entries.len() + 1 - evicted.len() <= self.config.max_total_statements)
		{
			log::debug!(
				target: LOG_TARGET,
				"Ignored statement {} from account {} because the store is full (size={}, count={})",
				HexDisplay::from(&hash),
				HexDisplay::from(account),
				self.total_size,
				self.entries.len(),
			);
			return Err(RejectionReason::StoreFull);
		}

		let mut banned = Vec::new();
		for h in &evicted {
			if let Some(Eviction::Banned(purge_at)) = self.make_expired(h, current_time) {
				banned.push((*h, purge_at));
			}
		}
		self.insert_new(hash, *account, statement);
		Ok(InsertOutcome { evicted, banned })
	}
}

impl Store {
	/// Create a new shared store instance. There should only be one per process.
	/// `path` will be used to open a statement database or create a new one if it does not exist.
	pub fn new_shared<Block, Client, BE>(
		path: &std::path::Path,
		config: Config,
		client: Arc<Client>,
		keystore: Arc<LocalKeystore>,
		prometheus: Option<&PrometheusRegistry>,
		task_spawner: Box<dyn SpawnNamed>,
	) -> Result<Arc<Store>>
	where
		Block: BlockT,
		Block::Hash: From<BlockHash>,
		BE: Backend<Block> + 'static,
		Client: HeaderBackend<Block> + StorageProvider<Block, BE> + Send + Sync + 'static,
	{
		let store =
			Arc::new(Self::new(path, config, client, keystore, prometheus, task_spawner.clone())?);

		// Perform periodic statement store maintenance
		let worker_store = store.clone();
		task_spawner.spawn(
			"statement-store-maintenance",
			Some("statement-store"),
			Box::pin(async move {
				let mut maintenance_interval = tokio::time::interval(MAINTENANCE_PERIOD);
				let mut enforce_limits_interval = tokio::time::interval(ENFORCE_LIMITS_PERIOD);
				loop {
					futures::select! {
						_ = maintenance_interval.tick().fuse() => {worker_store.maintain();}
						_ = enforce_limits_interval.tick().fuse() => {worker_store.enforce_limits();}
					}
				}
			}),
		);

		Ok(store)
	}

	/// Create a new instance.
	/// `path` will be used to open a statement database or create a new one if it does not exist.
	#[doc(hidden)]
	pub fn new<Block, Client, BE>(
		path: &std::path::Path,
		config: Config,
		client: Arc<Client>,
		keystore: Arc<LocalKeystore>,
		prometheus: Option<&PrometheusRegistry>,
		task_spawner: Box<dyn SpawnNamed>,
	) -> Result<Store>
	where
		Block: BlockT,
		Block::Hash: From<BlockHash>,
		BE: Backend<Block> + 'static,
		Client: HeaderBackend<Block> + StorageProvider<Block, BE> + Send + Sync + 'static,
	{
		config.validate()?;

		let mut path: std::path::PathBuf = path.into();
		path.push("statements");

		Self::migrate_columns(&path)?;
		let db = Self::open_db(&path)?;
		let needs_index_migration = Self::check_db_version(&db)?;

		let storage_reader =
			ClientWrapper { client, _block: Default::default(), _backend: Default::default() };
		let read_allowance_fn =
			Box::new(move |account_id: &AccountId, allowance_block: AllowanceBlock| {
				storage_reader.read_allowance(account_id, allowance_block)
			});

		let store = Store {
			db,
			submit_index: RwLock::new(SubmitIndex::new(config)),
			query_index: RwLock::new(QueryIndex::new()),
			read_allowance_fn,
			keystore,
			time_override: None,
			metrics: PrometheusMetrics::new(prometheus),
			subscription_manager: SubscriptionsHandle::new(
				task_spawner.clone(),
				NUM_FILTER_WORKERS,
			),
		};
		store.populate(needs_index_migration)?;
		Ok(store)
	}

	/// Migrate the column layout of an existing database to the current schema.
	fn migrate_columns(path: &std::path::Path) -> Result<()> {
		let Some(metadata) =
			parity_db::Options::load_metadata(path).map_err(|e| Error::Db(e.to_string()))?
		else {
			return Ok(());
		};
		if metadata.columns.len() >= col::COUNT as usize {
			return Ok(());
		}
		let mut migrate_config = parity_db::Options::with_columns(path, 0);
		migrate_config.columns = metadata.columns;
		while migrate_config.columns.len() < col::COUNT as usize {
			// `add_column` takes the options by value, so build a fresh one each iteration.
			let mut new_column_options = parity_db::ColumnOptions::default();
			new_column_options.btree_index = true;
			parity_db::Db::add_column(&mut migrate_config, new_column_options)
				.map_err(|e| Error::Db(e.to_string()))?;
		}
		Ok(())
	}

	/// Open (or create) the statement database with the column options expected by the current
	/// schema.
	fn open_db(path: &std::path::Path) -> Result<parity_db::Db> {
		let mut db_config = parity_db::Options::with_columns(path, col::COUNT);
		let statement_col = &mut db_config.columns[col::STATEMENTS as usize];
		statement_col.ref_counted = false;
		statement_col.preimage = true;
		statement_col.uniform = true;
		for c in [col::INDEX_BY_TOPIC, col::INDEX_BY_DEC_KEY, col::INDEX_EVICTED] {
			db_config.columns[c as usize].btree_index = true;
		}
		parity_db::Db::open_or_create(&db_config).map_err(|e| Error::Db(e.to_string()))
	}

	/// Read the on-disk database version and reconcile it with [`CURRENT_VERSION`].
	///
	/// A brand new database has its version initialised and needs no migration. An existing
	/// database from a newer version is rejected. Returns `true` if the on-disk indexes predate
	/// the current version and therefore need to be rebuilt.
	fn check_db_version(db: &parity_db::Db) -> Result<bool> {
		match db.get(col::META, &KEY_VERSION).map_err(|e| Error::Db(e.to_string()))? {
			Some(version) => {
				let version = u32::from_le_bytes(
					version
						.try_into()
						.map_err(|_| Error::Db("Error reading database version".into()))?,
				);
				if version > CURRENT_VERSION {
					return Err(Error::Db(format!("Unsupported database version: {version}")));
				}
				Ok(version < CURRENT_VERSION)
			},
			None => {
				// Brand new database: the index columns start empty, nothing to migrate.
				db.commit([(
					col::META,
					KEY_VERSION.to_vec(),
					Some(CURRENT_VERSION.to_le_bytes().to_vec()),
				)])
				.map_err(|e| Error::Db(e.to_string()))?;
				Ok(false)
			},
		}
	}

	/// Create memory index from the data.
	// This may be moved to a background thread if it slows startup too much.
	// This function should only be used on startup. There should be no other DB operations when
	// iterating the index.
	fn populate(&self, migrate_index: bool) -> Result<()> {
		// Holding both locks here is fine: this runs at startup before any statements are
		// processed, so there is no contention.
		let migration_ops = {
			let mut submit_index = self.submit_index.write();
			let mut query_index = self.query_index.write();
			let mut migration_ops: Vec<(u8, Vec<u8>, Option<Vec<u8>>)> = Vec::new();
			self.db
				.iter_column_while(col::STATEMENTS, |item| {
					let statement = item.value;
					if let Ok(statement) = Statement::decode(&mut statement.as_slice()) {
						let hash = statement.hash();
						log::trace!(
							target: LOG_TARGET,
							"Statement loaded {:?}",
							HexDisplay::from(&hash)
						);
						if let Some(account_id) = statement.account_id() {
							submit_index.insert_new(hash, account_id, &statement);
							query_index.note_initial(&statement);
							if migrate_index {
								migration_ops.extend(statement_index_ops(&hash, &statement, true));
							}
						} else {
							log::debug!(
								target: LOG_TARGET,
								"Error decoding statement loaded from the DB: {:?}",
								HexDisplay::from(&hash)
							);
						}
					}
					true
				})
				.map_err(|e| Error::Db(e.to_string()))?;
			let mut evicted_count = 0usize;
			self.db
				.iter_column_while(col::EXPIRED, |item| {
					let expired_info = item.value;
					if let Ok((hash, timestamp)) =
						<(Hash, u64)>::decode(&mut expired_info.as_slice())
					{
						log::trace!(
							target: LOG_TARGET,
							"Statement loaded (expired): {:?}",
							HexDisplay::from(&hash)
						);
						evicted_count += 1;
						if migrate_index {
							let purge_at =
								timestamp.saturating_add(submit_index.config.purge_after_sec);
							migration_ops.push((
								col::INDEX_EVICTED,
								evicted_index_key(purge_at, &hash),
								Some(INDEX_EMPTY_VALUE.to_vec()),
							));
						}
					}
					true
				})
				.map_err(|e| Error::Db(e.to_string()))?;
			submit_index.evicted_count = evicted_count;
			migration_ops
		};

		if migrate_index {
			// Commit the rebuilt index in bounded chunks, then mark the database as migrated.
			const MIGRATION_CHUNK: usize = 100_000;
			let total = migration_ops.len();
			for chunk in migration_ops.chunks(MIGRATION_CHUNK) {
				self.db.commit(chunk.iter().cloned()).map_err(|e| Error::Db(e.to_string()))?;
			}
			self.db
				.commit([(
					col::META,
					KEY_VERSION.to_vec(),
					Some(CURRENT_VERSION.to_le_bytes().to_vec()),
				)])
				.map_err(|e| Error::Db(e.to_string()))?;
			log::info!(
				target: LOG_TARGET,
				"Migrated statement store read index to the on-disk format ({} entries)",
				total
			);
		}

		self.maintain();
		Ok(())
	}

	/// Scans an on-disk btree index column for every hash whose key starts with `prefix`.
	fn scan_index_prefix(&self, column: u8, prefix: &[u8]) -> Result<HashSet<Hash>> {
		let mut set = HashSet::new();
		let mut iter = self.db.iter(column).map_err(|e| Error::Db(e.to_string()))?;
		iter.seek(prefix).map_err(|e| Error::Db(e.to_string()))?;
		while let Some((key, _)) = iter.next().map_err(|e| Error::Db(e.to_string()))? {
			if !key.starts_with(prefix) {
				break;
			}
			if let Some(hash) = hash_from_index_key(&key) {
				set.insert(hash);
			}
		}
		Ok(set)
	}

	/// Enumerates the hashes of all active statements. Each statement has exactly one entry in
	/// [`col::INDEX_BY_DEC_KEY`], so scanning that column's keys yields every hash exactly once,
	/// without decoding any bodies.
	fn enumerate_hashes(&self) -> Result<Vec<Hash>> {
		let mut hashes = Vec::new();
		let mut iter = self.db.iter(col::INDEX_BY_DEC_KEY).map_err(|e| Error::Db(e.to_string()))?;
		iter.seek_to_first().map_err(|e| Error::Db(e.to_string()))?;
		while let Some((key, _)) = iter.next().map_err(|e| Error::Db(e.to_string()))? {
			if let Some(hash) = hash_from_index_key(&key) {
				hashes.push(hash);
			}
		}
		Ok(hashes)
	}

	/// Tests, against the on-disk column, whether `hash` belongs to an index set.
	fn index_set_contains(&self, set: &IndexSet, hash: &Hash) -> Result<bool> {
		Ok(self
			.db
			.get_size(set.column(), &set.member_key(hash))
			.map_err(|e| Error::Db(e.to_string()))?
			.is_some())
	}

	/// Whether `hash` is present in every one of `sets`, tested against the on-disk index. Used to
	/// intersect a materialised candidate set with the remaining topic / decryption-key sets.
	fn hash_in_all_sets(&self, hash: &Hash, sets: &[IndexSet]) -> Result<bool> {
		sets.iter().try_fold(true, |acc, set| {
			Ok::<bool, Error>(acc && self.index_set_contains(set, hash)?)
		})
	}

	/// Enumerates matching hashes for `key` / `topic`, reading candidates directly from the on-disk
	/// index for every filter kind. Used both for ad-hoc reads and for subscription snapshots.
	fn iterate_with(
		&self,
		key: Option<DecryptionKey>,
		topic_filter: &OptimizedTopicFilter,
		f: impl FnMut(&Hash) -> Result<()>,
	) -> Result<()> {
		match topic_filter {
			OptimizedTopicFilter::Any => self.iterate_with_any(key, f),
			OptimizedTopicFilter::MatchAll(topics) => self.iterate_with_match_all(key, topics, f),
			OptimizedTopicFilter::MatchAny(topics) => self.iterate_with_match_any(key, topics, f),
		}
	}

	/// Streams every hash for a decryption key directly from disk.
	fn iterate_with_any(
		&self,
		key: Option<DecryptionKey>,
		mut f: impl FnMut(&Hash) -> Result<()>,
	) -> Result<()> {
		let prefix = IndexSet::DecKey(key).prefix();
		let mut iter = self.db.iter(col::INDEX_BY_DEC_KEY).map_err(|e| Error::Db(e.to_string()))?;
		iter.seek(&prefix).map_err(|e| Error::Db(e.to_string()))?;
		while let Some((k, _)) = iter.next().map_err(|e| Error::Db(e.to_string()))? {
			if !k.starts_with(&prefix) {
				break;
			}
			if let Some(hash) = hash_from_index_key(&k) {
				f(&hash)?;
			}
		}
		Ok(())
	}

	/// For each requested topic, streams its hashes from disk and yields those that also belong to
	/// the decryption-key set. A hash carrying several requested topics is yielded once per topic,
	/// matching the in-memory behaviour callers already tolerate.
	fn iterate_with_match_any(
		&self,
		key: Option<DecryptionKey>,
		topics: &HashSet<Topic>,
		mut f: impl FnMut(&Hash) -> Result<()>,
	) -> Result<()> {
		let key_set = IndexSet::DecKey(key);
		for topic in topics {
			let prefix = topic[..].to_vec();
			let mut iter =
				self.db.iter(col::INDEX_BY_TOPIC).map_err(|e| Error::Db(e.to_string()))?;
			iter.seek(&prefix).map_err(|e| Error::Db(e.to_string()))?;
			while let Some((k, _)) = iter.next().map_err(|e| Error::Db(e.to_string()))? {
				if !k.starts_with(&prefix) {
					break;
				}
				if let Some(hash) = hash_from_index_key(&k) {
					if self.index_set_contains(&key_set, &hash)? {
						f(&hash)?;
					}
				}
			}
		}
		Ok(())
	}

	/// Intersects the decryption-key set with all requested topic sets, reading candidates directly
	/// from the on-disk index. The lock is taken only briefly to order the sets by cardinality so
	/// the smallest is materialised first; this ordering is a best-effort hint (stale counters can
	/// only misorder the sets, never drop a statement). Materialising that set and probing the rest
	/// against disk then happen without holding the lock.
	fn iterate_with_match_all(
		&self,
		key: Option<DecryptionKey>,
		topics: &HashSet<Topic>,
		mut f: impl FnMut(&Hash) -> Result<()>,
	) -> Result<()> {
		if topics.len() > MAX_TOPICS {
			return Ok(());
		}
		let mut sets = Vec::with_capacity(topics.len() + 1);
		sets.push(IndexSet::DecKey(key));
		for topic in topics {
			sets.push(IndexSet::Topic(*topic));
		}
		{
			// Ordering only (best-effort): stale counters can misorder but never drop a statement.
			let query_index = self.query_index.read();
			sets.sort_by_key(|s| s.len(&query_index));
		}
		let smallest = self.scan_index_prefix(sets[0].column(), &sets[0].prefix())?;
		let others = &sets[1..];
		for hash in &smallest {
			if self.hash_in_all_sets(hash, others)? {
				log::trace!(
					target: LOG_TARGET,
					"Iterating by topic/key: statement {:?}",
					HexDisplay::from(hash)
				);
				f(hash)?;
			}
		}
		Ok(())
	}

	/// Reads the raw SCALE-encoded body of `hash` from `col::STATEMENTS`, or `None` if it is absent
	/// (a benign DB race: the statement was removed concurrently). The stored value is exactly
	/// `statement.encode()`, so it can be forwarded verbatim.
	fn read_statement_encoded(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
		match self.db.get(col::STATEMENTS, hash).map_err(|e| Error::Db(e.to_string()))? {
			Some(entry) => Ok(Some(entry)),
			None => {
				log::debug!(target: LOG_TARGET, "Missing statement {:?}", HexDisplay::from(hash));
				Ok(None)
			},
		}
	}

	/// Reads and decodes the statement `hash`, returning `None` if it is absent or its stored body
	/// fails to decode (a corrupt DB row, which is logged and skipped).
	fn read_statement(&self, hash: &Hash) -> Result<Option<Statement>> {
		let Some(entry) = self.read_statement_encoded(hash)? else { return Ok(None) };
		match Statement::decode(&mut entry.as_slice()) {
			Ok(statement) => Ok(Some(statement)),
			Err(_) => {
				log::warn!(target: LOG_TARGET, "Corrupt statement {:?}", HexDisplay::from(hash));
				Ok(None)
			},
		}
	}

	/// Collects statements matching `key` / `topic_filter`. Reads never hold the query-index lock
	/// across disk I/O: `Any` / `MatchAny` touch only the (thread-safe) database, and `MatchAll`
	/// takes the lock only momentarily to order the candidate sets.
	fn collect_statements<R>(
		&self,
		key: Option<DecryptionKey>,
		topic_filter: &OptimizedTopicFilter,
		mut f: impl FnMut(Statement) -> Option<R>,
	) -> Result<Vec<R>> {
		let mut result = Vec::new();
		self.iterate_with(key, topic_filter, |hash| {
			if let Some(statement) = self.read_statement(hash)? {
				if let Some(data) = f(statement) {
					result.push(data);
				}
			}
			Ok(())
		})?;
		Ok(result)
	}

	// Collects expired and over-allowance statement hashes for a single account.
	fn collect_evictions(
		&self,
		account: &AccountId,
		account_rec: &StatementsForAccount,
		current_time: u64,
	) -> Vec<Hash> {
		let mut to_evict = Vec::new();
		let mut expired_count = 0usize;
		let mut expired_size = 0usize;
		for (key, (_, len)) in account_rec.expired_by_iter(current_time) {
			to_evict.push(key.hash);
			expired_count += 1;
			expired_size += len;
		}

		// Enforce allowances for remaining (non-expired) statements, we use the finalized block to
		// make sure we enforce allowances based on the correct chain state.
		let allowance = match (self.read_allowance_fn)(account, AllowanceBlock::Finalized) {
			Ok(Some(allowance)) => allowance,
			Ok(None) => {
				log::debug!(
					target: LOG_TARGET,
					"No allowance found for account {:?}, treating as zero allowance",
					HexDisplay::from(account)
				);
				StatementAllowance { max_count: 0, max_size: 0 }
			},
			Err(e) => {
				log::error!(target: LOG_TARGET, "Error reading allowance: {:?}", e);
				// Skip allowance enforcement for this account on error
				return to_evict;
			},
		};

		// Calculate remaining count and size after expiring statements
		let mut remaining_count = account_rec.by_priority.len() - expired_count;
		let mut remaining_size = account_rec.data_size - expired_size;

		// Evict lowest priority statements that exceed allowance
		if remaining_count > allowance.max_count as usize ||
			remaining_size > allowance.max_size as usize
		{
			log::debug!(
				target: LOG_TARGET,
				"Account {:?} exceeds allowance: count={}/{}, size={}/{}",
				HexDisplay::from(account),
				remaining_count,
				allowance.max_count,
				remaining_size,
				allowance.max_size
			);

			// Skip expired statements (they're at the beginning due to BTreeMap ordering)
			for (key, (_, len)) in account_rec.by_priority.iter().skip(expired_count) {
				if remaining_count <= allowance.max_count as usize &&
					remaining_size <= allowance.max_size as usize
				{
					break;
				}
				to_evict.push(key.hash);
				remaining_count -= 1;
				remaining_size -= len;
				log::debug!(
					target: LOG_TARGET,
					"Evicting statement {:?} due to allowance enforcement",
					HexDisplay::from(&key.hash)
				);
			}
		}

		to_evict
	}

	// Checks for expired statements and enforces allowances, marking violating statements
	// as expired in the index.
	//
	// This function performs incremental checking to avoid blocking the store for too long.
	// It processes accounts in batches and stops when any of these limits are reached:
	// - `MAX_EXPIRY_STATEMENTS_PER_ITERATION` statements found to expire/evict
	// - `MAX_EXPIRY_ACCOUNTS_PER_ITERATION` accounts checked
	// - `MAX_EXPIRY_TIME_MS_PER_ITERATION` milliseconds elapsed
	//
	// The function maintains a list of accounts to check (`accounts_to_check_for_expiry_stmts`).
	// When this list is empty, it repopulates it with all current accounts and returns early,
	// deferring the actual check to the next call. This ensures the process eventually covers
	// all accounts across multiple invocations.
	//
	// Statements are considered expired when their priority (which encodes the expiration
	// timestamp in the upper 32 bits) is less than the current timestamp.
	fn enforce_limits(&self) {
		let _start_check_expiration_timer = self.metrics.start_check_expiration_timer();
		let current_time = self.timestamp();

		let (to_evict, num_accounts_checked) = {
			let submit_index = self.submit_index.upgradable_read();
			if submit_index.accounts_to_check_for_expiry_stmts.is_empty() {
				let existing_accounts = submit_index.accounts.keys().cloned().collect::<Vec<_>>();
				let mut submit_index = RwLockUpgradableReadGuard::upgrade(submit_index);
				submit_index.accounts_to_check_for_expiry_stmts = existing_accounts;
				return;
			}

			let mut to_evict = Vec::new();
			let mut num_accounts_checked = 0;
			let start = Instant::now();

			for account in submit_index.accounts_to_check_for_expiry_stmts.iter().rev() {
				num_accounts_checked += 1;
				if let Some(account_rec) = submit_index.accounts.get(account) {
					to_evict.extend(self.collect_evictions(account, account_rec, current_time));
				}

				if to_evict.len() >= MAX_EXPIRY_STATEMENTS_PER_ITERATION ||
					num_accounts_checked >= MAX_EXPIRY_ACCOUNTS_PER_ITERATION ||
					start.elapsed() >= MAX_EXPIRY_TIME_PER_ITERATION
				{
					break;
				}
			}

			(to_evict, num_accounts_checked)
		};

		let mut expired = 0;

		for hash in to_evict {
			if let Err(e) = self.remove(&hash) {
				log::debug!(
					target: LOG_TARGET,
					"Error marking statement {:?} as expired: {:?}",
					HexDisplay::from(&hash),
					e
				);
			} else {
				expired += 1;
				log::trace!(
					target: LOG_TARGET,
					"Marked statement {:?} as expired",
					HexDisplay::from(&hash)
				);
			}
		}

		let mut submit_index = self.submit_index.write();
		let new_len = submit_index
			.accounts_to_check_for_expiry_stmts
			.len()
			.saturating_sub(num_accounts_checked);
		submit_index.accounts_to_check_for_expiry_stmts.truncate(new_len);

		drop(_start_check_expiration_timer);

		self.metrics.report(|metrics| {
			metrics.statements_expired_total.inc_by(expired);
		});
	}

	/// Drains the on-disk evicted journal of entries whose purge deadline has passed. Returns how
	/// many entries were drained.
	fn drain_due_evicted(&self, current_time: u64) -> Result<usize> {
		let mut commit: Vec<(u8, Vec<u8>, Option<Vec<u8>>)> = Vec::new();
		let mut drained = 0usize;
		{
			let mut iter =
				self.db.iter(col::INDEX_EVICTED).map_err(|e| Error::Db(e.to_string()))?;
			iter.seek_to_first().map_err(|e| Error::Db(e.to_string()))?;
			loop {
				let Some((key, _)) = iter.next().map_err(|e| Error::Db(e.to_string()))? else {
					break;
				};
				if key.len() < 8 {
					continue;
				}
				let mut purge_at_bytes = [0u8; 8];
				purge_at_bytes.copy_from_slice(&key[0..8]);
				if u64::from_be_bytes(purge_at_bytes) > current_time {
					// Entries are ordered by purge time, so nothing further is due.
					break;
				}
				if let Some(hash) = hash_from_index_key(&key) {
					commit.push((col::EXPIRED, hash.to_vec(), None));
				}
				commit.push((col::INDEX_EVICTED, key, None));
				drained += 1;
			}
		}
		if !commit.is_empty() {
			self.db.commit(commit).map_err(|e| Error::Db(e.to_string()))?;
		}
		Ok(drained)
	}

	/// Perform periodic store maintenance: permanently delete statements whose purge period has
	/// elapsed and refresh store metrics.
	///
	/// Expired and evicted statements are not removed from the database immediately; they are kept
	/// in the `EXPIRED` column for [`DEFAULT_PURGE_AFTER_SEC`] (default 48h) to prevent
	/// re-acceptance while they may still be propagating over gossip. This method removes those
	/// whose purge period has passed.
	///
	/// Runs in a background task on a fixed interval (`MAINTENANCE_PERIOD`, 29s). Enforcing
	/// per-account and global limits — expiring over-quota statements — is handled separately by
	/// `enforce_limits` on its own interval (`ENFORCE_LIMITS_PERIOD`, 31s), kept distinct to avoid
	/// holding the index lock for too long during maintenance.
	pub fn maintain(&self) {
		log::trace!(target: LOG_TARGET, "Started store maintenance");
		let current_time = self.timestamp();
		let deleted_count = match self.drain_due_evicted(current_time) {
			Ok(count) => count as u64,
			Err(e) => {
				log::warn!(target: LOG_TARGET, "Error writing to the statement database: {:?}", e);
				0
			},
		};

		let (
			active_count,
			expired_count,
			total_size,
			accounts_count,
			capacity_statements,
			capacity_bytes,
		) = {
			let mut submit_index = self.submit_index.write();
			submit_index.evicted_count =
				submit_index.evicted_count.saturating_sub(deleted_count as usize);
			(
				submit_index.entries.len(),
				submit_index.evicted_count,
				submit_index.total_size,
				submit_index.accounts.len(),
				submit_index.config.max_total_statements,
				submit_index.config.max_total_size,
			)
		};

		if deleted_count > 0 {
			self.metrics.report(|metrics| metrics.statements_pruned.inc_by(deleted_count));
		}

		self.metrics.report(|metrics| {
			metrics.statements_total.set(active_count as u64);
			metrics.bytes_total.set(total_size as u64);
			metrics.accounts_total.set(accounts_count as u64);
			metrics.expired_total.set(expired_count as u64);
			metrics.capacity_statements.set(capacity_statements as u64);
			metrics.capacity_bytes.set(capacity_bytes as u64);
		});

		log::trace!(
			target: LOG_TARGET,
			"Completed store maintenance. Purged: {}, Active: {}, Expired: {}",
			deleted_count,
			active_count,
			expired_count
		);
	}

	fn timestamp(&self) -> u64 {
		self.time_override.unwrap_or_else(|| {
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_secs()
		})
	}

	#[cfg(test)]
	fn set_time(&mut self, time: u64) {
		self.time_override = Some(time);
	}

	/// Returns `self` as [`StatementStoreExt`].
	pub fn as_statement_store_ext(self: Arc<Self>) -> StatementStoreExt {
		StatementStoreExt::new(self)
	}

	/// Return information of all known statements whose decryption key is identified as
	/// `dest`. The key must be available to the client.
	fn posted_clear_inner<R>(
		&self,
		match_all_topics: &[Topic],
		dest: [u8; 32],
		// Map the statement and the decrypted data to the desired result.
		mut map_f: impl FnMut(Statement, Vec<u8>) -> R,
	) -> Result<Vec<R>> {
		self.collect_statements(
			Some(dest),
			&OptimizedTopicFilter::MatchAll(match_all_topics.iter().cloned().collect()),
			|statement| {
				if let (Some(key), Some(_)) = (statement.decryption_key(), statement.data()) {
					let public: sp_core::ed25519::Public = UncheckedFrom::unchecked_from(key);
					let public: sp_statement_store::ed25519::Public = public.into();
					match self.keystore.key_pair::<sp_statement_store::ed25519::Pair>(&public) {
						Err(e) => {
							log::debug!(
								target: LOG_TARGET,
								"Keystore error: {:?}, for statement {:?}",
								e,
								HexDisplay::from(&statement.hash())
							);
							None
						},
						Ok(None) => {
							log::debug!(
								target: LOG_TARGET,
								"Keystore is missing key for statement {:?}",
								HexDisplay::from(&statement.hash())
							);
							None
						},
						Ok(Some(pair)) => match statement.decrypt_private(&pair.into_inner()) {
							Ok(r) => r.map(|data| map_f(statement, data)),
							Err(e) => {
								log::debug!(
									target: LOG_TARGET,
									"Decryption error: {:?}, for statement {:?}",
									e,
									HexDisplay::from(&statement.hash())
								);
								None
							},
						},
					}
				} else {
					None
				}
			},
		)
	}
}

impl StatementStore for Store {
	/// Return every statement currently in the store.
	///
	/// Takes a read lock on the query index, iterates all indexed hashes, reads and SCALE-decodes
	/// each statement from the `STATEMENTS` database column, and skips any entry that fails to
	/// decode.
	fn statements(&self) -> Result<Vec<(Hash, Statement)>> {
		let hashes = self.enumerate_hashes()?;
		let mut result = Vec::with_capacity(hashes.len());
		for hash in hashes {
			let Some(encoded) =
				self.db.get(col::STATEMENTS, &hash).map_err(|e| Error::Db(e.to_string()))?
			else {
				continue;
			};
			if let Ok(statement) = Statement::decode(&mut encoded.as_slice()) {
				result.push((hash, statement));
			}
		}
		Ok(result)
	}

	fn take_recent_statements(&self) -> Result<Vec<(Hash, Statement)>> {
		let recent = self.query_index.write().take_recent();
		let mut result = Vec::with_capacity(recent.len());
		for hash in recent {
			let Some(encoded) =
				self.db.get(col::STATEMENTS, &hash).map_err(|e| Error::Db(e.to_string()))?
			else {
				continue;
			};
			if let Ok(statement) = Statement::decode(&mut encoded.as_slice()) {
				result.push((hash, statement));
			}
		}
		Ok(result)
	}

	/// Read a single statement directly from the `STATEMENTS` database column by hash and decode
	/// it. Returns `Ok(None)` if no statement with that hash is stored.
	fn statement(&self, hash: &Hash) -> Result<Option<Statement>> {
		Ok(
			match self
				.db
				.get(col::STATEMENTS, hash.as_slice())
				.map_err(|e| Error::Db(e.to_string()))?
			{
				Some(entry) => {
					log::trace!(
						target: LOG_TARGET,
						"Queried statement {:?}",
						HexDisplay::from(hash)
					);
					Some(
						Statement::decode(&mut entry.as_slice())
							.map_err(|e| Error::Decode(e.to_string()))?,
					)
				},
				None => {
					log::trace!(
						target: LOG_TARGET,
						"Queried missing statement {:?}",
						HexDisplay::from(hash)
					);
					None
				},
			},
		)
	}

	fn has_statement(&self, hash: &Hash) -> bool {
		match self.db.get_size(col::STATEMENTS, hash.as_slice()) {
			Ok(size) => size.is_some(),
			Err(e) => {
				log::debug!(
					target: LOG_TARGET,
					"Error checking statement presence {:?}: {:?}",
					HexDisplay::from(hash),
					e
				);
				false
			},
		}
	}

	fn statement_hashes(&self) -> Vec<Hash> {
		self.enumerate_hashes().unwrap_or_else(|e| {
			log::warn!(target: LOG_TARGET, "Error enumerating statement hashes: {:?}", e);
			Vec::new()
		})
	}

	fn statements_by_hashes(
		&self,
		hashes: &[Hash],
		filter: &mut dyn FnMut(&Hash, &[u8], &Statement) -> FilterDecision,
	) -> Result<(Vec<(Hash, Statement)>, usize)> {
		let mut result = Vec::new();
		let mut processed = 0;
		for hash in hashes {
			processed += 1;
			let Some(encoded) =
				self.db.get(col::STATEMENTS, hash).map_err(|e| Error::Db(e.to_string()))?
			else {
				continue;
			};
			let Ok(statement) = Statement::decode(&mut encoded.as_slice()) else { continue };
			match filter(hash, &encoded, &statement) {
				FilterDecision::Skip => {},
				FilterDecision::Take => {
					result.push((*hash, statement));
				},
				FilterDecision::Abort => {
					// We did not process it :)
					processed -= 1;
					break;
				},
			}
		}

		Ok((result, processed))
	}

	/// Return the `data` of all statements matching all of `match_all_topics` that have no
	/// decryption key (i.e. public broadcasts).
	///
	/// Filters the query index by topic (intersection; an empty list matches every broadcast),
	/// reads and decodes each match from the `STATEMENTS` column, and returns the plaintext data,
	/// skipping any inconsistent entries.
	fn broadcasts(&self, match_all_topics: &[Topic]) -> Result<Vec<Vec<u8>>> {
		self.collect_statements(
			None,
			&OptimizedTopicFilter::MatchAll(match_all_topics.iter().cloned().collect()),
			|statement| statement.into_data(),
		)
	}

	/// Return the (encrypted) `data` of all statements matching all of `match_all_topics` whose
	/// decryption key equals `dest`.
	///
	/// Same filtering and DB read as [`broadcasts`](Self::broadcasts), but keyed on `dest` rather
	/// than the absence of a decryption key.
	fn posted(&self, match_all_topics: &[Topic], dest: [u8; 32]) -> Result<Vec<Vec<u8>>> {
		self.collect_statements(
			Some(dest),
			&OptimizedTopicFilter::MatchAll(match_all_topics.iter().cloned().collect()),
			|statement| statement.into_data(),
		)
	}

	/// Like [`posted`](Self::posted) but returns the decrypted data.
	///
	/// For each match, looks up the ed25519 key identified by `dest` in the keystore and decrypts
	/// the statement data; statements are skipped when the key is unavailable or decryption fails.
	fn posted_clear(&self, match_all_topics: &[Topic], dest: [u8; 32]) -> Result<Vec<Vec<u8>>> {
		self.posted_clear_inner(match_all_topics, dest, |_statement, data| data)
	}

	/// Return the full SCALE-encoded statements matching all of `match_all_topics` that have no
	/// decryption key (i.e. public broadcasts).
	///
	/// Takes a read lock on the query index and filters by the absence of a decryption key and
	/// topics (intersection / AND — an empty topic list matches every broadcast), then reads,
	/// decodes and re-encodes each match from the `STATEMENTS` column, skipping inconsistent
	/// entries. Unlike [`broadcasts`](Self::broadcasts), which returns only the data, this returns
	/// the whole statement.
	fn broadcasts_stmt(&self, match_all_topics: &[Topic]) -> Result<Vec<Vec<u8>>> {
		self.collect_statements(
			None,
			&OptimizedTopicFilter::MatchAll(match_all_topics.iter().cloned().collect()),
			|statement| Some(statement.encode()),
		)
	}

	/// Return the full SCALE-encoded statements matching all of `match_all_topics` whose decryption
	/// key equals `dest`.
	///
	/// Takes a read lock on the query index and filters by decryption key (`dest`) and topics
	/// (intersection / AND — an empty topic list matches every statement keyed to `dest`), then
	/// reads, decodes and re-encodes each match from the `STATEMENTS` column, skipping inconsistent
	/// entries. Unlike [`posted`](Self::posted), which returns only the (still-encrypted) data,
	/// this returns the whole statement.
	fn posted_stmt(&self, match_all_topics: &[Topic], dest: [u8; 32]) -> Result<Vec<Vec<u8>>> {
		self.collect_statements(
			Some(dest),
			&OptimizedTopicFilter::MatchAll(match_all_topics.iter().cloned().collect()),
			|statement| Some(statement.encode()),
		)
	}

	/// Return, for each statement matching all of `match_all_topics` whose decryption key equals
	/// `dest`, the SCALE-encoded statement concatenated with its decrypted data.
	///
	/// Filters as [`posted_stmt`](Self::posted_stmt), then for each match looks up the ed25519 key
	/// identified by `dest` in the keystore and decrypts the statement data, appending the
	/// plaintext to the encoded statement. Statements are skipped when the key is unavailable or
	/// decryption fails.
	fn posted_clear_stmt(
		&self,
		match_all_topics: &[Topic],
		dest: [u8; 32],
	) -> Result<Vec<Vec<u8>>> {
		self.posted_clear_inner(match_all_topics, dest, |statement, data| {
			let mut res = Vec::with_capacity(statement.size_hint() + data.len());
			statement.encode_to(&mut res);
			res.extend_from_slice(&data);
			res
		})
	}

	/// Submit a statement to the store, validating it and enforcing constraints.
	///
	/// Runs the following pipeline, short-circuiting on the first failure:
	/// 1. **Expiry check** — reject if the statement's expiration timestamp is already in the past
	///    (`SubmitResult::Invalid(InvalidReason::AlreadyExpired)`).
	/// 2. **Encoding size check** — reject if the encoded statement exceeds [`MAX_STATEMENT_SIZE`]
	///    (`InvalidReason::EncodingTooLarge`).
	/// 3. **Duplicate check** — look the hash up in the index. Whether a known or known-expired
	///    statement may be resubmitted depends on the [`StatementSource`]: `Chain` and `Local` can
	///    renew an expired statement, `Network` cannot (`SubmitResult::Known` / `KnownExpired`).
	/// 4. **Proof & signature** — extract the account from the proof and verify the signature
	///    (`InvalidReason::NoProof` / `InvalidReason::BadProof`).
	/// 5. **Allowance** — read the account's allowance (`StatementAllowance`: max count and size)
	///    directly from chain state at the best block (via the `statement_allowance_key` storage
	///    key — not a runtime call); reject with `SubmitResult::Rejected(NoAllowance)` if none is
	///    set. The best block is used for responsiveness; a statement accepted here may later be
	///    evicted when limits are enforced against the finalized block.
	/// 6. **Constraint check & eviction** — insert into the submit index, enforcing per-account
	///    limits (count, size, one statement per channel, higher priority replaces lower) and
	///    global limits ([`DEFAULT_MAX_TOTAL_STATEMENTS`], [`DEFAULT_MAX_TOTAL_SIZE`]), evicting
	///    lower-priority statements as needed (`SubmitResult::Rejected` if it still does not fit).
	/// 7. **Persist** — write the new statement and any evictions to the database, then update the
	///    in-memory query index.
	///
	/// Returns `SubmitResult::New` on success.
	fn submit(&self, statement: Statement, source: StatementSource) -> SubmitResult {
		let _histogram_submit_start_timer = self.metrics.start_submit_timer();
		let hash = statement.hash();
		// Get unix timestamp
		if self.timestamp() >= statement.get_expiration_timestamp_secs().into() {
			log::debug!(
				target: LOG_TARGET,
				"Statement is already expired: {:?}",
				HexDisplay::from(&hash),
			);
			let reason = InvalidReason::AlreadyExpired;
			self.metrics.report(|metrics| {
				metrics.validations_invalid.with_label_values(&[reason.label()]).inc();
			});
			return SubmitResult::Invalid(reason);
		}
		let encoded_size = statement.encoded_size();
		if encoded_size > MAX_STATEMENT_SIZE {
			log::debug!(
				target: LOG_TARGET,
				"Statement is too big for propogation: {:?} ({}/{} bytes)",
				HexDisplay::from(&hash),
				statement.encoded_size(),
				MAX_STATEMENT_SIZE
			);
			let reason = InvalidReason::EncodingTooLarge {
				submitted_size: encoded_size,
				max_size: MAX_STATEMENT_SIZE,
			};
			self.metrics.report(|metrics| {
				metrics.validations_invalid.with_label_values(&[reason.label()]).inc();
			});
			return SubmitResult::Invalid(reason);
		}

		// Deduplicate against statements we already store (in-memory submit index) or have recently
		// evicted (on-disk evicted journal).
		if self.submit_index.read().entries.contains_key(&hash) {
			if !source.can_be_resubmitted() {
				self.metrics.report(|metrics| {
					metrics.known_statements.with_label_values(&["known"]).inc();
				});
				return SubmitResult::Known;
			}
		} else if self.db.get_size(col::EXPIRED, hash.as_slice()).ok().flatten().is_some() {
			if !source.can_be_resubmitted() {
				self.metrics.report(|metrics| {
					metrics.known_statements.with_label_values(&["known_expired"]).inc();
				});
				return SubmitResult::KnownExpired;
			}
		}

		let Some(account_id) = statement.account_id() else {
			log::debug!(
				target: LOG_TARGET,
				"Statement validation failed: Missing proof ({:?})",
				HexDisplay::from(&hash),
			);
			let reason = InvalidReason::NoProof;
			self.metrics.report(|metrics| {
				metrics.validations_invalid.with_label_values(&[reason.label()]).inc();
			});
			return SubmitResult::Invalid(reason);
		};

		match statement.verify_signature() {
			SignatureVerificationResult::Valid(_) => {},
			SignatureVerificationResult::Invalid => {
				log::debug!(
					target: LOG_TARGET,
					"Statement validation failed: BadProof, {:?}",
					HexDisplay::from(&hash),
				);
				let reason = InvalidReason::BadProof;
				self.metrics.report(|metrics| {
					metrics.validations_invalid.with_label_values(&[reason.label()]).inc();
				});
				return SubmitResult::Invalid(reason);
			},
			SignatureVerificationResult::NoSignature => {
				log::debug!(
					target: LOG_TARGET,
					"Statement validation failed: NoProof, {:?}",
					HexDisplay::from(&hash),
				);
				let reason = InvalidReason::NoProof;
				self.metrics.report(|metrics| {
					metrics.validations_invalid.with_label_values(&[reason.label()]).inc();
				});
				return SubmitResult::Invalid(reason);
			},
		};

		// Check statement allowance for the account and evict statements if necessary to make room
		// for the new statement. We use the best block for allowance checks to allow for more
		// up-to-date allowances. This means that in some cases, a statement may be accepted but
		// then later evicted when we enforce limits based on the finalized block, if the best_hash
		// does not make it into the finalized chain, but this is an acceptable tradeoff for
		// better responsiveness to allowance changes.
		let validation = match (self.read_allowance_fn)(&account_id, AllowanceBlock::Best) {
			Ok(Some(allowance)) => allowance,
			Ok(None) => {
				log::debug!(
					target: LOG_TARGET,
					"Account {} has no statement allowance set",
					HexDisplay::from(&account_id),
				);
				let reason = RejectionReason::NoAllowance;
				self.metrics.report(|metrics| {
					metrics.rejections.with_label_values(&[reason.label()]).inc();
				});
				return SubmitResult::Rejected(reason);
			},
			Err(e) => {
				log::debug!(
					target: LOG_TARGET,
					"Reading statement allowance for account {} failed",
					HexDisplay::from(&account_id),
				);
				self.metrics.report(|metrics| {
					metrics.internal_errors.with_label_values(&["read_allowance"]).inc();
				});
				return SubmitResult::InternalError(e);
			},
		};

		let current_time = self.timestamp();
		let (evicted, seq) = {
			let mut submit_index = self.submit_index.write();

			let outcome =
				match submit_index.insert(hash, &statement, &account_id, &validation, current_time)
				{
					Ok(outcome) => outcome,
					Err(reason) => {
						self.metrics.report(|metrics| {
							metrics.rejections.with_label_values(&[reason.label()]).inc();
						});
						return SubmitResult::Rejected(reason);
					},
				};

			let mut commit = Vec::new();
			commit.push((col::STATEMENTS, hash.to_vec(), Some(statement.encode())));
			commit.extend(statement_index_ops(&hash, &statement, true));

			let mut evicted_statements = Vec::new();
			for h in &outcome.evicted {
				commit.push((col::STATEMENTS, h.to_vec(), None));
				match self.db.get(col::STATEMENTS, h) {
					Ok(Some(encoded)) => {
						if let Ok(evicted_statement) = Statement::decode(&mut encoded.as_slice()) {
							commit.extend(statement_index_ops(h, &evicted_statement, false));
							evicted_statements.push(evicted_statement);
						}
					},
					Ok(None) => {},
					Err(e) => log::warn!(
						target: LOG_TARGET,
						"Could not read evicted statement {:?} to clear its index: {:?}",
						HexDisplay::from(h),
						e
					),
				}
			}
			for (h, purge_at) in &outcome.banned {
				commit.push((col::EXPIRED, h.to_vec(), Some((h, current_time).encode())));
				commit.push((
					col::INDEX_EVICTED,
					evicted_index_key(*purge_at, h),
					Some(INDEX_EMPTY_VALUE.to_vec()),
				));
			}

			if let Err(e) = self.db.commit(commit) {
				log::debug!(
					target: LOG_TARGET,
					"Statement validation failed: database error {}, {:?}",
					e,
					statement
				);
				self.metrics.report(|metrics| {
					metrics.internal_errors.with_label_values(&["db_commit"]).inc();
				});
				return SubmitResult::InternalError(Error::Db(e.to_string()));
			}
			let seq = submit_index.note_seq(hash);
			(evicted_statements, seq)
		}; // Release submit index lock
		{
			let mut query_index = self.query_index.write();
			for h in &evicted {
				query_index.note_remove(&h.hash(), h);
			}
			query_index.note_insert(hash, &statement);
		} // Release read index lock
		self.subscription_manager.notify(seq, statement);
		self.metrics.report(|metrics| metrics.submitted_statements.inc());
		log::trace!(target: LOG_TARGET, "Statement submitted: {:?}", HexDisplay::from(&hash));
		SubmitResult::New
	}

	/// Soft-delete a statement by hash: mark it expired in the index, drop it from the `STATEMENTS`
	/// column, and record it in the `EXPIRED` column so it cannot be re-accepted until its purge
	/// period elapses (see [`maintain`](Self::maintain)). No-op if the statement is unknown.
	fn remove(&self, hash: &Hash) -> Result<()> {
		let current_time = self.timestamp();
		let removed_statement = {
			let mut submit_index = self.submit_index.write();
			// Read the body under the submit-index lock: a concurrent first-time submit could
			// otherwise commit the statement between the read and `make_expired`, and its
			// read-index entries would never be cleared.
			let statement =
				match self.db.get(col::STATEMENTS, hash).map_err(|e| Error::Db(e.to_string()))? {
					Some(encoded) => Statement::decode(&mut encoded.as_slice()).ok(),
					None => None,
				};
			match submit_index.make_expired(hash, current_time) {
				Some(eviction) => {
					let mut commit = vec![(col::STATEMENTS, hash.to_vec(), None)];
					if let Some(statement) = &statement {
						commit.extend(statement_index_ops(hash, statement, false));
					}
					if let Eviction::Banned(purge_at) = eviction {
						commit.push((
							col::EXPIRED,
							hash.to_vec(),
							Some((hash, current_time).encode()),
						));
						commit.push((
							col::INDEX_EVICTED,
							evicted_index_key(purge_at, hash),
							Some(INDEX_EMPTY_VALUE.to_vec()),
						));
					}
					if let Err(e) = self.db.commit(commit) {
						log::debug!(
							target: LOG_TARGET,
							"Error removing statement: database error {}, {:?}",
							e,
							HexDisplay::from(hash),
						);
						return Err(Error::Db(e.to_string()));
					}
					statement
				},
				None => None,
			}
		};
		if let Some(statement) = &removed_statement {
			self.query_index.write().note_remove(hash, statement);
		}
		Ok(())
	}

	/// Remove every statement authored by `who`, applying the same soft-delete as
	/// [`remove`](Self::remove) to each.
	fn remove_by(&self, who: [u8; 32]) -> Result<()> {
		let current_time = self.timestamp();
		let removed_statements = {
			let mut submit_index = self.submit_index.write();
			let hashes: Vec<Hash> = submit_index
				.accounts
				.get(&who)
				.map(|account_rec| account_rec.by_priority.keys().map(|k| k.hash).collect())
				.unwrap_or_default();

			let mut commit = Vec::new();
			let mut removed_statements = Vec::new();
			for hash in &hashes {
				// Read the body before deleting it, to clear its query-index entries.
				let statement = match self.db.get(col::STATEMENTS, hash) {
					Ok(Some(encoded)) => Statement::decode(&mut encoded.as_slice()).ok(),
					Ok(None) => None,
					Err(e) => {
						log::warn!(
							target: LOG_TARGET,
							"Could not read statement {:?} to clear its index: {:?}",
							HexDisplay::from(hash),
							e
						);
						None
					},
				};
				if let Some(eviction) = submit_index.make_expired(hash, current_time) {
					commit.push((col::STATEMENTS, hash.to_vec(), None));
					if let Eviction::Banned(purge_at) = eviction {
						commit.push((
							col::EXPIRED,
							hash.to_vec(),
							Some((hash, current_time).encode()),
						));
						commit.push((
							col::INDEX_EVICTED,
							evicted_index_key(purge_at, hash),
							Some(INDEX_EMPTY_VALUE.to_vec()),
						));
					}
					if let Some(statement) = statement {
						commit.extend(statement_index_ops(hash, &statement, false));
						removed_statements.push((*hash, statement));
					}
				}
			}
			self.db.commit(commit).map_err(|e| {
				log::debug!(
					target: LOG_TARGET,
					"Error removing statement: database error {}, remove by {:?}",
					e,
					HexDisplay::from(&who),
				);

				Error::Db(e.to_string())
			})?;
			removed_statements
		};
		if !removed_statements.is_empty() {
			let mut read_index = self.query_index.write();
			for (hash, statement) in &removed_statements {
				read_index.note_remove(hash, statement);
			}
		}
		Ok(())
	}
}

/// RAII guard that deregisters a subscription snapshot scan (see [`SubmitIndex::begin_scan`] /
/// [`SubmitIndex::end_scan`]) when dropped, so the `recent_seqs` window is always released — on the
/// happy path, on an early `?` return, or on a panic during the snapshot.
struct ScanGuard<'a> {
	store: &'a Store,
	watermark: u64,
}

impl Drop for ScanGuard<'_> {
	fn drop(&mut self) {
		self.store.submit_index.write().end_scan(self.watermark);
	}
}

impl StatementStoreSubscriptionApi for Store {
	fn subscribe_statement(
		&self,
		topic_filter: OptimizedTopicFilter,
	) -> Result<(Vec<Vec<u8>>, async_channel::Sender<StatementEvent>, SubscriptionStatementsStream)>
	{
		// Exactly-once delivery via a sequence-number watermark. Under the submit-index write lock
		// we atomically (a) capture the current sequence boundary `W` and (b) register the
		// subscription with `W` as its watermark.
		let (subscription_sender, subscription_stream, watermark) = {
			let mut submit_index = self.submit_index.write();
			let watermark = submit_index.begin_scan();
			let (sender, stream) =
				self.subscription_manager.subscribe(topic_filter.clone(), watermark);
			(sender, stream, watermark)
		};
		let _scan_guard = ScanGuard { store: self, watermark };

		let mut hashes = HashSet::new();
		self.iterate_with(None, &topic_filter, |hash| {
			hashes.insert(*hash);
			Ok(())
		})?;

		let hashes: Vec<Hash> = {
			let submit_index = self.submit_index.read();
			hashes
				.into_iter()
				.filter(|hash| submit_index.seq_covered_by_snapshot(hash, watermark))
				.collect()
		};

		let mut existing_statements = Vec::with_capacity(hashes.len());
		for hash in hashes {
			if let Some(entry) = self.read_statement_encoded(&hash)? {
				existing_statements.push(entry);
			}
		}

		if existing_statements.is_empty() {
			subscription_sender
				.send_blocking(StatementEvent::NewStatements {
					statements: vec![],
					remaining: Some(0),
				})
				.ok();
		}
		Ok((existing_statements, subscription_sender, subscription_stream))
	}
}

impl Store {
	fn with_snapshot_hashes(
		&self,
		filter: &OptimizedTopicFilter,
		enqueue: &mut dyn FnMut(Vec<Hash>),
	) -> Result<()> {
		// Hold the submit-index read lock across both the on-disk index scan and the `AddFilter`
		// enqueue. `submit` assigns the statement's sequence number and commits its body under the
		// write lock and only notifies matchers afterwards, so with the read lock held here every
		// statement is either already committed (and therefore visible to the scan below) or its
		// `NewStatement` notification is enqueued after the `AddFilter` message and matched live.
		// Statements that end up in both are deduplicated by hash on the matcher side. The lock
		// order (`submit_index` → `query_index`, taken briefly inside `iterate_with`) matches
		// `populate`.
		let _guard = self.submit_index.read();
		let mut snapshot_hashes = Vec::new();
		let mut seen = HashSet::new();
		self.iterate_with(None, filter, |hash| {
			if seen.insert(*hash) {
				snapshot_hashes.push(*hash);
			}
			Ok(())
		})?;
		enqueue(snapshot_hashes);
		Ok(())
	}

	fn statement_by_hash(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
		self.db.get(col::STATEMENTS, hash).map_err(|e| Error::Db(e.to_string()))
	}
}

impl MultiFilterSubscriptionApi for Arc<Store> {
	fn create_subscription(&self) -> (SubscriptionHandle, MultiFilterEventStream) {
		let inner =
			Arc::new(parking_lot::Mutex::new(crate::subscription::SubscriptionHandleInner::new()));
		let snapshot_provider: Arc<dyn ReplaySnapshotProvider> = Arc::new(Arc::downgrade(self));
		let (sub_id, stream) = self.subscription_manager.subscribe_empty(snapshot_provider.clone());

		let handle = SubscriptionHandle {
			sub_id,
			inner,
			matchers: self.subscription_manager.matchers(),
			snapshot_provider,
		};
		(handle, stream)
	}
}

#[cfg(test)]
impl Store {
	/// Number of hashes currently in the on-disk evicted journal (per the in-memory counter).
	fn evicted_count(&self) -> usize {
		self.submit_index.read().evicted_count
	}

	/// Whether `hash` is currently banned from re-acceptance (present in the evicted journal).
	fn is_evicted(&self, hash: &Hash) -> bool {
		self.db.get_size(col::EXPIRED, hash.as_slice()).ok().flatten().is_some()
	}

	/// Whether the on-disk topic index links `topic` to `hash`.
	fn index_has_topic(&self, topic: &Topic, hash: &Hash) -> bool {
		self.index_set_contains(&IndexSet::Topic(*topic), hash).unwrap_or(false)
	}

	/// Whether the on-disk decryption-key index links `key` to `hash`.
	fn index_has_dec_key(&self, key: &Option<DecryptionKey>, hash: &Hash) -> bool {
		self.index_set_contains(&IndexSet::DecKey(*key), hash).unwrap_or(false)
	}
}

#[cfg(test)]
mod tests {

	use crate::{col, Store};
	use sc_keystore::Keystore;
	use sp_core::{Decode, Encode, Pair};
	use sp_statement_store::{
		AccountId, Channel, DecryptionKey, InvalidReason, Proof, RejectionReason, Statement,
		StatementSource, StatementStore, SubmitResult, Topic,
	};

	type Extrinsic = sp_runtime::OpaqueExtrinsic;
	type Hash = sp_core::H256;
	type Hashing = sp_runtime::traits::BlakeTwo256;
	type BlockNumber = u64;
	type Header = sp_runtime::generic::Header<BlockNumber, Hashing>;
	type Block = sp_runtime::generic::Block<Header, Extrinsic>;

	const TEST_BEST_BLOCK_HASH: [u8; 32] = [1u8; 32];

	/// Maximum seed value used by `account(seed)`/`statement(seed, ...)` in this
	/// test module. Increase if you add tests that pass larger seed values to
	/// `statement(..)`. The reverse-lookup table in `TestClient::storage` is
	/// populated lazily for seeds in `0..=MAX_TEST_ACCOUNT_SEED`.
	const MAX_TEST_ACCOUNT_SEED: u64 = 64;

	/// Reverse-lookup table from a real sr25519 public key back to the synthetic
	/// `u64` seed it was derived from. Populated once with seeds in
	/// `0..=MAX_TEST_ACCOUNT_SEED`, then consulted by `TestClient::storage` to
	/// figure out which allowance bucket to return for a given account.
	fn account_seed_table() -> &'static std::collections::BTreeMap<AccountId, u64> {
		use std::sync::OnceLock;
		static TABLE: OnceLock<std::collections::BTreeMap<AccountId, u64>> = OnceLock::new();
		TABLE.get_or_init(|| {
			let mut t = std::collections::BTreeMap::new();
			for seed in 0..=MAX_TEST_ACCOUNT_SEED {
				t.insert(account_keypair(seed).public().0, seed);
			}
			t
		})
	}

	#[derive(Clone)]
	pub(crate) struct TestClient;

	pub(crate) type TestBackend = sc_client_api::in_mem::Backend<Block>;

	impl sc_client_api::StorageProvider<Block, TestBackend> for TestClient {
		fn storage(
			&self,
			_hash: Hash,
			key: &sc_client_api::StorageKey,
		) -> sp_blockchain::Result<Option<sc_client_api::StorageData>> {
			use sp_statement_store::StatementAllowance;

			assert_eq!(&key.0[0..21], b":statement_allowance:" as &[u8],);

			// Recover the synthetic test seed from the account id. Unknown accounts
			// (e.g. //Alice for `signed_statement`) fall through to a generic default.
			let account_bytes: AccountId = key.0[21..53].try_into().unwrap();
			let seed = account_seed_table().get(&account_bytes).copied();
			let allowance = match seed {
				// Account 0 has no allowance (used to test eviction of all statements)
				Some(0) => return Ok(None),
				Some(1) => StatementAllowance::new(1, 1000),
				Some(2) => StatementAllowance::new(2, 1000),
				Some(3) => StatementAllowance::new(3, 1000),
				Some(4) => StatementAllowance::new(4, 1000),
				Some(42) => StatementAllowance::new(42, (42 * crate::MAX_STATEMENT_SIZE) as u32),
				Some(_) | None => StatementAllowance::new(100, 1000),
			};
			Ok(Some(sc_client_api::StorageData(allowance.encode())))
		}

		fn storage_hash(
			&self,
			_hash: Hash,
			_key: &sc_client_api::StorageKey,
		) -> sp_blockchain::Result<Option<Hash>> {
			unimplemented!()
		}

		fn storage_keys(
			&self,
			_hash: Hash,
			_prefix: Option<&sc_client_api::StorageKey>,
			_start_key: Option<&sc_client_api::StorageKey>,
		) -> sp_blockchain::Result<
			sc_client_api::backend::KeysIter<
				<TestBackend as sc_client_api::Backend<Block>>::State,
				Block,
			>,
		> {
			unimplemented!()
		}

		fn storage_pairs(
			&self,
			_hash: Hash,
			_prefix: Option<&sc_client_api::StorageKey>,
			_start_key: Option<&sc_client_api::StorageKey>,
		) -> sp_blockchain::Result<
			sc_client_api::backend::PairsIter<
				<TestBackend as sc_client_api::Backend<Block>>::State,
				Block,
			>,
		> {
			unimplemented!()
		}

		fn child_storage(
			&self,
			_hash: Hash,
			_child_info: &sc_client_api::ChildInfo,
			_key: &sc_client_api::StorageKey,
		) -> sp_blockchain::Result<Option<sc_client_api::StorageData>> {
			unimplemented!()
		}

		fn child_storage_keys(
			&self,
			_hash: Hash,
			_child_info: sc_client_api::ChildInfo,
			_prefix: Option<&sc_client_api::StorageKey>,
			_start_key: Option<&sc_client_api::StorageKey>,
		) -> sp_blockchain::Result<
			sc_client_api::backend::KeysIter<
				<TestBackend as sc_client_api::Backend<Block>>::State,
				Block,
			>,
		> {
			unimplemented!()
		}

		fn child_storage_hash(
			&self,
			_hash: Hash,
			_child_info: &sc_client_api::ChildInfo,
			_key: &sc_client_api::StorageKey,
		) -> sp_blockchain::Result<Option<Hash>> {
			unimplemented!()
		}

		fn closest_merkle_value(
			&self,
			_hash: Hash,
			_key: &sc_client_api::StorageKey,
		) -> sp_blockchain::Result<Option<sc_client_api::MerkleValue<Hash>>> {
			unimplemented!()
		}

		fn child_closest_merkle_value(
			&self,
			_hash: Hash,
			_child_info: &sc_client_api::ChildInfo,
			_key: &sc_client_api::StorageKey,
		) -> sp_blockchain::Result<Option<sc_client_api::MerkleValue<Hash>>> {
			unimplemented!()
		}
	}

	impl sp_blockchain::HeaderBackend<Block> for TestClient {
		fn header(&self, _hash: Hash) -> sp_blockchain::Result<Option<Header>> {
			unimplemented!()
		}
		fn info(&self) -> sp_blockchain::Info<Block> {
			sp_blockchain::Info {
				best_hash: TEST_BEST_BLOCK_HASH.into(),
				best_number: 0,
				genesis_hash: Default::default(),
				finalized_hash: TEST_BEST_BLOCK_HASH.into(),
				finalized_number: 1,
				finalized_state: None,
				number_leaves: 0,
				block_gap: None,
			}
		}
		fn status(&self, _hash: Hash) -> sp_blockchain::Result<sp_blockchain::BlockStatus> {
			unimplemented!()
		}
		fn number(&self, _hash: Hash) -> sp_blockchain::Result<Option<BlockNumber>> {
			unimplemented!()
		}
		fn hash(&self, _number: BlockNumber) -> sp_blockchain::Result<Option<Hash>> {
			unimplemented!()
		}
	}

	fn test_store() -> (Store, tempfile::TempDir) {
		sp_tracing::init_for_tests();
		let temp_dir = tempfile::Builder::new().tempdir().expect("Error creating test dir");

		let client = std::sync::Arc::new(TestClient);
		let mut path: std::path::PathBuf = temp_dir.path().into();
		path.push("db");
		let keystore = std::sync::Arc::new(sc_keystore::LocalKeystore::in_memory());
		let store = Store::new::<Block, TestClient, TestBackend>(
			&path,
			Default::default(),
			client,
			keystore,
			None,
			Box::new(sp_core::testing::TaskExecutor::new()),
		)
		.unwrap();
		(store, temp_dir) // return order is important. Store must be dropped before TempDir
	}

	pub fn signed_statement(data: u8) -> Statement {
		signed_statement_with_topics(data, &[], None)
	}

	fn signed_statement_with_topics(
		data: u8,
		topics: &[Topic],
		dec_key: Option<DecryptionKey>,
	) -> Statement {
		let mut statement = Statement::new();
		statement.set_plain_data(vec![data]);
		statement.set_expiry(u64::MAX);

		for i in 0..topics.len() {
			statement.set_topic(i, topics[i]);
		}
		if let Some(key) = dec_key {
			statement.set_decryption_key(key);
		}
		let kp = sp_core::ed25519::Pair::from_string("//Alice", None).unwrap();
		statement.sign_ed25519_private(&kp);
		statement
	}

	fn topic(data: u64) -> Topic {
		let mut bytes = [0u8; 32];
		bytes[0..8].copy_from_slice(&data.to_le_bytes());
		Topic::from(bytes)
	}

	fn dec_key(data: u64) -> DecryptionKey {
		let mut dec_key: DecryptionKey = Default::default();
		dec_key[0..8].copy_from_slice(&data.to_le_bytes());
		dec_key
	}

	/// Returns the deterministic ed25519 keypair used to author statements for the
	/// synthetic test account `seed`.
	///
	/// Uses ed25519 rather than sr25519 because schnorrkel signing is non-deterministic
	/// (the signature depends on RNG state), so calling `statement(id, prio, ch, len)`
	/// twice would produce different hashes. Several tests compare statement hashes
	/// against pre-computed values; ed25519 keeps those comparisons stable.
	fn account_keypair(seed: u64) -> sp_core::ed25519::Pair {
		sp_core::ed25519::Pair::from_string(&format!("//StatementAccount{seed}"), None)
			.expect("Derivation path is valid; qed")
	}

	fn account(id: u64) -> AccountId {
		account_keypair(id).public().0
	}

	/// Signs `stmt` with `account_id`'s test keypair. Tests that build a statement via
	/// `unsigned_statement(..)` and then mutate it call this exactly once at the end.
	fn sign_with(stmt: &mut Statement, account_id: u64) {
		stmt.sign_ed25519_private(&account_keypair(account_id));
	}

	fn channel(id: u64) -> Channel {
		let mut channel: Channel = Default::default();
		channel[0..8].copy_from_slice(&id.to_le_bytes());
		channel
	}

	/// Builds a test statement without signing it. Use this when a test needs to mutate
	/// the statement (encryption, expiry change, topic update, etc.) before submission —
	/// call `sign_with(&mut stmt, account_id)` once after all mutations.
	fn unsigned_statement(
		account_id: u64,
		priority: u32,
		c: Option<u64>,
		data_len: usize,
	) -> Statement {
		assert!(
			account_id <= MAX_TEST_ACCOUNT_SEED,
			"account_id {account_id} exceeds MAX_TEST_ACCOUNT_SEED ({MAX_TEST_ACCOUNT_SEED}); \
			 raise the constant if you need a wider range",
		);
		let mut statement = Statement::new();
		let mut data = Vec::new();
		data.resize(data_len, 0);
		statement.set_plain_data(data);
		statement.set_expiry_from_parts(u32::MAX, priority);
		if let Some(c) = c {
			statement.set_channel(channel(c));
		}
		statement
	}

	fn statement(account_id: u64, priority: u32, c: Option<u64>, data_len: usize) -> Statement {
		let mut statement = unsigned_statement(account_id, priority, c, data_len);
		sign_with(&mut statement, account_id);
		statement
	}

	#[test]
	fn submit_one() {
		let (store, _temp) = test_store();
		let statement0 = signed_statement(0);
		assert_eq!(store.submit(statement0, StatementSource::Network), SubmitResult::New);
		let statement1 = statement(1, 1, None, 0);
		assert_eq!(store.submit(statement1, StatementSource::Network), SubmitResult::New);
	}

	#[test]
	fn save_and_load_statements() {
		let (store, temp) = test_store();
		let statement0 = signed_statement(0);
		let statement1 = signed_statement(1);
		let statement2 = signed_statement(2);
		assert_eq!(store.submit(statement0.clone(), StatementSource::Network), SubmitResult::New);
		assert_eq!(store.submit(statement1.clone(), StatementSource::Network), SubmitResult::New);
		assert_eq!(store.submit(statement2.clone(), StatementSource::Network), SubmitResult::New);
		assert_eq!(store.statements().unwrap().len(), 3);
		assert_eq!(store.broadcasts(&[]).unwrap().len(), 3);
		assert_eq!(store.statement(&statement1.hash()).unwrap(), Some(statement1.clone()));
		let keystore = store.keystore.clone();
		drop(store);

		let client = std::sync::Arc::new(TestClient);
		let mut path: std::path::PathBuf = temp.path().into();
		path.push("db");
		let store = Store::new::<Block, TestClient, TestBackend>(
			&path,
			Default::default(),
			client,
			keystore,
			None,
			Box::new(sp_core::testing::TaskExecutor::new()),
		)
		.unwrap();
		assert_eq!(store.statements().unwrap().len(), 3);
		assert_eq!(store.broadcasts(&[]).unwrap().len(), 3);
		assert_eq!(store.statement(&statement1.hash()).unwrap(), Some(statement1));
	}

	#[test]
	fn migrates_v1_database_to_on_disk_index() {
		sp_tracing::init_for_tests();
		let temp = tempfile::Builder::new().tempdir().expect("Error creating test dir");
		let mut path: std::path::PathBuf = temp.path().into();
		path.push("db");
		// The store appends `statements` to the path it is given.
		let mut db_path = path.clone();
		db_path.push("statements");

		// One addressed statement (topics 1 & 2, decryption key 9) and one broadcast (topic 1, no
		// key).
		let addressed = signed_statement_with_topics(1, &[topic(1), topic(2)], Some(dec_key(9)));
		let broadcast = signed_statement_with_topics(2, &[topic(1)], None);
		let h_addressed = addressed.hash();
		let h_broadcast = broadcast.hash();

		// A hash seeded into the legacy EXPIRED column, with a deadline far in the future so it
		// survives the maintenance pass triggered during migration.
		let expired_hash = topic(999);
		let expired_ts = 10_000_000_000u64;

		// Build a version-1 database by hand: three columns and no on-disk read index.
		{
			let mut cfg = parity_db::Options::with_columns(&db_path, 3);
			let statement_col = &mut cfg.columns[1];
			statement_col.ref_counted = false;
			statement_col.preimage = true;
			statement_col.uniform = true;
			let db = parity_db::Db::open_or_create(&cfg).unwrap();
			db.commit([
				(0u8, b"version".to_vec(), Some(1u32.to_le_bytes().to_vec())),
				(1u8, h_addressed.to_vec(), Some(addressed.encode())),
				(1u8, h_broadcast.to_vec(), Some(broadcast.encode())),
				(2u8, expired_hash.to_vec(), Some((expired_hash, expired_ts).encode())),
			])
			.unwrap();
		}

		let open = |path: &std::path::Path| {
			Store::new::<Block, TestClient, TestBackend>(
				path,
				Default::default(),
				std::sync::Arc::new(TestClient),
				std::sync::Arc::new(sc_keystore::LocalKeystore::in_memory()),
				None,
				Box::new(sp_core::testing::TaskExecutor::new()),
			)
			.unwrap()
		};

		// Re-open through the store: it must add the index columns, rebuild them from the bodies,
		// rebuild the evicted journal from EXPIRED, and bump the version to 2.
		let store = open(&path);

		// Bodies survived.
		assert_eq!(store.statements().unwrap().len(), 2);
		assert!(store.statement(&h_addressed).unwrap().is_some());
		assert!(store.statement(&h_broadcast).unwrap().is_some());

		// The read index was rebuilt on disk.
		assert!(store.index_has_topic(&topic(1), &h_addressed));
		assert!(store.index_has_topic(&topic(1), &h_broadcast));
		assert!(store.index_has_topic(&topic(2), &h_addressed));
		assert!(store.index_has_dec_key(&Some(dec_key(9)), &h_addressed));
		assert!(store.index_has_dec_key(&None, &h_broadcast));

		// And it answers queries: only the broadcast matches topic 1 with no key, only the
		// addressed statement matches topic 1 for key 9.
		assert_eq!(store.broadcasts(&[topic(1)]).unwrap().len(), 1);
		assert_eq!(store.posted(&[topic(1)], dec_key(9)).unwrap().len(), 1);

		// The evicted journal was rebuilt from the legacy EXPIRED column.
		assert!(store.is_evicted(&expired_hash));
		assert_eq!(store.evicted_count(), 1);

		// The database is now at the current version; re-opening does not migrate again.
		drop(store);
		let store = open(&path);
		assert_eq!(store.statements().unwrap().len(), 2);
		assert!(store.index_has_topic(&topic(1), &h_broadcast));
		assert!(store.is_evicted(&expired_hash));
	}

	#[test]
	fn take_recent_statements_clears_index() {
		let (store, _temp) = test_store();
		let statement0 = signed_statement(0);
		let statement1 = signed_statement(1);
		let statement2 = signed_statement(2);
		let statement3 = signed_statement(3);

		let _ = store.submit(statement0.clone(), StatementSource::Local);
		let _ = store.submit(statement1.clone(), StatementSource::Local);
		let _ = store.submit(statement2.clone(), StatementSource::Local);

		let recent1 = store.take_recent_statements().unwrap();
		let (recent1_hashes, recent1_statements): (Vec<_>, Vec<_>) = recent1.into_iter().unzip();
		let expected1 = vec![statement0, statement1, statement2];
		assert!(expected1.iter().all(|s| recent1_hashes.contains(&s.hash())));
		assert!(expected1.iter().all(|s| recent1_statements.contains(s)));

		// Recent statements are cleared.
		let recent2 = store.take_recent_statements().unwrap();
		assert_eq!(recent2.len(), 0);

		store.submit(statement3.clone(), StatementSource::Network);

		let recent3 = store.take_recent_statements().unwrap();
		let (recent3_hashes, recent3_statements): (Vec<_>, Vec<_>) = recent3.into_iter().unzip();
		let expected3 = vec![statement3];
		assert!(expected3.iter().all(|s| recent3_hashes.contains(&s.hash())));
		assert!(expected3.iter().all(|s| recent3_statements.contains(s)));

		// Recent statements are cleared, but statements remain in the store.
		assert_eq!(store.statements().unwrap().len(), 4);
	}

	#[test]
	fn search_by_topic_and_key() {
		let (store, _temp) = test_store();
		let statement0 = signed_statement(0);
		let statement1 = signed_statement_with_topics(1, &[topic(0)], None);
		let statement2 = signed_statement_with_topics(2, &[topic(0), topic(1)], Some(dec_key(2)));
		let statement3 = signed_statement_with_topics(3, &[topic(0), topic(1), topic(2)], None);
		let statement4 =
			signed_statement_with_topics(4, &[topic(0), topic(42), topic(2), topic(3)], None);
		let statements = vec![statement0, statement1, statement2, statement3, statement4];
		for s in &statements {
			store.submit(s.clone(), StatementSource::Network);
		}

		let assert_topics = |topics: &[u64], key: Option<u64>, expected: &[u8]| {
			let key = key.map(dec_key);
			let topics: Vec<_> = topics.iter().map(|t| topic(*t)).collect();
			let mut got_vals: Vec<_> = if let Some(key) = key {
				store.posted(&topics, key).unwrap().into_iter().map(|d| d[0]).collect()
			} else {
				store.broadcasts(&topics).unwrap().into_iter().map(|d| d[0]).collect()
			};
			got_vals.sort();
			assert_eq!(expected.to_vec(), got_vals);
		};

		assert_topics(&[], None, &[0, 1, 3, 4]);
		assert_topics(&[], Some(2), &[2]);
		assert_topics(&[0], None, &[1, 3, 4]);
		assert_topics(&[1], None, &[3]);
		assert_topics(&[2], None, &[3, 4]);
		assert_topics(&[3], None, &[4]);
		assert_topics(&[42], None, &[4]);

		assert_topics(&[0, 1], None, &[3]);
		assert_topics(&[0, 1], Some(2), &[2]);
		assert_topics(&[0, 1, 99], Some(2), &[]);
		assert_topics(&[1, 2], None, &[3]);
		assert_topics(&[99], None, &[]);
		assert_topics(&[0, 99], None, &[]);
		assert_topics(&[0, 1, 2, 3, 42], None, &[]);
	}

	#[test]
	fn constraints() {
		let (store, _temp) = test_store();

		store.submit_index.write().config.max_total_size = 3000;
		let source = StatementSource::Network;
		let ok = SubmitResult::New;

		// Account 1 (limit = 1 msg, 1000 bytes)

		// Oversized statement is not allowed. Limit for account 1 is 1 msg, 1000 bytes
		assert!(matches!(
			store.submit(statement(1, 1, Some(1), 2000), source),
			SubmitResult::Rejected(_)
		));
		assert_eq!(store.submit(statement(1, 1, Some(1), 500), source), ok);
		// Would not replace channel message with same priority
		assert!(matches!(
			store.submit(statement(1, 1, Some(1), 200), source),
			SubmitResult::Rejected(_)
		));
		assert_eq!(store.submit(statement(1, 2, Some(1), 600), source), ok);
		// Submit another message to another channel with lower priority. Should not be allowed
		// because msg count limit is 1
		assert!(matches!(
			store.submit(statement(1, 1, Some(2), 100), source),
			SubmitResult::Rejected(_)
		));
		assert_eq!(store.evicted_count(), 1);

		// Account 2 (limit = 2 msg, 1000 bytes)

		let s2_prio1 = statement(2, 1, None, 500);
		let s2_prio2 = statement(2, 2, None, 100);
		assert_eq!(store.submit(s2_prio1.clone(), source), ok);
		assert_eq!(store.submit(s2_prio2.clone(), source), ok);
		// Equal priority to lowest should be rejected
		assert!(matches!(
			store.submit(statement(2, 1, None, 50), source),
			SubmitResult::Rejected(RejectionReason::AccountFull { .. })
		));
		// Should evict priority 1
		let s2_prio3 = statement(2, 3, None, 500);
		assert_eq!(store.submit(s2_prio3.clone(), source), ok);
		assert_eq!(store.evicted_count(), 2);
		assert!(store.is_evicted(&s2_prio1.hash()));
		assert!(store.statement(&s2_prio1.hash()).unwrap().is_none());
		// Should evict all
		assert_eq!(store.submit(statement(2, 4, None, 1000), source), ok);
		assert_eq!(store.evicted_count(), 4);
		assert!(store.is_evicted(&s2_prio2.hash()));
		assert!(store.is_evicted(&s2_prio3.hash()));

		// Account 3 (limit = 3 msg, 1000 bytes)

		let s3_prio2 = statement(3, 2, Some(1), 300);
		let s3_prio3 = statement(3, 3, Some(2), 300);
		assert_eq!(store.submit(s3_prio2.clone(), source), ok);
		assert_eq!(store.submit(s3_prio3.clone(), source), ok);
		assert_eq!(store.submit(statement(3, 4, Some(3), 300), source), ok);
		// Should evict 2 and 3
		assert_eq!(store.submit(statement(3, 5, None, 500), source), ok);
		assert_eq!(store.evicted_count(), 6);
		assert!(store.is_evicted(&s3_prio2.hash()));
		assert!(store.is_evicted(&s3_prio3.hash()));

		assert_eq!(store.submit_index.read().total_size, 2400);
		assert_eq!(store.submit_index.read().entries.len(), 4);

		// Should be over the global size limit
		assert!(matches!(
			store.submit(statement(1, 1, None, 700), source),
			SubmitResult::Rejected(_)
		));
		// Should be over the global count limit
		store.submit_index.write().config.max_total_statements = 4;
		assert!(matches!(
			store.submit(statement(1, 1, None, 100), source),
			SubmitResult::Rejected(_)
		));

		let mut expected_statements = vec![
			statement(1, 2, Some(1), 600).hash(),
			statement(2, 4, None, 1000).hash(),
			statement(3, 4, Some(3), 300).hash(),
			statement(3, 5, None, 500).hash(),
		];
		expected_statements.sort();
		let mut statements: Vec<_> =
			store.statements().unwrap().into_iter().map(|(hash, _)| hash).collect();
		statements.sort();
		assert_eq!(expected_statements, statements);
	}

	#[test]
	fn max_statement_size_for_gossiping() {
		let (store, _temp) = test_store();
		store.submit_index.write().config.max_total_size = 42 * crate::MAX_STATEMENT_SIZE;

		assert_eq!(
			store.submit(
				statement(42, 1, Some(1), crate::MAX_STATEMENT_SIZE - 500),
				StatementSource::Local
			),
			SubmitResult::New
		);

		assert!(matches!(
			store.submit(
				statement(42, 2, Some(1), 2 * crate::MAX_STATEMENT_SIZE),
				StatementSource::Local
			),
			SubmitResult::Invalid(_)
		));
	}

	#[test]
	fn expired_statements_are_purged() {
		use super::DEFAULT_PURGE_AFTER_SEC;
		let (mut store, temp) = test_store();
		let mut statement = unsigned_statement(1, 1, Some(3), 100);
		store.set_time(0);
		statement.set_topic(0, topic(4));
		sign_with(&mut statement, 1);
		store.submit(statement.clone(), StatementSource::Network);
		assert_eq!(store.submit_index.read().entries.len(), 1);
		store.remove(&statement.hash()).unwrap();
		assert_eq!(store.submit_index.read().entries.len(), 0);
		assert_eq!(store.submit_index.read().accounts.len(), 0);
		store.set_time(DEFAULT_PURGE_AFTER_SEC + 1);
		store.maintain();
		assert_eq!(store.evicted_count(), 0);
		let keystore = store.keystore.clone();
		drop(store);

		let client = std::sync::Arc::new(TestClient);
		let mut path: std::path::PathBuf = temp.path().into();
		path.push("db");
		let store = Store::new::<Block, TestClient, TestBackend>(
			&path,
			Default::default(),
			client,
			keystore,
			None,
			Box::new(sp_core::testing::TaskExecutor::new()),
		)
		.unwrap();
		assert_eq!(store.statements().unwrap().len(), 0);
		assert_eq!(store.evicted_count(), 0);
	}

	#[test]
	fn posted_clear_decrypts() {
		let (store, _temp) = test_store();
		let public = store
			.keystore
			.ed25519_generate_new(sp_core::crypto::key_types::STATEMENT, None)
			.unwrap();
		let statement1 = statement(1, 1, None, 100);
		let mut statement2 = unsigned_statement(1, 2, None, 0);
		let plain = b"The most valuable secret".to_vec();
		statement2.encrypt(&plain, &public).unwrap();
		sign_with(&mut statement2, 1);
		store.submit(statement1, StatementSource::Network);
		store.submit(statement2, StatementSource::Network);
		let posted_clear = store.posted_clear(&[], public.into()).unwrap();
		assert_eq!(posted_clear, vec![plain]);
	}

	#[test]
	fn broadcasts_stmt_returns_encoded_statements() {
		let (store, _tmp) = test_store();

		// no key, no topic
		let s0 = signed_statement_with_topics(0, &[], None);
		// same, but with a topic = 42
		let s1 = signed_statement_with_topics(1, &[topic(42)], None);
		// has a decryption key -> must NOT be returned by broadcasts_stmt
		let s2 = signed_statement_with_topics(2, &[topic(42)], Some(dec_key(99)));

		for s in [&s0, &s1, &s2] {
			store.submit(s.clone(), StatementSource::Network);
		}

		// no topic filter
		let mut hashes: Vec<_> = store
			.broadcasts_stmt(&[])
			.unwrap()
			.into_iter()
			.map(|bytes| Statement::decode(&mut &bytes[..]).unwrap().hash())
			.collect();
		hashes.sort();
		let expected_hashes = {
			let mut e = vec![s0.hash(), s1.hash()];
			e.sort();
			e
		};
		assert_eq!(hashes, expected_hashes);

		// filter on topic 42
		let got = store.broadcasts_stmt(&[topic(42)]).unwrap();
		assert_eq!(got.len(), 1);
		let st = Statement::decode(&mut &got[0][..]).unwrap();
		assert_eq!(st.hash(), s1.hash());
	}

	#[test]
	fn posted_stmt_returns_encoded_statements_for_dest() {
		let (store, _tmp) = test_store();

		let public1 = store
			.keystore
			.ed25519_generate_new(sp_core::crypto::key_types::STATEMENT, None)
			.unwrap();
		let dest: [u8; 32] = public1.into();

		let public2 = store
			.keystore
			.ed25519_generate_new(sp_core::crypto::key_types::STATEMENT, None)
			.unwrap();

		// A statement that does have dec_key = dest
		let mut s_with_key = unsigned_statement(1, 1, None, 0);
		let plain1 = b"The most valuable secret".to_vec();
		s_with_key.encrypt(&plain1, &public1).unwrap();
		sign_with(&mut s_with_key, 1);

		// A statement with a different dec_key
		let mut s_other_key = unsigned_statement(2, 2, None, 0);
		let plain2 = b"The second most valuable secret".to_vec();
		s_other_key.encrypt(&plain2, &public2).unwrap();
		sign_with(&mut s_other_key, 2);

		// Submit them all
		for s in [&s_with_key, &s_other_key] {
			store.submit(s.clone(), StatementSource::Network);
		}

		// posted_stmt should only return the one with dec_key = dest
		let retrieved = store.posted_stmt(&[], dest).unwrap();
		assert_eq!(retrieved.len(), 1, "Only one statement has dec_key=dest");

		// Re-decode that returned statement to confirm it is correct
		let returned_stmt = Statement::decode(&mut &retrieved[0][..]).unwrap();
		assert_eq!(
			returned_stmt.hash(),
			s_with_key.hash(),
			"Returned statement must match s_with_key"
		);
	}

	#[test]
	fn posted_clear_stmt_returns_statement_followed_by_plain_data() {
		let (store, _tmp) = test_store();

		let public1 = store
			.keystore
			.ed25519_generate_new(sp_core::crypto::key_types::STATEMENT, None)
			.unwrap();
		let dest: [u8; 32] = public1.into();

		let public2 = store
			.keystore
			.ed25519_generate_new(sp_core::crypto::key_types::STATEMENT, None)
			.unwrap();

		// A statement that does have dec_key = dest
		let mut s_with_key = unsigned_statement(1, 1, None, 0);
		let plain1 = b"The most valuable secret".to_vec();
		s_with_key.encrypt(&plain1, &public1).unwrap();
		sign_with(&mut s_with_key, 1);

		// A statement with a different dec_key
		let mut s_other_key = unsigned_statement(2, 2, None, 0);
		let plain2 = b"The second most valuable secret".to_vec();
		s_other_key.encrypt(&plain2, &public2).unwrap();
		sign_with(&mut s_other_key, 2);

		// Submit them all
		for s in [&s_with_key, &s_other_key] {
			store.submit(s.clone(), StatementSource::Network);
		}

		// posted_stmt should only return the one with dec_key = dest
		let retrieved = store.posted_clear_stmt(&[], dest).unwrap();
		assert_eq!(retrieved.len(), 1, "Only one statement has dec_key=dest");

		// We expect: [ encoded Statement ] + [ the decrypted bytes ]
		let encoded_stmt = s_with_key.encode();
		let stmt_len = encoded_stmt.len();

		// 1) statement is first
		assert_eq!(&retrieved[0][..stmt_len], &encoded_stmt[..]);

		// 2) followed by the decrypted payload
		let trailing = &retrieved[0][stmt_len..];
		assert_eq!(trailing, &plain1[..]);
	}

	#[test]
	fn posted_clear_returns_plain_data_for_dest_and_topics() {
		let (store, _tmp) = test_store();

		// prepare two key-pairs
		let public_dest = store
			.keystore
			.ed25519_generate_new(sp_core::crypto::key_types::STATEMENT, None)
			.unwrap();
		let dest: [u8; 32] = public_dest.into();

		let public_other = store
			.keystore
			.ed25519_generate_new(sp_core::crypto::key_types::STATEMENT, None)
			.unwrap();

		// statement that SHOULD be returned (matches dest & topic 42)
		let mut s_good = unsigned_statement(1, 1, None, 0);
		let plaintext_good = b"The most valuable secret".to_vec();
		s_good.encrypt(&plaintext_good, &public_dest).unwrap();
		s_good.set_topic(0, topic(42));
		sign_with(&mut s_good, 1);

		// statement that should NOT be returned (same dest but different topic)
		let mut s_wrong_topic = unsigned_statement(2, 2, None, 0);
		s_wrong_topic.encrypt(b"Wrong topic", &public_dest).unwrap();
		s_wrong_topic.set_topic(0, topic(99));
		sign_with(&mut s_wrong_topic, 2);

		// statement that should NOT be returned (different dest)
		let mut s_other_dest = unsigned_statement(3, 3, None, 0);
		s_other_dest.encrypt(b"Other dest", &public_other).unwrap();
		s_other_dest.set_topic(0, topic(42));
		sign_with(&mut s_other_dest, 3);

		// submit all
		for s in [&s_good, &s_wrong_topic, &s_other_dest] {
			store.submit(s.clone(), StatementSource::Network);
		}

		// call posted_clear with the topic filter and dest
		let retrieved = store.posted_clear(&[topic(42)], dest).unwrap();

		// exactly one element, equal to the expected plaintext
		assert_eq!(retrieved, vec![plaintext_good]);
	}

	#[test]
	fn already_expired_statement_is_rejected() {
		let (mut store, _temp) = test_store();

		// Set current time to 1000 seconds
		store.set_time(1000);

		// Create a statement that has already expired (expiration at 500 seconds, before current
		// time)
		let mut expired_statement = unsigned_statement(1, 1, None, 100);
		// set_expiry_from_parts: first arg is expiration timestamp in seconds, second is priority
		expired_statement.set_expiry_from_parts(500, 1);
		sign_with(&mut expired_statement, 1);

		// Submit should fail with AlreadyExpired
		assert_eq!(
			store.submit(expired_statement, StatementSource::Network),
			SubmitResult::Invalid(InvalidReason::AlreadyExpired)
		);

		// Verify the statement was not added
		assert_eq!(store.statements().unwrap().len(), 0);

		// Now create a statement that is not expired (expiration at 2000 seconds, after current
		// time)
		let mut valid_statement = unsigned_statement(1, 1, None, 100);
		valid_statement.set_expiry_from_parts(2000, 1);
		sign_with(&mut valid_statement, 1);

		// Submit should succeed
		assert_eq!(store.submit(valid_statement, StatementSource::Network), SubmitResult::New);
		assert_eq!(store.statements().unwrap().len(), 1);
	}

	#[test]
	fn remove_by_covers_various_situations() {
		use sp_statement_store::{StatementSource, StatementStore, SubmitResult};

		// Use a fresh store and fixed time so we can control purging.
		let (mut store, _temp) = test_store();
		store.set_time(0);

		// Reuse helpers from this module.
		let t42 = topic(42);
		let k7 = dec_key(7);

		// Account A = 4 (has per-account limits (4, 1000) in the mock runtime)
		// - Mix of topic, decryption-key and channel to exercise every index.
		let mut s_a1 = unsigned_statement(4, 10, Some(100), 100);
		s_a1.set_topic(0, t42);
		sign_with(&mut s_a1, 4);
		let h_a1 = s_a1.hash();

		let mut s_a2 = unsigned_statement(4, 20, Some(200), 150);
		s_a2.set_decryption_key(k7);
		sign_with(&mut s_a2, 4);
		let h_a2 = s_a2.hash();

		let s_a3 = statement(4, 30, None, 50);
		let h_a3 = s_a3.hash();

		// Account B = 3 (control group that must remain untouched).
		let s_b1 = statement(3, 10, None, 100);
		let h_b1 = s_b1.hash();

		let mut s_b2 = unsigned_statement(3, 15, Some(300), 100);
		s_b2.set_topic(0, t42);
		s_b2.set_decryption_key(k7);
		sign_with(&mut s_b2, 3);
		let h_b2 = s_b2.hash();

		// Submit all statements.
		for s in [&s_a1, &s_a2, &s_a3, &s_b1, &s_b2] {
			assert_eq!(store.submit(s.clone(), StatementSource::Network), SubmitResult::New);
		}

		// --- Pre-conditions: everything is indexed as expected.
		{
			let submit_idx = store.submit_index.read();
			assert_eq!(submit_idx.entries.len(), 5, "all 5 should be present");
			assert!(submit_idx.accounts.contains_key(&account(4)));
			assert!(submit_idx.accounts.contains_key(&account(3)));
			assert_eq!(submit_idx.total_size, 100 + 150 + 50 + 100 + 100);
			drop(submit_idx);

			// Topic and key sets contain both A & B entries.
			assert!(store.index_has_topic(&t42, &h_a1) && store.index_has_topic(&t42, &h_b2));
			assert!(
				store.index_has_dec_key(&Some(k7), &h_a2) &&
					store.index_has_dec_key(&Some(k7), &h_b2)
			);
		}

		// --- Action: remove all statements by Account A.
		store.remove_by(account(4)).expect("remove_by should succeed");

		// --- Post-conditions: A's statements are gone and marked expired; B's remain.
		{
			// A's statements removed from DB view.
			for h in [h_a1, h_a2, h_a3] {
				assert!(store.statement(&h).unwrap().is_none(), "A's statement should be removed");
			}

			// B's statements still present.
			for h in [h_b1, h_b2] {
				assert!(store.statement(&h).unwrap().is_some(), "B's statement should remain");
			}

			let submit_idx = store.submit_index.read();
			// Account map updated.
			assert!(!submit_idx.accounts.contains_key(&account(4)), "Account A must be gone");
			assert!(submit_idx.accounts.contains_key(&account(3)), "Account B must remain");
			// Entry count & total_size reflect only B's data.
			assert_eq!(submit_idx.entries.len(), 2);
			assert_eq!(submit_idx.total_size, 100 + 100);
			drop(submit_idx);

			// Removed statements are banned in the on-disk evicted journal.
			assert!(store.is_evicted(&h_a1));
			assert!(store.is_evicted(&h_a2));
			assert!(store.is_evicted(&h_a3));
			assert_eq!(store.evicted_count(), 3);

			// Topic index: only B2 remains for topic 42.
			assert!(store.index_has_topic(&t42, &h_b2));
			assert!(!store.index_has_topic(&t42, &h_a1));

			// Decryption-key index: only B2 remains for key 7.
			assert!(store.index_has_dec_key(&Some(k7), &h_b2));
			assert!(!store.index_has_dec_key(&Some(k7), &h_a2));
		}

		// --- Idempotency: removing again is a no-op and should not error.
		store.remove_by(account(4)).expect("second remove_by should be a no-op");

		// --- Purge: advance time beyond TTL and run maintenance; expired entries disappear.
		let purge_after = store.submit_index.read().config.purge_after_sec;
		store.set_time(purge_after + 1);
		store.maintain();
		assert_eq!(store.evicted_count(), 0, "expired entries should be purged");

		// --- Reuse: Account A can submit again after purge.
		let s_new = statement(4, 40, None, 10);
		assert_eq!(store.submit(s_new, StatementSource::Network), SubmitResult::New);
	}

	#[test]
	fn check_expiration_repopulates_account_list_when_empty() {
		let (mut store, _temp) = test_store();
		store.set_time(1000);

		// Create statements for multiple accounts
		// Note: The statement() helper uses set_expiry_from_parts(u32::MAX, priority)
		// which creates a very large expiry value that won't trigger expiration
		let s1 = statement(1, 1, None, 100);
		let s2 = statement(2, 1, None, 100);
		let s3 = statement(3, 1, None, 100);

		for s in [&s1, &s2, &s3] {
			store.submit(s.clone(), StatementSource::Network);
		}

		// Initially, accounts_to_check_for_expiry_stmts is empty
		assert!(store.submit_index.read().accounts_to_check_for_expiry_stmts.is_empty());

		// First call to check_expiration should populate the list
		store.enforce_limits();

		// Now accounts_to_check_for_expiry_stmts should contain all 3 accounts
		let accounts = store.submit_index.read().accounts_to_check_for_expiry_stmts.clone();
		assert_eq!(accounts.len(), 3, "Should have 3 accounts to check");
		assert!(accounts.contains(&account(1)));
		assert!(accounts.contains(&account(2)));
		assert!(accounts.contains(&account(3)));

		// No statements should have been expired since they're all valid
		assert_eq!(store.evicted_count(), 0);
		assert_eq!(store.submit_index.read().entries.len(), 3);
	}

	#[test]
	fn check_expiration_expires_statements_past_current_time() {
		let (mut store, _temp) = test_store();

		// The check_expiration function compares Expiry(current_time << 32) against
		// Expiry(expiry) where expiry is the full 64-bit value with timestamp in high 32 bits.
		// Statements with expiration timestamp < current_time will be expired.

		store.set_time(100);

		// Create a statement that will expire at timestamp 500
		let mut expired_stmt = unsigned_statement(1, 1, None, 100);
		expired_stmt.set_expiry_from_parts(500, 1);
		sign_with(&mut expired_stmt, 1);
		let expired_hash = expired_stmt.hash();
		store.submit(expired_stmt, StatementSource::Network);

		// Create a statement that won't expire (far future expiry)
		let valid_stmt = statement(2, 1, None, 100); // Uses u32::MAX as timestamp
		let valid_hash = valid_stmt.hash();
		store.submit(valid_stmt, StatementSource::Network);

		// Verify both statements are in the store
		assert_eq!(store.submit_index.read().entries.len(), 2);

		// First check_expiration populates the account list
		store.enforce_limits();
		assert!(!store.submit_index.read().accounts_to_check_for_expiry_stmts.is_empty());

		// Advance time past the expiry of the first statement
		store.set_time(1000);

		// Second check_expiration should find and expire the statement
		store.enforce_limits();

		// Naturally-expired statements are not added to the expired map (AlreadyExpired check
		// in submit rejects them without consulting the map)
		let index = store.submit_index.read();
		assert!(
			!store.is_evicted(&expired_hash),
			"Naturally expired statement must not be added to the expired map"
		);
		assert!(
			!index.entries.contains_key(&expired_hash),
			"Expired statement should be removed from entries"
		);

		// The valid statement should still be in entries
		assert!(
			index.entries.contains_key(&valid_hash),
			"Valid statement should still be in entries"
		);
		assert!(!store.is_evicted(&valid_hash), "Valid statement should not be expired");
	}

	#[test]
	fn check_expiration_removes_checked_accounts_from_list_when_expiring() {
		let (mut store, _temp) = test_store();
		store.set_time(100);

		// Create statements with expiry at timestamp 200
		let mut stmt1 = unsigned_statement(1, 1, None, 100);
		stmt1.set_expiry_from_parts(200, 1);
		sign_with(&mut stmt1, 1);
		store.submit(stmt1, StatementSource::Network);

		let mut stmt2 = unsigned_statement(2, 1, None, 100);
		stmt2.set_expiry_from_parts(200, 1);
		sign_with(&mut stmt2, 2);
		store.submit(stmt2, StatementSource::Network);

		let mut stmt3 = unsigned_statement(3, 1, None, 100);
		stmt3.set_expiry_from_parts(200, 1);
		sign_with(&mut stmt3, 3);
		store.submit(stmt3, StatementSource::Network);

		// First call populates the list
		store.enforce_limits();
		assert_eq!(
			store.submit_index.read().accounts_to_check_for_expiry_stmts.len(),
			3,
			"Should have 3 accounts to check"
		);

		// Advance time past expiry
		store.set_time(300);

		// Second call should check accounts, expire statements, and remove checked accounts
		store.enforce_limits();

		// The list should now be empty (all accounts checked and removed)
		assert!(
			store.submit_index.read().accounts_to_check_for_expiry_stmts.is_empty(),
			"All accounts should have been checked and removed after expiration"
		);

		// All statements were naturally expired (past their own timestamp), so they are not
		// added to the expired map AlreadyExpired check in submit handles re-gossip prevention
		assert_eq!(store.evicted_count(), 0);
		assert_eq!(store.submit_index.read().entries.len(), 0);
	}

	#[test]
	fn check_expiration_truncates_list_even_when_nothing_expires() {
		let (mut store, _temp) = test_store();
		store.set_time(1000);

		// Create statements for multiple accounts with far future expiry (using statement helper)
		// The statement() helper uses set_expiry_from_parts(u32::MAX, priority) which creates
		// a very large expiry value that won't trigger expiration
		for acc_id in 1..=5u64 {
			let stmt = statement(acc_id, 1, None, 100);
			store.submit(stmt, StatementSource::Network);
		}

		// First call populates the list
		store.enforce_limits();
		assert_eq!(store.submit_index.read().accounts_to_check_for_expiry_stmts.len(), 5);

		// Second call checks accounts and truncates the list (even though nothing expires)
		store.enforce_limits();

		// The list should now be empty - accounts are removed after being checked
		assert!(
			store.submit_index.read().accounts_to_check_for_expiry_stmts.is_empty(),
			"List should be empty after all accounts have been checked"
		);

		// No statements should have been expired
		assert_eq!(store.evicted_count(), 0);
		assert_eq!(store.submit_index.read().entries.len(), 5);
	}

	#[test]
	fn check_expiration_handles_multiple_statements_per_account() {
		let (mut store, _temp) = test_store();
		store.set_time(100);

		// Create multiple statements for the same account with different expiry timestamps
		// Account 42 has limit of 42 statements
		let mut stmt1 = unsigned_statement(42, 1, Some(1), 100);
		stmt1.set_expiry_from_parts(200, 1); // Expires at timestamp 200
		sign_with(&mut stmt1, 42);
		let hash1 = stmt1.hash();
		store.submit(stmt1, StatementSource::Network);

		let mut stmt2 = unsigned_statement(42, 2, Some(2), 100);
		stmt2.set_expiry_from_parts(300, 2); // Expires at timestamp 300
		sign_with(&mut stmt2, 42);
		let hash2 = stmt2.hash();
		store.submit(stmt2, StatementSource::Network);

		let mut stmt3 = unsigned_statement(42, 3, Some(3), 100);
		stmt3.set_expiry_from_parts(500, 3); // Expires at timestamp 500
		sign_with(&mut stmt3, 42);
		let hash3 = stmt3.hash();
		store.submit(stmt3, StatementSource::Network);

		// Verify all statements are in the store
		assert_eq!(store.submit_index.read().entries.len(), 3);

		// First check_expiration populates the account list
		store.enforce_limits();

		// Advance time to 250 (stmt1 should expire since 250 > 200)
		store.set_time(250);
		store.enforce_limits();

		{
			let index = store.submit_index.read();
			// Naturally expired statements are not added to the expired map.
			assert!(!store.is_evicted(&hash1), "stmt1 naturally expired, not in map");
			assert!(!store.is_evicted(&hash2), "stmt2 should not be expired yet");
			assert!(!store.is_evicted(&hash3), "stmt3 should not be expired yet");
			assert_eq!(index.entries.len(), 2);
		}

		// Repopulate the account list for next check
		store.enforce_limits();

		// Advance time to 400 (stmt2 should also expire since 400 > 300)
		store.set_time(400);
		store.enforce_limits();

		{
			let index = store.submit_index.read();
			assert!(!store.is_evicted(&hash1));
			assert!(!store.is_evicted(&hash2), "stmt2 naturally expired, not in map");
			assert!(!store.is_evicted(&hash3), "stmt3 should not be expired yet");
			assert_eq!(index.entries.len(), 1);
		}

		// Repopulate and check again at time 600 (stmt3 should expire since 600 > 500)
		store.enforce_limits();
		store.set_time(600);
		store.enforce_limits();

		{
			let index = store.submit_index.read();
			assert!(!store.is_evicted(&hash1));
			assert!(!store.is_evicted(&hash2));
			assert!(!store.is_evicted(&hash3), "stmt3 naturally expired, not in map");
			assert_eq!(index.entries.len(), 0);
		}
	}

	#[test]
	fn check_expiration_does_nothing_when_no_expired_statements() {
		let (mut store, _temp) = test_store();
		store.set_time(1000);

		// Create statement with expiry far in the future
		// The statement() helper uses set_expiry_from_parts(u32::MAX, priority)
		let stmt = statement(1, 1, None, 100);
		let hash = stmt.hash();
		store.submit(stmt, StatementSource::Network);

		// Populate the account list
		store.enforce_limits();

		// Check expiration - nothing should happen
		store.enforce_limits();

		// Statement should still be there
		let index = store.submit_index.read();
		assert!(index.entries.contains_key(&hash));
		assert!(!store.is_evicted(&hash));
		assert_eq!(index.entries.len(), 1);
		assert_eq!(store.evicted_count(), 0);
	}

	#[test]
	fn check_expiration_correctly_updates_account_data() {
		let (mut store, _temp) = test_store();
		store.set_time(100);

		// Create a statement with expiry at timestamp 200
		let mut stmt = unsigned_statement(1, 1, Some(1), 100);
		stmt.set_expiry_from_parts(200, 1);
		sign_with(&mut stmt, 1);
		let hash = stmt.hash();
		store.submit(stmt, StatementSource::Network);

		// Verify account exists before expiration
		{
			let index = store.submit_index.read();
			assert!(index.accounts.contains_key(&account(1)));
			assert_eq!(index.total_size, 100);
		}

		// Populate and then expire
		store.enforce_limits();
		store.set_time(300);
		store.enforce_limits();

		// Verify account is removed after its only statement expires
		{
			let index = store.submit_index.read();
			assert!(
				!index.accounts.contains_key(&account(1)),
				"Account should be removed when all its statements expire"
			);
			assert_eq!(index.total_size, 0, "Total size should be zero");
			assert!(!store.is_evicted(&hash), "Naturally expired, not in map");
		}
	}

	#[test]
	fn check_expiration_clears_topic_and_key_indexes() {
		let (mut store, _temp) = test_store();
		store.set_time(100);

		// Create a statement with topic and decryption key
		let mut stmt = unsigned_statement(1, 1, Some(1), 100);
		stmt.set_expiry_from_parts(200, 1);
		stmt.set_topic(0, topic(42));
		stmt.set_decryption_key(dec_key(7));
		sign_with(&mut stmt, 1);
		let hash = stmt.hash();
		store.submit(stmt, StatementSource::Network);

		// Verify indexes are populated
		{
			assert!(store.index_has_topic(&topic(42), &hash));
			assert!(store.index_has_dec_key(&Some(dec_key(7)), &hash));
		}

		// Populate and then expire
		store.enforce_limits();
		store.set_time(300);
		store.enforce_limits();

		// Verify indexes are cleared
		{
			assert!(!store.index_has_topic(&topic(42), &hash), "Topic index should be cleared");
			assert!(
				!store.index_has_dec_key(&Some(dec_key(7)), &hash),
				"Decryption key index should be cleared"
			);
			assert!(!store.is_evicted(&hash), "Naturally expired, not in map");
		}
	}

	#[test]
	fn check_expiration_handles_empty_store() {
		let (mut store, _temp) = test_store();
		store.set_time(1000);

		// With no statements, check_expiration should not panic
		store.enforce_limits();

		// Second call should also work (empty repopulation)
		store.enforce_limits();

		assert!(store.submit_index.read().accounts_to_check_for_expiry_stmts.is_empty());
		assert_eq!(store.submit_index.read().entries.len(), 0);
		assert_eq!(store.evicted_count(), 0);
	}

	#[test]
	fn check_expiration_expires_properly_formatted_statements() {
		// With the fix (Expiry(current_time << 32)), check_expiration properly
		// compares timestamps and can expire statements submitted through normal flow.

		let (mut store, _temp) = test_store();
		store.set_time(1000);

		// Create a statement with expiration timestamp just 1 second in the future
		let mut stmt = unsigned_statement(1, 1, None, 100);
		stmt.set_expiry_from_parts(1001, 1); // Expires at timestamp 1001
		sign_with(&mut stmt, 1);
		let hash = stmt.hash();
		store.submit(stmt, StatementSource::Network);

		assert_eq!(store.submit_index.read().entries.len(), 1);

		// Populate the accounts list
		store.enforce_limits();

		// Advance time past the expiration timestamp
		store.set_time(2000);
		store.enforce_limits();

		// Statement SHOULD be expired because check_expiration now compares
		// Expiry(2000 << 32) against Expiry(1001 << 32 | 1), and
		// (2000 << 32) > (1001 << 32 | 1)
		let index = store.submit_index.read();
		assert!(
			!index.entries.contains_key(&hash),
			"Statement should be removed from entries after expiration"
		);
		// Naturally expired: timestamp 1001 < current_time 2000, not added to expired map.
		assert!(!store.is_evicted(&hash), "Naturally expired, not in map");
	}

	#[test]
	fn check_expiration_updates_database_columns() {
		// This test verifies that check_expiration properly updates the database.
		let (mut store, _temp) = test_store();
		store.set_time(100);

		// Create a statement with expiry at timestamp 200
		let mut stmt = unsigned_statement(1, 1, None, 100);
		stmt.set_expiry_from_parts(200, 1);
		sign_with(&mut stmt, 1);
		let hash = stmt.hash();
		store.submit(stmt.clone(), StatementSource::Network);

		// Verify statement is in the database
		let db_entry = store.db.get(col::STATEMENTS, &hash).unwrap();
		assert!(db_entry.is_some(), "Statement should be in col::STATEMENTS after submit");

		// Populate the accounts list
		store.enforce_limits();

		// Advance time past expiry and run check_expiration
		store.set_time(300);
		store.enforce_limits();

		// Verify in-memory state is updated correctly
		{
			let index = store.submit_index.read();
			assert!(
				!index.entries.contains_key(&hash),
				"Statement should be removed from in-memory entries"
			);
			// Naturally expired: not added to expired map, no need for suppression.
			assert!(
				!store.is_evicted(&hash),
				"Naturally expired statement must not be in the expired map"
			);
		}

		let db_entry = store.db.get(col::STATEMENTS, &hash).unwrap();
		assert!(
			db_entry.is_none(),
			"Statement should be removed from col::STATEMENTS after expiration"
		);

		// Naturally expired statements are not written to col::EXPIRED either, so that
		// the optimization survives node restarts.
		let expired_entry = store.db.get(col::EXPIRED, &hash).unwrap();
		assert!(expired_entry.is_none(), "Naturally expired: not written to col::EXPIRED");
	}

	#[test]
	fn enforce_allowances_evicts_excess_statements() {
		// This test verifies that check_expiration correctly evicts statements
		// when statements exceed the current allowance. We directly insert into
		// the index (bypassing submit's validation) to simulate statements that
		// existed before allowances were reduced.
		let (mut store, _temp) = test_store();
		store.set_time(0);

		// Account 4 has allowance (4 statements, 1000 bytes) from TestClient
		let s1 = statement(4, 10, None, 100); // lowest priority - will be evicted
		let s2 = statement(4, 20, None, 100);
		let s3 = statement(4, 30, None, 100);
		let s4 = statement(4, 40, None, 100);
		let s5 = statement(4, 50, None, 100); // highest priority

		let h1 = s1.hash();
		let h5 = s5.hash();

		// Directly insert into index, bypassing `submit`'s allowance check
		{
			let mut submit_index = store.submit_index.write();
			for s in [&s1, &s2, &s3, &s4, &s5] {
				submit_index.insert_new(s.hash(), account(4), s);
			}
		}

		// Verify initial state - all 5 should be present
		assert_eq!(store.submit_index.read().entries.len(), 5);
		assert_eq!(store.submit_index.read().total_size, 500);

		// Run check_expiration which handles both expiration and allowance enforcement
		// First call populates the accounts list, second call processes them
		// Since account 4 has max_count=4, one statement should be evicted
		store.enforce_limits();
		store.enforce_limits();

		// Should evict the lowest priority statement (s1)
		let index = store.submit_index.read();
		assert_eq!(index.entries.len(), 4, "Should have 4 statements after eviction");
		assert!(!index.entries.contains_key(&h1), "Lowest priority should be evicted");
		assert!(index.entries.contains_key(&h5), "Highest priority should remain");
		assert_eq!(index.total_size, 400);

		// Evicted statement should be marked as expired
		assert!(store.is_evicted(&h1));
	}

	#[test]
	fn enforce_allowances_evicts_all_when_no_allowance_found() {
		let (mut store, _temp) = test_store();
		store.set_time(0);

		// Account 0 has NO allowance in TestClient
		let s1 = statement(0, 10, None, 100);
		let s2 = statement(0, 20, None, 150);

		let h1 = s1.hash();
		let h2 = s2.hash();

		// Directly insert statements for account with no allowance
		{
			let mut submit_index = store.submit_index.write();
			submit_index.insert_new(h1, account(0), &s1);
			submit_index.insert_new(h2, account(0), &s2);
		}

		assert_eq!(store.submit_index.read().entries.len(), 2);

		// Run check_expiration - should evict ALL statements since no allowance exists
		// First call populates the accounts list, second call processes them
		store.enforce_limits();
		store.enforce_limits();

		let index = store.submit_index.read();
		assert_eq!(index.entries.len(), 0, "All statements should be evicted");
		assert!(!index.accounts.contains_key(&account(0)), "Account should be removed");
		assert!(store.is_evicted(&h1));
		assert!(store.is_evicted(&h2));
	}

	#[test]
	fn enforce_allowances_based_on_size() {
		// This test verifies that check_expiration evicts based on size limits.
		let (mut store, _temp) = test_store();
		store.set_time(0);

		// Account 2 has allowance (2, 1000) from TestClient
		// Insert 2 statements that together exceed 1000 bytes
		let s1 = statement(2, 10, None, 600); // lowest priority
		let s2 = statement(2, 20, None, 600); // higher priority

		let h1 = s1.hash();
		let h2 = s2.hash();

		// Directly insert both statements (total 1200 bytes > 1000 limit)
		{
			let mut submit_index = store.submit_index.write();
			submit_index.insert_new(h1, account(2), &s1);
			submit_index.insert_new(h2, account(2), &s2);
		}

		assert_eq!(store.submit_index.read().total_size, 1200);

		// Run check_expiration - should evict s1 to get under 1000 bytes
		// First call populates the accounts list, second call processes them
		store.enforce_limits();
		store.enforce_limits();

		let index = store.submit_index.read();
		assert_eq!(index.entries.len(), 1);
		assert!(index.entries.contains_key(&h2), "Higher priority should remain");
		assert!(!index.entries.contains_key(&h1), "Lower priority should be evicted");
		assert_eq!(index.total_size, 600);
	}

	#[test]
	fn channel_replacement_only_higher_priority_succeeds() {
		let (store, _temp) = test_store();
		let source = StatementSource::Network;

		// Account 1: max_count=1, max_size=1000
		// Submit channel 1 with priority 5
		let s1 = statement(1, 5, Some(1), 100);
		let h1 = s1.hash();
		assert_eq!(store.submit(s1, source), SubmitResult::New);

		// Lower priority on same channel → ChannelPriorityTooLow
		let result = store.submit(statement(1, 3, Some(1), 100), source);
		assert!(
			matches!(result, SubmitResult::Rejected(RejectionReason::ChannelPriorityTooLow { .. })),
			"Lower priority should be rejected with ChannelPriorityTooLow, got: {result:?}"
		);

		// Equal priority on same channel → ChannelPriorityTooLow (check is <=)
		// Use different data_len to get a distinct hash with same priority
		let result = store.submit(statement(1, 5, Some(1), 101), source);
		assert!(
			matches!(result, SubmitResult::Rejected(RejectionReason::ChannelPriorityTooLow { .. })),
			"Equal priority should be rejected with ChannelPriorityTooLow, got: {result:?}"
		);

		// Higher priority on same channel → replaces
		let s2 = statement(1, 10, Some(1), 200);
		let h2 = s2.hash();
		assert_eq!(store.submit(s2, source), SubmitResult::New);

		{
			let index = store.submit_index.read();
			assert_eq!(index.entries.len(), 1);
			assert!(!index.entries.contains_key(&h1), "Old channel message should be gone");
			assert!(index.entries.contains_key(&h2), "New channel message should exist");
			assert!(store.is_evicted(&h1), "Old should be in expired");
			assert_eq!(index.total_size, 200);
		}
	}

	#[test]
	fn submit_rejects_malformed_statements() {
		let (store, _temp) = test_store();

		let mut base = Statement::new();
		base.set_expiry(u64::MAX);
		base.set_plain_data(vec![1]);

		let ed_kp = sp_core::ed25519::Pair::from_string("//Alice", None).unwrap();
		let sr_kp = sp_core::sr25519::Pair::from_string("//Alice", None).unwrap();
		let ecdsa_kp = sp_core::ecdsa::Pair::from_string("//Alice", None).unwrap();

		assert_eq!(
			store.submit(base.clone(), StatementSource::Network),
			SubmitResult::Invalid(InvalidReason::NoProof)
		);

		let bad_proofs = [
			Proof::Ed25519 { signature: [0xAB; 64], signer: ed_kp.public().0 },
			Proof::Sr25519 { signature: [0xCD; 64], signer: sr_kp.public().0 },
			Proof::Secp256k1Ecdsa { signature: [0xEF; 65], signer: ecdsa_kp.public().0 },
		];
		for proof in bad_proofs {
			let mut s = base.clone();
			s.set_proof(proof);
			assert_eq!(
				store.submit(s, StatementSource::Network),
				SubmitResult::Invalid(InvalidReason::BadProof)
			);
		}

		let mut wrong_signer = base.clone();
		wrong_signer.sign_ed25519_private(&ed_kp);
		let alice_sig = match wrong_signer.proof().unwrap() {
			Proof::Ed25519 { signature, .. } => *signature,
			_ => panic!("expected Ed25519 proof after sign_ed25519_private"),
		};
		let bob_kp = sp_core::ed25519::Pair::from_string("//Bob", None).unwrap();
		wrong_signer.set_proof(Proof::Ed25519 { signature: alice_sig, signer: bob_kp.public().0 });
		assert_eq!(
			store.submit(wrong_signer, StatementSource::Network),
			SubmitResult::Invalid(InvalidReason::BadProof)
		);
	}

	#[test]
	fn channel_replacement_with_size_increase_evicts_others() {
		let (store, _temp) = test_store();
		let source = StatementSource::Network;

		// Account 3: max_count=3, max_size=1000
		// channel msg (200b) + two non-channel msgs (300b each) = 800b
		let s_ch = statement(3, 5, Some(1), 200);
		let s_low = statement(3, 2, None, 300);
		let s_mid = statement(3, 3, None, 300);
		let h_ch = s_ch.hash();
		let h_low = s_low.hash();
		let h_mid = s_mid.hash();

		assert_eq!(store.submit(s_ch, source), SubmitResult::New);
		assert_eq!(store.submit(s_low, source), SubmitResult::New);
		assert_eq!(store.submit(s_mid, source), SubmitResult::New);
		assert_eq!(store.submit_index.read().total_size, 800);

		// Replace channel with 600b message (priority 10 > 5)
		// Must evict lowest priority non-channel statement (priority 2) to fit
		let s_ch_big = statement(3, 10, Some(1), 600);
		let h_ch_big = s_ch_big.hash();
		assert_eq!(store.submit(s_ch_big, source), SubmitResult::New);

		{
			let index = store.submit_index.read();
			assert_eq!(index.entries.len(), 2);
			assert!(!index.entries.contains_key(&h_ch), "Old channel message replaced");
			assert!(!index.entries.contains_key(&h_low), "Priority 2 evicted to fit size");
			assert!(index.entries.contains_key(&h_mid), "Priority 3 should remain");
			assert!(index.entries.contains_key(&h_ch_big), "New channel message added");
			assert_eq!(index.total_size, 900); // 300 (mid) + 600 (new channel)
		}
	}

	#[test]
	fn subscription_reconnect_receives_current_state() {
		use crate::StatementStoreSubscriptionApi;
		use sp_statement_store::OptimizedTopicFilter;

		let (store, _temp) = test_store();
		let source = StatementSource::Local;

		// Submit 3 statements
		for i in 0..3u8 {
			let res = store.submit(signed_statement(i), source);
			assert_eq!(res, SubmitResult::New);
		}

		// First subscribe → should get 3 existing statements
		let (existing, sender, stream) =
			store.subscribe_statement(OptimizedTopicFilter::Any).unwrap();
		assert_eq!(existing.len(), 3, "First subscribe should return 3 existing statements");

		// Drop stream
		drop(stream);
		drop(sender);

		// Submit 2 more while disconnected
		for i in 3..5u8 {
			assert_eq!(store.submit(signed_statement(i), source), SubmitResult::New);
		}
		let (existing, sender, stream) =
			store.subscribe_statement(OptimizedTopicFilter::Any).unwrap();
		assert_eq!(existing.len(), 5, "Re-subscribe should return all 5 current statements");

		// Drop and remove one statement
		drop(stream);
		drop(sender);
		let hash_to_remove = signed_statement(0).hash();
		store.remove(&hash_to_remove).unwrap();

		// Re-subscribe → should get 4
		let (existing, _sender, _stream) =
			store.subscribe_statement(OptimizedTopicFilter::Any).unwrap();
		assert_eq!(existing.len(), 4, "Re-subscribe after removal should return 4 statements");
	}

	#[test]
	fn subscription_reconnect_with_topic_filter() {
		use crate::StatementStoreSubscriptionApi;
		use sp_statement_store::OptimizedTopicFilter;

		let (store, _temp) = test_store();
		let source = StatementSource::Local;
		let topic_a = topic(1);
		let topic_b = topic(2);

		// s1: topic A only
		let s1 = signed_statement_with_topics(1, &[topic_a], None);
		// s2: topic B only
		let s2 = signed_statement_with_topics(2, &[topic_b], None);
		// s3: topics A + B
		let s3 = signed_statement_with_topics(3, &[topic_a, topic_b], None);

		assert_eq!(store.submit(s1, source), SubmitResult::New);
		assert_eq!(store.submit(s2, source), SubmitResult::New);
		assert_eq!(store.submit(s3, source), SubmitResult::New);

		// Subscribe with MatchAll([A]) → s1, s3
		let filter_a = OptimizedTopicFilter::MatchAll(std::collections::HashSet::from([topic_a]));
		let (existing, sender, stream) = store.subscribe_statement(filter_a.clone()).unwrap();
		assert_eq!(existing.len(), 2, "MatchAll([A]) should match s1 and s3");

		// Drop and add s4 with topic A
		drop(sender);
		drop(stream);
		let s4 = signed_statement_with_topics(4, &[topic_a], None);
		assert_eq!(store.submit(s4, source), SubmitResult::New);
		// Re-subscribe with same filter → s1, s3, s4
		let (existing, sender, stream) = store.subscribe_statement(filter_a).unwrap();
		assert_eq!(existing.len(), 3, "Re-subscribe MatchAll([A]) should return s1, s3, s4");

		// Drop and re-subscribe with different filter MatchAll([B]) → s2, s3
		drop(sender);
		drop(stream);
		let filter_b = OptimizedTopicFilter::MatchAll(std::collections::HashSet::from([topic_b]));
		let (existing, _sender, _stream) = store.subscribe_statement(filter_b).unwrap();
		assert_eq!(existing.len(), 2, "Re-subscribe MatchAll([B]) should return s2 and s3");
	}

	#[tokio::test]
	async fn subscription_delivers_each_statement_exactly_once_across_boundary() {
		// Exactly-once: a statement existing before the subscription is delivered only through the
		// initial snapshot, and a statement submitted afterwards only through the live stream —
		// never both (which was the at-least-once regression) and never neither.
		use crate::StatementStoreSubscriptionApi;
		use futures::StreamExt;
		use sp_statement_store::{OptimizedTopicFilter, StatementEvent};

		let (store, _temp) = test_store();
		let source = StatementSource::Local;

		// Two statements exist before the subscription is created.
		let a = signed_statement(0);
		let b = signed_statement(1);
		assert_eq!(store.submit(a.clone(), source), SubmitResult::New);
		assert_eq!(store.submit(b.clone(), source), SubmitResult::New);

		// The snapshot must contain exactly the pre-existing statements.
		let (existing, _sender, mut stream) =
			store.subscribe_statement(OptimizedTopicFilter::Any).unwrap();
		let mut snapshot: Vec<Statement> = existing
			.iter()
			.map(|bytes| Statement::decode(&mut &bytes[..]).unwrap())
			.collect();
		snapshot.sort_by_key(|s| s.hash());
		let mut expected_snapshot = vec![a.clone(), b.clone()];
		expected_snapshot.sort_by_key(|s| s.hash());
		assert_eq!(
			snapshot, expected_snapshot,
			"snapshot must contain exactly the pre-existing statements"
		);

		// A statement submitted after the subscription must arrive on the live stream, exactly
		// once.
		let c = signed_statement(2);
		assert_eq!(store.submit(c.clone(), source), SubmitResult::New);

		let event = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
			.await
			.expect("live statement should arrive within the timeout")
			.expect("stream should yield an event");
		let StatementEvent::NewStatements { statements, .. } = event;
		let live: Vec<Statement> = statements
			.iter()
			.map(|bytes| Statement::decode(&mut &bytes.0[..]).unwrap())
			.collect();
		assert_eq!(
			live,
			vec![c.clone()],
			"live stream must deliver exactly the post-subscribe statement"
		);

		// No duplicate: neither the new statement nor the snapshot statements are delivered again.
		assert!(
			tokio::time::timeout(std::time::Duration::from_millis(500), stream.next())
				.await
				.is_err(),
			"no further (duplicate) statements must be delivered"
		);
	}

	#[test]
	fn subscription_snapshot_deduplicates_multi_topic_match_any() {
		// A `MatchAny` snapshot must contain a statement matching several of the filter's topics
		// only once, not once per matching topic.
		use crate::StatementStoreSubscriptionApi;
		use sp_statement_store::OptimizedTopicFilter;

		let (store, _temp) = test_store();
		let topic_a = topic(1);
		let topic_b = topic(2);

		// A statement carrying BOTH topics.
		let s = signed_statement_with_topics(1, &[topic_a, topic_b], None);
		assert_eq!(store.submit(s, StatementSource::Local), SubmitResult::New);

		let filter =
			OptimizedTopicFilter::MatchAny(std::collections::HashSet::from([topic_a, topic_b]));
		let (existing, _sender, _stream) = store.subscribe_statement(filter).unwrap();
		assert_eq!(
			existing.len(),
			1,
			"MatchAny snapshot must not duplicate a multi-topic statement"
		);
	}

	#[tokio::test]
	async fn subscription_match_all_delivers_exactly_once_across_boundary() {
		// The `MatchAll` snapshot is enumerated authoritatively from disk, so a matching statement
		// present before the subscription is delivered via the snapshot (never lost to the
		// in-memory counters/cache lagging a commit), while one submitted afterwards is delivered
		// live exactly once.
		use crate::StatementStoreSubscriptionApi;
		use futures::StreamExt;
		use sp_statement_store::{OptimizedTopicFilter, StatementEvent};

		let (store, _temp) = test_store();
		let source = StatementSource::Local;
		let t = topic(7);

		// Two matching statements exist before the subscription.
		let a = signed_statement_with_topics(0, &[t], None);
		let b = signed_statement_with_topics(1, &[t], None);
		assert_eq!(store.submit(a, source), SubmitResult::New);
		assert_eq!(store.submit(b, source), SubmitResult::New);

		let filter = OptimizedTopicFilter::MatchAll(std::collections::HashSet::from([t]));
		let (existing, _sender, mut stream) = store.subscribe_statement(filter).unwrap();
		assert_eq!(
			existing.len(),
			2,
			"MatchAll snapshot must contain both pre-existing statements"
		);

		// A matching statement submitted afterwards must arrive live, exactly once.
		let c = signed_statement_with_topics(2, &[t], None);
		assert_eq!(store.submit(c.clone(), source), SubmitResult::New);

		let event = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
			.await
			.expect("live statement should arrive within the timeout")
			.expect("stream should yield an event");
		let StatementEvent::NewStatements { statements, .. } = event;
		let live: Vec<Statement> = statements
			.iter()
			.map(|bytes| Statement::decode(&mut &bytes.0[..]).unwrap())
			.collect();
		assert_eq!(
			live,
			vec![c],
			"MatchAll live stream must deliver exactly the post-subscribe statement"
		);

		assert!(
			tokio::time::timeout(std::time::Duration::from_millis(500), stream.next())
				.await
				.is_err(),
			"no further (duplicate) statements must be delivered"
		);
	}

	#[tokio::test]
	async fn subscription_no_loss_or_duplicate_under_concurrent_submits() {
		// Race a stream of submissions against the subscription registration: whatever the
		// interleaving, every matching statement must be delivered exactly once across the snapshot
		// and the live stream — no loss (the bug the on-disk `MatchAll` snapshot could cause) and
		// no duplicate (the original at-least-once regression). This assertion holds for any
		// timing, so the test is deterministic even though the race window is hit
		// non-deterministically.
		use crate::StatementStoreSubscriptionApi;
		use futures::StreamExt;
		use sp_statement_store::{OptimizedTopicFilter, StatementEvent};
		use std::collections::HashSet;

		let (store, _temp) = test_store();
		let store = std::sync::Arc::new(store);
		let t = topic(9);
		// Keep well under SUBSCRIPTION_BUFFER_SIZE (128) so the live channel never overflows (which
		// would auto-unsubscribe and legitimately drop statements).
		const N: u8 = 60;

		let all: Vec<Statement> =
			(0..N).map(|i| signed_statement_with_topics(i, &[t], None)).collect();
		let all_hashes: HashSet<_> = all.iter().map(|s| s.hash()).collect();

		// A handful exist before subscribing; the rest are submitted concurrently with the
		// subscribe.
		let split = 10usize;
		for s in &all[..split] {
			assert_eq!(store.submit(s.clone(), StatementSource::Local), SubmitResult::New);
		}
		let store2 = store.clone();
		let rest: Vec<Statement> = all[split..].to_vec();
		let submitter = std::thread::spawn(move || {
			for s in rest {
				let _ = store2.submit(s, StatementSource::Local);
			}
		});

		let filter = OptimizedTopicFilter::MatchAll(HashSet::from([t]));
		let (existing, _sender, mut stream) = store.subscribe_statement(filter).unwrap();

		submitter.join().unwrap();

		// Everything delivered so far, snapshot first.
		let mut seen = existing
			.iter()
			.map(|b| Statement::decode(&mut &b[..]).unwrap().hash())
			.collect::<Vec<_>>();
		// Drain the live stream until every statement is accounted for (or a timeout on loss).
		while seen.len() < N as usize {
			match tokio::time::timeout(std::time::Duration::from_secs(5), stream.next()).await {
				Ok(Some(StatementEvent::NewStatements { statements, .. })) => {
					for b in statements {
						seen.push(Statement::decode(&mut &b.0[..]).unwrap().hash());
					}
				},
				_ => break,
			}
		}

		let seen_set: HashSet<_> = seen.iter().copied().collect();
		assert_eq!(
			seen.len(),
			N as usize,
			"each statement must be delivered exactly once (got {} deliveries for {} statements — loss or duplicate)",
			seen.len(),
			N
		);
		assert_eq!(seen_set, all_hashes, "delivered set must equal submitted set");
	}

	// Tests for the multi-filter subscription API (`MultiFilterSubscriptionApi` /
	// `create_subscription`), as opposed to the single-filter `subscribe_statement` tests above.
	mod multi_filter {
		use super::*;
		use crate::{
			MultiFilterEventStream, MultiFilterSubscriptionApi, MultiFilterSubscriptionEvent,
		};
		use futures::StreamExt;
		use sp_statement_store::OptimizedTopicFilter;
		use std::{collections::HashSet, sync::Arc, time::Duration};

		fn arc_test_store() -> (Arc<Store>, tempfile::TempDir) {
			let (store, dir) = test_store();
			(Arc::new(store), dir)
		}

		async fn drain_all(
			stream: &mut MultiFilterEventStream,
			idle: Duration,
		) -> Vec<MultiFilterSubscriptionEvent> {
			let mut events = Vec::new();
			while let Ok(Some(event)) = tokio::time::timeout(idle, stream.next()).await {
				events.push(event);
			}
			events
		}

		#[tokio::test]
		async fn add_filter_replays_snapshot_and_delivers_later_submissions_live() {
			let (store, _dir) = arc_test_store();
			let (handle, mut stream) = store.create_subscription();

			const NUM_STATEMENTS: u8 = 50;
			const NUM_PRE_FILTER: u8 = 20;

			// Statements stored before the filter is attached; the replay snapshot must cover
			// exactly these.
			for i in 0..NUM_PRE_FILTER {
				let stmt = signed_statement(i);
				assert_eq!(store.submit(stmt, StatementSource::Local), SubmitResult::New);
			}

			let filter_id = handle.add_filter(OptimizedTopicFilter::Any).unwrap();

			// The snapshot is collected atomically with the filter registration, so statements
			// submitted after `add_filter` returns must arrive as live events only.
			for i in NUM_PRE_FILTER..NUM_STATEMENTS {
				let stmt = signed_statement(i);
				assert_eq!(store.submit(stmt, StatementSource::Local), SubmitResult::New);
			}

			let mut snapshot_hashes: HashSet<[u8; 32]> = HashSet::new();
			let mut live_with_filter: HashSet<[u8; 32]> = HashSet::new();
			for event in drain_all(&mut stream, Duration::from_millis(300)).await {
				match event {
					MultiFilterSubscriptionEvent::ReplayStatements { statements, .. } => {
						snapshot_hashes.extend(
							statements
								.iter()
								.map(|bytes| Statement::decode(&mut &bytes[..]).unwrap().hash()),
						);
					},
					MultiFilterSubscriptionEvent::NewStatement(event)
						if event.matched_filter_ids.contains(&filter_id) =>
					{
						live_with_filter.insert(event.hash);
					},
					_ => {},
				}
			}

			let expected_replayed: HashSet<[u8; 32]> =
				(0..NUM_PRE_FILTER).map(|i| signed_statement(i).hash()).collect();
			let expected_live: HashSet<[u8; 32]> =
				(NUM_PRE_FILTER..NUM_STATEMENTS).map(|i| signed_statement(i).hash()).collect();

			assert_eq!(snapshot_hashes, expected_replayed);
			assert_eq!(live_with_filter, expected_live);
		}
	}
}
