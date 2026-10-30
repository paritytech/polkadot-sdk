// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Fuzz harness for the storage-root PoV size estimation.
//!
//! A [`Script`] describes an initial state and a sequence of blocks shaped like real block
//! execution. [`run_script`] executes it through the real storage host functions
//! ([`sp_io::storage`], [`sp_io::default_child_storage`] and
//! [`storage_proof_size::storage_proof_size`], preceded by
//! `Externalities::record_proof_for_dirty_keys` as in version 2 of the host function) and checks
//! that the final `storage_root` never records a trie node that the last estimation did not
//! already record.
//!
//! Hard invariants (violations panic in [`run_script`]):
//!
//! - **No overshoot**: after the final estimation `e_last` and `sp_io::storage::root`, the recorder
//!   size `after` satisfies `after <= e_last`.
//! - **Idempotence**: two consecutive estimations at the end of a block return the same value.
//! - **Monotone**: estimations within a block never decrease, except across a rolled back client
//!   transaction (which drops the recorded nodes).
//! - **Reclaim never negative**: in executive mode, for every extrinsic with the reclaim extension,
//!   `post >= pre`.
//! - **No trie warnings and no panics** in the estimation path. A panic is reported as
//!   [`FailureKind::PanicAlsoWithoutEstimation`] if the script also panics without estimation.
//! - **Complete proof**: the recorder size after `storage_root` is at least the truth `t` (see
//!   below); otherwise the recorded proof misses nodes a validator needs.
//! - **Validator agreement**: every block of the estimation pass is replayed as `validate_block`
//!   executes it: on the collator's proof, in one runtime call without host transactions, without
//!   the extrinsics the block builder dropped. The replay must not miss a trie node, must reach the
//!   collator's post-block root and must observe exactly the collator's estimation values, since
//!   weight reclaim writes them into `frame_system::BlockWeight`.
//!
//! Soft statistic: the same script is executed a second time without any estimation and the
//! recorder size after `storage_root` is taken as the truth `t`; [`BlockReport::overestimation`]
//! is `e_last - t`.

use arbitrary::{Arbitrary, Result as ArbitraryResult, Unstructured};
use cumulus_primitives_proof_size_hostfunction::storage_proof_size;
use sp_core::{
	storage::{ChildInfo, StateVersion, Storage, StorageChild},
	Blake2Hasher, H256,
};
use sp_externalities::{Extensions, TransactionType};
use sp_state_machine::{Ext, InMemoryBackend, OverlayedChanges, TrieBackend, TrieBackendBuilder};
use sp_trie::{
	cache::{CacheSize, LocalTrieCache, SharedTrieCache},
	proof_size_extension::ProofSizeExt,
	recorder::Recorder,
	MemoryDB, PrefixedMemoryDB, StorageProof,
};
use std::{
	cell::{Cell, RefCell},
	collections::BTreeMap,
	fmt,
	panic::{self, AssertUnwindSafe},
	sync::{Once, OnceLock},
};

/// Maximum number of blocks per script.
pub const MAX_BLOCKS: usize = 4;
/// Maximum number of extrinsics per block.
pub const MAX_XTS: usize = 12;
/// Maximum number of ops in a top-level op list (`init`, `body`, `finalize`).
pub const MAX_OPS: usize = 24;
/// Maximum number of ops in the `pre` and `post` lists of an extrinsic.
pub const MAX_PRE_POST_OPS: usize = 8;
/// Maximum number of ops inside a nested runtime transaction.
pub const MAX_NESTED_OPS: usize = 8;
/// Maximum nesting depth of runtime transactions.
pub const MAX_TX_DEPTH: usize = 6;
/// Maximum number of explicitly listed initial entries (main trie or one child).
pub const MAX_EXTRA_ENTRIES: usize = 16;
/// Maximum number of initially populated child tries. There are [`CHILD_IDS`] child ids, so
/// at least one child always starts empty.
pub const MAX_INITIAL_CHILDREN: usize = 3;
/// Number of distinct child tries a script can address.
pub const CHILD_IDS: u8 = 4;

type Hasher = Blake2Hasher;
type State = InMemoryBackend<Hasher>;
type BlockBackend<'a> =
	TrieBackend<&'a PrefixedMemoryDB<Hasher>, Hasher, LocalTrieCache<Hasher>, Recorder<Hasher>>;
/// Backend of the validator pass: the collator's proof as the only database.
type ValidatorBackend =
	TrieBackend<MemoryDB<Hasher>, Hasher, LocalTrieCache<Hasher>, Recorder<Hasher>>;

/// State version selector.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
	V0,
	V1,
}

impl Version {
	fn state_version(self) -> StateVersion {
		match self {
			Version::V0 => StateVersion::V0,
			Version::V1 => StateVersion::V1,
		}
	}
}

/// Size of the key space and of the initial tries.
///
/// Small tries find and shrink local bugs quickly, but almost every operation shares trie nodes
/// with every other one, and the recorder deduplicates nodes, so a missed node can be hidden by
/// an unrelated read. Larger scales reduce that overlap.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scale {
	/// 4 pallets, 3 items, 128 indices (1536 map keys); up to 600 initial main trie keys and
	/// 100 keys per child. Scripts run in milliseconds.
	Small,
	/// 16 pallets, 8 items, 4096 indices; up to 5000 initial keys and 1000 per child.
	Medium,
	/// 16 pallets, 8 items, 65536 indices; up to 50000 initial keys and 5000 per child.
	/// Scripts take seconds.
	Large,
}

impl Scale {
	fn pallets(self) -> u64 {
		match self {
			Scale::Small => 4,
			Scale::Medium | Scale::Large => 16,
		}
	}

	fn items(self) -> u64 {
		match self {
			Scale::Small => 3,
			Scale::Medium | Scale::Large => 8,
		}
	}

	fn indices(self) -> u64 {
		match self {
			Scale::Small => 128,
			Scale::Medium => 4096,
			Scale::Large => 65536,
		}
	}

	/// Maximum number of generated keys in the initial main trie.
	pub fn max_initial_top(self) -> u32 {
		match self {
			Scale::Small => 600,
			Scale::Medium => 5_000,
			Scale::Large => 50_000,
		}
	}

	/// Maximum number of generated keys per initial child trie.
	pub fn max_child_bulk(self) -> u32 {
		match self {
			Scale::Small => 100,
			Scale::Medium => 1_000,
			Scale::Large => 5_000,
		}
	}

	/// Exclusive upper bound of `clear_prefix` and `storage_kill` limits.
	fn limit_range(self) -> u32 {
		match self {
			Scale::Small => 8,
			Scale::Medium => 64,
			Scale::Large => 1024,
		}
	}
}

/// Where estimations happen.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
	/// Estimations only at the fixed points of the executive: before and after each extrinsic
	/// with the reclaim extension, and at the end of the block. [`Op::Estimate`] is a no-op.
	Executive,
	/// Like [`Mode::Executive`], and additionally every [`Op::Estimate`] estimates.
	Free,
}

/// Outcome of a transaction.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
	Commit,
	Rollback,
}

/// Size class of a value.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
	/// Zero bytes.
	Empty,
	/// 1 to 31 bytes, always inline.
	Small,
	/// Exactly 32 bytes, the largest value kept inline under `StateVersion::V1`.
	Exact32,
	/// 33 to 128 bytes, hashed out under `StateVersion::V1`.
	Large,
}

/// A value: size class plus a seed for its content and length.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Val(pub Size, pub u8);

impl Val {
	/// The value bytes.
	pub fn bytes(&self) -> Vec<u8> {
		let Val(size, seed) = *self;
		let len = match size {
			Size::Empty => 0,
			Size::Small => 1 + seed as usize % 31,
			Size::Exact32 => 32,
			Size::Large => 33 + seed as usize % 96,
		};
		pseudo_bytes(4, seed as u64, len)
	}
}

/// A storage key. Keys are described by indices so that mutations stay meaningful and the
/// `Debug` output of a failing script is short. The indices are reduced modulo the pallet, item
/// and index counts of the script's [`Scale`] when the bytes are built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
	/// `pallet(p) ++ item(p, m) ++ hash(i) ++ i` (50 bytes), like a `Blake2_128Concat` map
	/// entry. All `Map(p, m, _)` share a 32 byte prefix.
	Map(u8, u8, u16),
	/// `pallet(p) ++ item(p, m)` (32 bytes), a storage value that is a prefix of all
	/// `Map(p, m, _)` keys, so it lives in a branch node.
	Value(u8, u8),
	/// `Map(p, m, i)` with one nibble at byte `pos % 50` changed. Bit 4 of the last field
	/// selects the high or low nibble, its low bits the xor value. Removing such a sibling
	/// collapses a branch.
	Sibling(u8, u8, u16, u8, u8),
	/// A one to three byte key `[a, a ^ 0x5a, a * 31 + 7][..1 + len % 3]`, so collapses happen
	/// near the root and short keys are prefixes of each other.
	Short(u8, u8),
	/// Explicit bytes, only used by hand-written scripts.
	Raw(Vec<u8>),
}

impl Key {
	/// The key bytes. Never starts with `:`, so it never hits `:child_storage:`.
	pub fn bytes(&self, scale: Scale) -> Vec<u8> {
		let mut key = match self {
			Key::Map(p, m, i) => map_key(*p, *m, *i, scale),
			Key::Value(p, m) => map_key(*p, *m, 0, scale)[..32].to_vec(),
			Key::Sibling(p, m, i, pos, x) => {
				let mut key = map_key(*p, *m, *i, scale);
				let pos = *pos as usize % key.len();
				let delta = x % 15 + 1;
				if x & 0x10 == 0 {
					key[pos] ^= delta;
				} else {
					key[pos] ^= delta << 4;
				}
				key
			},
			Key::Short(a, len) => {
				[*a, a ^ 0x5a, a.wrapping_mul(31).wrapping_add(7)][..1 + *len as usize % 3].to_vec()
			},
			Key::Raw(bytes) => bytes.clone(),
		};
		if key.first() == Some(&b':') {
			key[0] = b';';
		}
		key
	}
}

fn map_key(p: u8, m: u8, i: u16, scale: Scale) -> Vec<u8> {
	let p = p as u64 % scale.pallets();
	let m = m as u64 % scale.items();
	let i = (i as u64 % scale.indices()) as u16;
	let mut key = pseudo_bytes(1, p, 16);
	key.extend(pseudo_bytes(2, p * 16 + m, 16));
	key.extend(pseudo_bytes(3, i as u64, 16));
	key.extend(i.to_le_bytes());
	key
}

impl<'a> Arbitrary<'a> for Key {
	fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
		Ok(match u.int_in_range(0..=99u8)? {
			0..=59 => Key::Map(u.arbitrary()?, u.arbitrary()?, u.arbitrary()?),
			60..=67 => Key::Value(u.arbitrary()?, u.arbitrary()?),
			68..=84 => Key::Sibling(
				u.arbitrary()?,
				u.arbitrary()?,
				u.arbitrary()?,
				u.arbitrary()?,
				u.arbitrary()?,
			),
			_ => Key::Short(u.arbitrary()?, u.int_in_range(0..=2)?),
		})
	}
}

/// A prefix: the key truncated according to the second field (`% 6`): full key, all but the
/// last byte, half, first 16 bytes, first 32 bytes, first byte.
#[derive(Arbitrary, Debug, Clone, PartialEq, Eq)]
pub struct Prefix(pub Key, pub u8);

impl Prefix {
	/// The prefix bytes.
	pub fn bytes(&self, scale: Scale) -> Vec<u8> {
		let mut key = self.0.bytes(scale);
		let len = key.len();
		let keep = match self.1 % 6 {
			0 => len,
			1 => len - 1,
			2 => len / 2,
			3 => len.min(16),
			4 => len.min(32),
			_ => 1,
		};
		key.truncate(keep.max(1));
		key
	}
}

/// One storage operation, executed through the host functions inside the runtime.
///
/// Child ids are taken `% 4`, limits modulo the [`Scale`]'s limit range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
	Set(Key, Val),
	Clear(Key),
	Append(Key, Val),
	Get(Key),
	Exists(Key),
	NextKey(Key),
	ClearPrefix(Prefix, Option<u16>),
	ChildSet(u8, Key, Val),
	ChildClear(u8, Key),
	ChildGet(u8, Key),
	ChildKill(u8, Option<u16>),
	ChildClearPrefix(u8, Prefix, Option<u16>),
	ChildRoot(u8),
	/// A nested runtime transaction (`with_transaction`).
	Tx(Vec<Op>, Outcome),
	/// `sp_io::storage::root` in the middle of the block.
	Root,
	/// Estimation at an arbitrary point. Only executed in [`Mode::Free`].
	Estimate,
}

impl<'a> Arbitrary<'a> for Op {
	fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
		gen_op(u, 0)
	}
}

fn gen_child(u: &mut Unstructured) -> ArbitraryResult<u8> {
	u.int_in_range(0..=CHILD_IDS - 1)
}

fn gen_op(u: &mut Unstructured, depth: usize) -> ArbitraryResult<Op> {
	Ok(match u.int_in_range(0..=99u8)? {
		0..=23 => Op::Set(u.arbitrary()?, u.arbitrary()?),
		24..=38 => Op::Clear(u.arbitrary()?),
		39..=43 => Op::Append(u.arbitrary()?, u.arbitrary()?),
		44..=47 => Op::Get(u.arbitrary()?),
		48..=50 => Op::Exists(u.arbitrary()?),
		51..=53 => Op::NextKey(u.arbitrary()?),
		54..=58 => Op::ClearPrefix(u.arbitrary()?, u.arbitrary()?),
		59..=65 => Op::ChildSet(gen_child(u)?, u.arbitrary()?, u.arbitrary()?),
		66..=70 => Op::ChildClear(gen_child(u)?, u.arbitrary()?),
		71..=73 => Op::ChildGet(gen_child(u)?, u.arbitrary()?),
		74..=75 => Op::ChildKill(gen_child(u)?, u.arbitrary()?),
		76..=77 => Op::ChildClearPrefix(gen_child(u)?, u.arbitrary()?, u.arbitrary()?),
		78..=79 => Op::ChildRoot(gen_child(u)?),
		80..=89 if depth < MAX_TX_DEPTH => {
			Op::Tx(gen_ops(u, depth + 1, MAX_NESTED_OPS)?, u.arbitrary()?)
		},
		80..=89 => Op::Set(u.arbitrary()?, u.arbitrary()?),
		90 => Op::Root,
		_ => Op::Estimate,
	})
}

fn gen_ops(u: &mut Unstructured, depth: usize, max: usize) -> ArbitraryResult<Vec<Op>> {
	let len = u.int_in_range(0..=max)?;
	(0..len).map(|_| gen_op(u, depth)).collect()
}

fn gen_outcome(u: &mut Unstructured, rollback_percent: u8) -> ArbitraryResult<Outcome> {
	Ok(if u.ratio(rollback_percent, 100)? { Outcome::Rollback } else { Outcome::Commit })
}

/// Global configuration of a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
	/// Key space and initial trie size.
	pub scale: Scale,
	/// State version the initial trie is built with.
	pub backend_version: Version,
	/// State version passed to `storage_root` (also for `Op::Root` and `Op::ChildRoot`).
	pub root_version: Version,
	/// Run the trie backend with a `LocalTrieCache` from a `SharedTrieCache`.
	pub cache: bool,
	/// Estimation placement.
	pub mode: Mode,
}

impl<'a> Arbitrary<'a> for Config {
	fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
		Ok(Config {
			scale: match u.int_in_range(0..=19u8)? {
				0..=13 => Scale::Small,
				14..=18 => Scale::Medium,
				_ => Scale::Large,
			},
			backend_version: u.arbitrary()?,
			root_version: u.arbitrary()?,
			cache: u.arbitrary()?,
			mode: if u.ratio(1u8, 4)? { Mode::Free } else { Mode::Executive },
		})
	}
}

/// Initial content of one child trie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildInit {
	/// Child id (`% 4`). The child storage key is `[0x10 * (id % 4 + 1)]`.
	pub id: u8,
	/// Number of generated entries (from the script seed).
	pub bulk: u32,
	/// Explicit entries.
	pub extra: Vec<(Key, Val)>,
}

/// Initial state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialTrie {
	/// Seed for the generated entries.
	pub seed: u64,
	/// Number of generated main trie entries.
	pub top: u32,
	/// Explicit main trie entries.
	pub extra_top: Vec<(Key, Val)>,
	/// Child tries. Children with no entries do not exist.
	pub children: Vec<ChildInit>,
}

impl InitialTrie {
	fn arbitrary(u: &mut Unstructured, scale: Scale) -> ArbitraryResult<Self> {
		let seed = u.arbitrary()?;
		let top = u.int_in_range(0..=scale.max_initial_top())?;
		let extra_len = u.int_in_range(0..=MAX_EXTRA_ENTRIES)?;
		let extra_top = (0..extra_len).map(|_| u.arbitrary()).collect::<Result<_, _>>()?;
		let children_len = u.int_in_range(0..=MAX_INITIAL_CHILDREN)?;
		let children = (0..children_len)
			.map(|_| {
				let id = gen_child(u)?;
				let bulk = u.int_in_range(0..=scale.max_child_bulk())?;
				let extra_len = u.int_in_range(0..=MAX_EXTRA_ENTRIES / 2)?;
				let extra = (0..extra_len).map(|_| u.arbitrary()).collect::<Result<_, _>>()?;
				Ok(ChildInit { id, bulk, extra })
			})
			.collect::<ArbitraryResult<_>>()?;
		Ok(InitialTrie { seed, top, extra_top, children })
	}
}

impl InitialTrie {
	/// Build the initial [`Storage`].
	pub fn storage(&self, scale: Scale) -> Storage {
		let mut top = BTreeMap::new();
		for (k, v) in generated_entries(self.seed, self.top as usize) {
			top.insert(k.bytes(scale), v.bytes());
		}
		for (k, v) in &self.extra_top {
			top.insert(k.bytes(scale), v.bytes());
		}
		let mut children: BTreeMap<u8, BTreeMap<Vec<u8>, Vec<u8>>> = BTreeMap::new();
		for child in &self.children {
			let id = child.id % CHILD_IDS;
			let data = children.entry(id).or_default();
			for (k, v) in generated_entries(child_seed(self.seed, id), child.bulk as usize) {
				data.insert(k.bytes(scale), v.bytes());
			}
			for (k, v) in &child.extra {
				data.insert(k.bytes(scale), v.bytes());
			}
		}
		let children_default = children
			.into_iter()
			.filter(|(_, data)| !data.is_empty())
			.map(|(id, data)| {
				let child_info = child_info(id);
				(child_info.storage_key().to_vec(), StorageChild { data, child_info })
			})
			.collect();
		Storage { top, children_default }
	}
}

fn child_seed(seed: u64, id: u8) -> u64 {
	seed ^ splitmix(((id % CHILD_IDS) as u64 + 1) << 32)
}

fn generated_entries(seed: u64, count: usize) -> Vec<(Key, Val)> {
	let mut rng = XorShift::new(seed);
	let bytes = rng.bytes(count * 16);
	let mut u = Unstructured::new(&bytes);
	(0..count)
		.map(|_| {
			let key = Key::arbitrary(&mut u).unwrap_or(Key::Short(0, 0));
			let val = Val::arbitrary(&mut u).unwrap_or(Val(Size::Empty, 0));
			(key, val)
		})
		.collect()
}

/// One extrinsic, modelled after `sc-block-builder` plus the weight-reclaim extension:
///
/// ```text
/// client start_transaction
///   [Estimate pre]          (with_reclaim)
///   pre ops
///   runtime start_transaction
///     body ops
///   commit | rollback       (body_outcome)
///   post ops
///   [Estimate post]         (with_reclaim)
/// client commit | rollback  (client_outcome)
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xt {
	pub pre: Vec<Op>,
	pub body: Vec<Op>,
	pub body_outcome: Outcome,
	pub post: Vec<Op>,
	pub client_outcome: Outcome,
	pub with_reclaim: bool,
}

impl<'a> Arbitrary<'a> for Xt {
	fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
		Ok(Xt {
			pre: gen_ops(u, 0, MAX_PRE_POST_OPS)?,
			body: gen_ops(u, 0, MAX_OPS)?,
			body_outcome: gen_outcome(u, 30)?,
			post: gen_ops(u, 0, MAX_PRE_POST_OPS)?,
			client_outcome: gen_outcome(u, 20)?,
			with_reclaim: u.ratio(4u8, 5)?,
		})
	}
}

/// One block: `init` and `finalize` each run as one runtime call in a committed client
/// transaction. The final estimations and `storage_root` run at the end of `finalize`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
	pub init: Vec<Op>,
	pub xts: Vec<Xt>,
	pub finalize: Vec<Op>,
}

impl<'a> Arbitrary<'a> for Block {
	fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
		let init = gen_ops(u, 0, MAX_OPS)?;
		let xts_len = u.int_in_range(0..=MAX_XTS)?;
		let xts = (0..xts_len).map(|_| u.arbitrary()).collect::<Result<_, _>>()?;
		let finalize = gen_ops(u, 0, MAX_OPS)?;
		Ok(Block { init, xts, finalize })
	}
}

/// A complete fuzz input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
	pub config: Config,
	pub initial: InitialTrie,
	/// Blocks are executed on top of each other: after each block the overlay is drained into
	/// the backend and the next block starts with a fresh overlay and recorder.
	pub blocks: Vec<Block>,
}

impl Script {
	/// Sets the scale and caps the generated initial trie sizes to the new scale's maxima.
	pub fn set_scale(&mut self, scale: Scale) {
		self.config.scale = scale;
		self.initial.top = self.initial.top.min(scale.max_initial_top());
		for child in &mut self.initial.children {
			child.bulk = child.bulk.min(scale.max_child_bulk());
		}
	}
}

impl<'a> Arbitrary<'a> for Script {
	fn arbitrary(u: &mut Unstructured<'a>) -> ArbitraryResult<Self> {
		let config: Config = u.arbitrary()?;
		let initial = InitialTrie::arbitrary(u, config.scale)?;
		let blocks_len = u.int_in_range(1..=MAX_BLOCKS)?;
		let blocks = (0..blocks_len).map(|_| u.arbitrary()).collect::<Result<_, _>>()?;
		Ok(Script { config, initial, blocks })
	}
}

/// Per-block numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockReport {
	/// Last estimation before `storage_root`.
	pub e_last: u64,
	/// Recorder size after `storage_root`.
	pub after: u64,
	/// Recorder size after `storage_root` when the block runs without any estimation.
	pub truth: u64,
	/// Number of estimations executed in the block.
	pub estimates: usize,
	/// Number of estimation values the validator pass reproduced (the estimations of dropped
	/// extrinsics are not part of the block).
	pub validated_estimates: usize,
	/// Number of extrinsics the block builder rolled back; the validator pass skips them.
	pub dropped_xts: usize,
}

impl BlockReport {
	/// `e_last - truth`, the bytes the estimation over-reports.
	pub fn overestimation(&self) -> i64 {
		self.e_last as i64 - self.truth as i64
	}
}

/// Result of a successful [`run_script`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
	pub blocks: Vec<BlockReport>,
}

/// Which hard invariant failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
	/// `storage_root` recorded nodes the final estimation did not: `after > e_last`.
	Overshoot,
	/// Two consecutive estimations differ.
	NotIdempotent,
	/// An estimation is smaller than the previous one without client rollback in between.
	NotMonotone,
	/// `post < pre` for an extrinsic with reclaim.
	NegativeReclaim,
	/// A `trie` or `state` warning was logged.
	Warning,
	/// The run with estimations recorded fewer bytes after `storage_root` than the same run
	/// without estimations (`after < truth`), so the proof misses nodes that a validator
	/// reads.
	IncompleteProof,
	/// Something panicked, and the same script without any estimation does not panic: the
	/// panic is caused by the estimation path.
	Panic,
	/// Something panicked, and the same script without any estimation panics too: not caused
	/// by the estimation (or not only).
	PanicAlsoWithoutEstimation,
	/// The validator pass hit a trie node that is not in the collator's proof: it panicked
	/// (`Ext` panics with `EXT_NOT_ALLOWED_TO_FAIL` on `IncompleteDatabase`) or logged a `trie`
	/// or `state` warning (the estimation code logs read errors). The PoV is unusable.
	ValidatorMissingNode,
	/// The validator pass computed a different post-block storage root than the collator.
	ValidatorRootMismatch,
	/// The validator pass observed a different sequence of estimation values than the collator
	/// for the included extrinsics. Weight reclaim stores `post - pre` in `BlockWeight`, so the
	/// validator computes a different state and rejects the block.
	ValidatorEstimateMismatch,
}

impl FailureKind {
	/// Whether the failure was detected by the validator pass.
	pub fn is_validator(self) -> bool {
		matches!(
			self,
			FailureKind::ValidatorMissingNode |
				FailureKind::ValidatorRootMismatch |
				FailureKind::ValidatorEstimateMismatch
		)
	}
}

/// Where an estimation value was taken within a block. Extrinsic indices are positions in
/// [`Block::xts`], counting extrinsics the block builder dropped, so the collator and the
/// validator tag the same estimation with the same position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstimatePos {
	/// The `n`-th executed [`Op::Estimate`] of `init` ([`Mode::Free`]).
	Init(usize),
	/// Pre-dispatch estimation of extrinsic `x`.
	Pre(usize),
	/// The `n`-th executed [`Op::Estimate`] of extrinsic `x`, `Free(x, n)`.
	Free(usize, usize),
	/// Post-dispatch estimation of extrinsic `x`.
	Post(usize),
	/// The `n`-th executed [`Op::Estimate`] of `finalize`.
	Finalize(usize),
	/// The `n`-th of the two final estimations before `storage_root`. Production has no such
	/// estimation (see the README caveats).
	Final(usize),
}

impl EstimatePos {
	/// The extrinsic the estimation belongs to.
	pub fn xt(self) -> Option<usize> {
		match self {
			EstimatePos::Pre(x) | EstimatePos::Free(x, _) | EstimatePos::Post(x) => Some(x),
			EstimatePos::Init(_) | EstimatePos::Finalize(_) | EstimatePos::Final(_) => None,
		}
	}
}

fn fmt_estimates(values: &[(EstimatePos, u64)]) -> String {
	let items: Vec<_> = values.iter().map(|(pos, v)| format!("{pos:?}={v}")).collect();
	format!("[{}]", items.join(", "))
}

/// A hard invariant violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
	pub kind: FailureKind,
	/// Index of the block in which the invariant failed.
	pub block: usize,
	/// What was compared, including the numbers involved.
	pub detail: String,
	/// For validator failures tied to an estimation: the position of the first differing value
	/// ([`FailureKind::ValidatorEstimateMismatch`]) or of the estimation that first logged a
	/// warning ([`FailureKind::ValidatorMissingNode`]).
	pub position: Option<EstimatePos>,
}

impl fmt::Display for Failure {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{:?} in block {}: {}", self.kind, self.block, self.detail)
	}
}

thread_local! {
	static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
	static QUIET_PANICS: Cell<bool> = const { Cell::new(false) };
	static PANIC_LOCATION: Cell<Option<String>> = const { Cell::new(None) };
	static CURRENT_BLOCK: Cell<usize> = const { Cell::new(0) };
}

struct WarningCapture;

impl log::Log for WarningCapture {
	fn enabled(&self, metadata: &log::Metadata) -> bool {
		metadata.level() <= log::Level::Warn || log_enabled(metadata)
	}

	fn log(&self, record: &log::Record) {
		let target = record.target();
		if record.level() <= log::Level::Warn && (target == "trie" || target == "state") {
			WARNINGS.with(|w| w.borrow_mut().push(format!("{target}: {}", record.args())));
		}
		if log_enabled(record.metadata()) {
			eprintln!("{} {target}: {}", record.level(), record.args());
		}
	}

	fn flush(&self) {}
}

static LOGGER: WarningCapture = WarningCapture;

/// `POV_FUZZ_LOG`, in `RUST_LOG` syntax. Empty when logging is off.
fn log_spec() -> &'static str {
	static SPEC: OnceLock<String> = OnceLock::new();
	SPEC.get_or_init(|| std::env::var("POV_FUZZ_LOG").unwrap_or_default())
}

/// The `target=level` and bare `level` directives of [`log_spec`], applied to `log` records.
fn log_directives() -> &'static [(Option<String>, log::LevelFilter)] {
	static DIRECTIVES: OnceLock<Vec<(Option<String>, log::LevelFilter)>> = OnceLock::new();
	DIRECTIVES.get_or_init(|| {
		log_spec()
			.split(',')
			.filter_map(|directive| match directive.split_once('=') {
				Some((target, level)) => Some((Some(target.to_string()), level.parse().ok()?)),
				None => Some((None, directive.parse().ok()?)),
			})
			.collect()
	})
}

fn log_enabled(metadata: &log::Metadata) -> bool {
	log_directives().iter().any(|(target, level)| {
		target
			.as_ref()
			.map_or(true, |target| metadata.target().starts_with(target.as_str())) &&
			metadata.level() <= *level
	})
}

/// Prints a progress marker between the log lines, so that the passes and runtime calls of a
/// script can be told apart.
fn mark(message: fmt::Arguments) {
	if !log_spec().is_empty() {
		eprintln!("pov-fuzz: {message}");
	}
}

fn install_hooks() {
	static ONCE: Once = Once::new();
	ONCE.call_once(|| {
		if log::set_logger(&LOGGER).is_ok() {
			log::set_max_level(if log_spec().is_empty() {
				log::LevelFilter::Warn
			} else {
				log::LevelFilter::Trace
			});
		}
		if !log_spec().is_empty() {
			use sp_tracing::tracing_subscriber::{fmt, EnvFilter};
			let subscriber = fmt()
				.with_env_filter(EnvFilter::new(log_spec()))
				.with_writer(std::io::stderr)
				.finish();
			let _ = sp_tracing::tracing::subscriber::set_global_default(subscriber);
		}
		let previous = panic::take_hook();
		panic::set_hook(Box::new(move |info| {
			PANIC_LOCATION.with(|l| l.set(info.location().map(|l| l.to_string())));
			if !QUIET_PANICS.with(|q| q.get()) {
				previous(info)
			}
		}));
	});
}

fn take_warnings() -> Vec<String> {
	WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

fn child_info(id: u8) -> ChildInfo {
	ChildInfo::new_default(&[0x10 * (id % CHILD_IDS + 1)])
}

fn limit(limit: &Option<u16>, scale: Scale) -> Option<u32> {
	limit.map(|l| l as u32 % scale.limit_range())
}

/// The runtime call an estimation happens in, used to tag the values with an [`EstimatePos`].
#[derive(Debug, Clone, Copy)]
enum Phase {
	Init,
	Xt(usize),
	Finalize,
}

/// Per-pass execution state, mutated from inside the externalities closures.
struct Ctx {
	scale: Scale,
	mode: Mode,
	estimate: bool,
	root_version: StateVersion,
	block: usize,
	last_estimate: Option<u64>,
	estimates: usize,
	failure: Option<Failure>,
	phase: Phase,
	/// Number of [`Op::Estimate`] executed in the current phase.
	free_estimates: usize,
	/// All estimation values of the block in execution order.
	trace: Vec<(EstimatePos, u64)>,
	/// The first estimation after which a `trie` or `state` warning was pending.
	first_warning_at: Option<EstimatePos>,
}

impl Ctx {
	fn new(config: &Config, estimate: bool, block: usize) -> Self {
		Ctx {
			scale: config.scale,
			mode: config.mode,
			estimate,
			root_version: config.root_version.state_version(),
			block,
			last_estimate: None,
			estimates: 0,
			failure: None,
			phase: Phase::Init,
			free_estimates: 0,
			trace: Vec::new(),
			first_warning_at: None,
		}
	}

	/// Records the first hard invariant violation of the block.
	fn fail(&mut self, kind: FailureKind, detail: String) {
		if self.failure.is_none() {
			self.failure = Some(Failure { kind, block: self.block, detail, position: None });
		}
	}

	fn enter(&mut self, phase: Phase) {
		mark(format_args!("{phase:?}"));
		self.phase = phase;
		self.free_estimates = 0;
	}

	/// Does what the v2 host function does, records the value and checks monotonicity.
	fn estimate(&mut self, pos: EstimatePos) -> u64 {
		// Version 2 of `storage_proof_size` is `register_only`, so the bare call below resolves to
		// version 1 and the runtime-interface macro keeps the versioned functions private. The
		// estimation is therefore run by hand first, which is exactly what version 2 does when
		// `ProofSizeExt` is registered. Must stay in sync with the version 2 body.
		sp_externalities::with_externalities(|ext| ext.record_proof_for_dirty_keys());
		let size = storage_proof_size::storage_proof_size();
		self.estimates += 1;
		if let Some(prev) = self.last_estimate {
			if size < prev {
				self.fail(
					FailureKind::NotMonotone,
					format!(
						"estimate #{} ({pos:?}) decreased from {prev} to {size}",
						self.estimates
					),
				);
			}
		}
		mark(format_args!("estimate {pos:?} = {size}"));
		self.last_estimate = Some(size);
		self.trace.push((pos, size));
		if self.first_warning_at.is_none() && WARNINGS.with(|w| !w.borrow().is_empty()) {
			self.first_warning_at = Some(pos);
		}
		size
	}

	/// Runtime side of `initialize_block`.
	fn initialize_block(&mut self, ops: &[Op]) {
		self.enter(Phase::Init);
		self.exec_ops(ops);
	}

	/// Runtime side of `apply_extrinsic`: weight-reclaim `validate()`, the extension pipeline,
	/// `with_storage_layer` around the call, `post_dispatch()`. Collator and validator both run
	/// exactly this; only the host transaction around it differs.
	fn apply_extrinsic(&mut self, xt_index: usize, xt: &Xt) {
		self.enter(Phase::Xt(xt_index));
		let reclaim = xt.with_reclaim && self.estimate;
		let pre = reclaim.then(|| self.estimate(EstimatePos::Pre(xt_index)));
		self.exec_ops(&xt.pre);
		sp_io::storage::start_transaction();
		self.exec_ops(&xt.body);
		match xt.body_outcome {
			Outcome::Commit => sp_io::storage::commit_transaction(),
			Outcome::Rollback => sp_io::storage::rollback_transaction(),
		}
		self.exec_ops(&xt.post);
		if let Some(pre) = pre {
			let post = self.estimate(EstimatePos::Post(xt_index));
			if post < pre {
				self.fail(
					FailureKind::NegativeReclaim,
					format!("extrinsic {xt_index}: pre = {pre}, post = {post}"),
				);
			}
		}
	}

	/// Runtime side of `finalize_block`, then the two final estimations and `storage_root`.
	/// Returns the last estimation (0 without estimation) and the encoded root.
	fn finalize_block(&mut self, ops: &[Op]) -> (u64, Vec<u8>) {
		self.enter(Phase::Finalize);
		self.exec_ops(ops);
		let e_last = if self.estimate {
			let first = self.estimate(EstimatePos::Final(0));
			let second = self.estimate(EstimatePos::Final(1));
			if first != second {
				self.fail(
					FailureKind::NotIdempotent,
					format!("consecutive final estimations differ: {first}, then {second}"),
				);
			}
			second
		} else {
			0
		};
		mark(format_args!("storage_root"));
		(e_last, sp_io::storage::root(self.root_version))
	}

	fn exec_ops(&mut self, ops: &[Op]) {
		for op in ops {
			self.exec_op(op);
		}
	}

	fn exec_op(&mut self, op: &Op) {
		use sp_io::{default_child_storage as child, storage};
		let scale = self.scale;
		match op {
			Op::Set(k, v) => storage::set(&k.bytes(scale), &v.bytes()),
			Op::Clear(k) => storage::clear(&k.bytes(scale)),
			Op::Append(k, v) => storage::append(&k.bytes(scale), v.bytes()),
			Op::Get(k) => {
				let _ = storage::get(&k.bytes(scale));
			},
			Op::Exists(k) => {
				let _ = storage::exists(&k.bytes(scale));
			},
			Op::NextKey(k) => {
				let _ = storage::next_key(&k.bytes(scale));
			},
			Op::ClearPrefix(p, l) => {
				let _ = storage::clear_prefix(&p.bytes(scale), limit(l, scale));
			},
			Op::ChildSet(c, k, v) => {
				child::set(child_info(*c).storage_key(), &k.bytes(scale), &v.bytes())
			},
			Op::ChildClear(c, k) => child::clear(child_info(*c).storage_key(), &k.bytes(scale)),
			Op::ChildGet(c, k) => {
				let _ = child::get(child_info(*c).storage_key(), &k.bytes(scale));
			},
			Op::ChildKill(c, l) => {
				let _ = child::storage_kill(child_info(*c).storage_key(), limit(l, scale));
			},
			Op::ChildClearPrefix(c, p, l) => {
				let _ = child::clear_prefix(
					child_info(*c).storage_key(),
					&p.bytes(scale),
					limit(l, scale),
				);
			},
			Op::ChildRoot(c) => {
				let _ = child::root(child_info(*c).storage_key(), self.root_version);
			},
			Op::Tx(ops, outcome) => {
				storage::start_transaction();
				self.exec_ops(ops);
				match outcome {
					Outcome::Commit => storage::commit_transaction(),
					Outcome::Rollback => storage::rollback_transaction(),
				}
			},
			Op::Root => {
				let _ = storage::root(self.root_version);
			},
			Op::Estimate => {
				if self.estimate && self.mode == Mode::Free {
					let n = self.free_estimates;
					self.free_estimates += 1;
					let pos = match self.phase {
						Phase::Init => EstimatePos::Init(n),
						Phase::Xt(x) => EstimatePos::Free(x, n),
						Phase::Finalize => EstimatePos::Finalize(n),
					};
					self.estimate(pos);
				}
			},
		}
	}
}

/// The recorder, extensions, state and overlay of one block.
///
/// The trie backend is not part of it: every runtime call gets a fresh one, as in production.
/// `LocalCallExecutor::contextual_call` (`substrate/client/service/src/client/call_executor.rs`)
/// calls `state_at` and `TrieBackendBuilder::wrap(..).with_recorder(..)` per runtime call, so
/// the `TrieBackendEssence` caches (the child root cache) live for one runtime call, which is
/// one host transaction. The recorder and the overlay are shared by all calls of the block.
struct BlockEnv<'a> {
	recorder: Recorder<Hasher>,
	extensions: Extensions,
	state: &'a State,
	cache: Option<&'a SharedTrieCache<Hasher>>,
	overlay: OverlayedChanges<Hasher>,
	state_version: StateVersion,
}

impl<'a> BlockEnv<'a> {
	fn new(
		state: &'a State,
		cache: Option<&'a SharedTrieCache<Hasher>>,
		state_version: StateVersion,
	) -> Self {
		let recorder = Recorder::<Hasher>::default();
		let mut extensions = Extensions::new();
		extensions.register(ProofSizeExt::new(recorder.clone()));
		BlockEnv {
			recorder,
			extensions,
			state,
			cache,
			overlay: OverlayedChanges::default(),
			state_version,
		}
	}

	/// A fresh backend for one runtime call, recording into the block's recorder.
	fn backend(&self) -> BlockBackend<'a> {
		TrieBackendBuilder::wrap(self.state)
			.with_recorder(self.recorder.clone())
			.with_optional_cache(self.cache.map(|c| c.local_cache_untrusted()))
			.build()
	}

	/// Client transaction as opened by `sp-api` (`execute_in_transaction` and runtime calls
	/// at depth 0): overlay, recorder and extensions (`TransactionType::Host`).
	fn client_start(&mut self) {
		self.overlay.start_transaction();
		self.recorder.start_transaction();
		self.extensions.start_transaction(TransactionType::Host);
	}

	fn client_end(&mut self, outcome: Outcome) {
		const PROOF: &str = "client transactions are balanced; qed";
		match outcome {
			Outcome::Commit => {
				self.recorder.commit_transaction().expect(PROOF);
				self.overlay.commit_transaction().expect(PROOF);
				self.extensions.commit_transaction(TransactionType::Host);
			},
			Outcome::Rollback => {
				self.recorder.rollback_transaction().expect(PROOF);
				self.overlay.rollback_transaction().expect(PROOF);
				self.extensions.rollback_transaction(TransactionType::Host);
			},
		}
	}

	/// Runs `f` as one runtime call: `enter_runtime`, a fresh backend and `Ext` with the
	/// extensions, `exit_runtime`.
	fn in_runtime<R>(&mut self, f: impl FnOnce() -> R) -> R {
		self.overlay.enter_runtime().expect("not in runtime; qed");
		let res = {
			let backend = self.backend();
			let mut ext = Ext::new(
				&mut self.overlay,
				&backend,
				Some(&mut self.extensions),
				self.state_version,
			);
			sp_externalities::set_and_run_with_externalities(&mut ext, f)
		};
		self.overlay.exit_runtime().expect("in runtime; qed");
		res
	}
}

/// Per-block result of one pass.
struct BlockRun {
	e_last: u64,
	after: u64,
	estimates: usize,
	validated_estimates: usize,
	dropped_xts: usize,
}

/// What the collator hands to the validator for one block, plus the estimation values the
/// validator has to reproduce.
struct BlockWitness {
	/// State root before the block.
	pre_root: H256,
	/// State root after the block (`transaction_storage_root` of the drained changes).
	post_root: H256,
	/// The block recorder's proof, taken after `drain_storage_changes` like
	/// `sc-block-builder` does.
	proof: StorageProof,
	/// All estimation values of the collator in execution order, including those of extrinsics
	/// the block builder dropped.
	estimates: Vec<(EstimatePos, u64)>,
}

/// Executes the whole script once. With `estimate == false` all estimations are skipped
/// (used for the truth run). With `estimate == true` every block is also replayed by
/// [`validate_block`] on the collator's proof.
///
/// A validator failure takes precedence over a collator-side failure of the same block: a block
/// the validator rejects is worse than one whose weight is off. The collator failure is
/// appended to the detail.
fn execute(script: &Script, estimate: bool) -> Result<Vec<BlockRun>, Failure> {
	let config = &script.config;
	let root_version = config.root_version.state_version();
	let mut state: State =
		(script.initial.storage(config.scale), config.backend_version.state_version()).into();
	let shared_cache = config
		.cache
		.then(|| SharedTrieCache::<Hasher>::new(CacheSize::new(64 << 20), None));
	let _ = take_warnings();

	let mut runs = Vec::with_capacity(script.blocks.len());
	for (block_index, block) in script.blocks.iter().enumerate() {
		CURRENT_BLOCK.with(|b| b.set(block_index));
		mark(format_args!(
			"{} pass, block {block_index}",
			if estimate { "collator" } else { "truth" }
		));
		let pre_root = *state.root();
		let mut ctx = Ctx::new(config, estimate, block_index);
		let mut env = BlockEnv::new(&state, shared_cache.as_ref(), root_version);

		// `initialize_block`.
		env.client_start();
		env.in_runtime(|| ctx.initialize_block(&block.init));
		env.client_end(Outcome::Commit);

		// `apply_extrinsic` inside `execute_in_transaction`.
		for (xt_index, xt) in block.xts.iter().enumerate() {
			env.client_start();
			env.in_runtime(|| ctx.apply_extrinsic(xt_index, xt));
			env.client_end(xt.client_outcome);
			if xt.client_outcome == Outcome::Rollback {
				// The recorder dropped the nodes recorded in the transaction.
				ctx.last_estimate = None;
			}
		}

		// `finalize_block`: final estimations, then the storage root.
		env.client_start();
		let (e_last, _) = env.in_runtime(|| ctx.finalize_block(&block.finalize));
		env.client_end(Outcome::Commit);
		let after = env.recorder.estimate_encoded_size() as u64;

		if estimate && after > e_last {
			ctx.fail(
				FailureKind::Overshoot,
				format!("e_last = {e_last}, after = {after} (+{} bytes)", after - e_last),
			);
		}
		let warnings = take_warnings();
		if !warnings.is_empty() {
			ctx.fail(FailureKind::Warning, warnings.join("; "));
		}

		// Finish the block as `sc-block-builder` does: drain the changes, then take the proof.
		let backend = env.backend();
		let BlockEnv { mut overlay, recorder, .. } = env;
		let changes = overlay
			.drain_storage_changes(&backend, root_version)
			.expect("in-memory backend does not fail; qed");
		drop(backend);

		let post_root = changes.transaction_storage_root;
		let dropped_xts =
			block.xts.iter().filter(|xt| xt.client_outcome == Outcome::Rollback).count();

		let mut validated_estimates = 0;
		if estimate {
			let witness = BlockWitness {
				pre_root,
				post_root,
				proof: recorder.to_storage_proof(),
				estimates: std::mem::take(&mut ctx.trace),
			};
			match validate_block(config, block_index, block, &witness) {
				Ok(compared) => validated_estimates = compared,
				Err(mut failure) => {
					if let Some(collator) = &ctx.failure {
						failure.detail = format!("{}; collator also: {collator}", failure.detail);
					}
					return Err(failure);
				},
			}
		}
		if let Some(failure) = ctx.failure {
			return Err(failure);
		}
		runs.push(BlockRun {
			e_last,
			after,
			estimates: ctx.estimates,
			validated_estimates,
			dropped_xts,
		});

		// Commit the block into the backend for the next one.
		state.apply_transaction(post_root, changes.transaction);
	}
	Ok(runs)
}

/// Replays `block` the way `validate_block`
/// (`cumulus/pallets/parachain-system/src/validate_block/implementation.rs`) executes it and
/// compares the result with the collator's [`BlockWitness`]. Returns the number of compared
/// estimation values.
///
/// - The backend is the collator's proof as a `MemoryDB` with root `pre_root`, wrapped with a fresh
///   recorder. It is built with `TrieBackendBuilder::new` rather than `create_proof_check_backend`,
///   which only adds the check that the root node is in the proof; a block that reads nothing has
///   an empty proof, and any missing node, including the root, surfaces on access.
/// - One `OverlayedChanges`, one `Ext`, one `set_and_run_with_externalities` for the whole block.
///   No host transactions, no `enter_runtime`; the runtime's own transactions
///   (`with_storage_layer`, [`Op::Tx`]) are the only layers.
/// - Extrinsics with `client_outcome == Rollback` are not part of the block and are skipped.
/// - With version 2 enabled, `validate_block` answers `storage_proof_size` with
///   `ext.record_proof_for_dirty_keys()` and the `estimate_encoded_size()` of a
///   `SizeOnlyRecorderProvider`. The harness does the same through `ProofSizeExt` on a `Recorder`.
///   Both sum the encoded sizes of the distinct recorded nodes and values;
///   `recorder_equivalence_cache` and `recorder_equivalence_no_cache` in
///   `validate_block/trie_recorder.rs` assert that the two report the same size. The validator side
///   has no `std` child-root cache in production; here the proof backend has one, which is harmless
///   without host rollbacks.
fn validate_block(
	config: &Config,
	block_index: usize,
	block: &Block,
	witness: &BlockWitness,
) -> Result<usize, Failure> {
	let included = |pos: &EstimatePos| {
		pos.xt().map_or(true, |x| block.xts[x].client_outcome == Outcome::Commit)
	};
	let (expected, dropped): (Vec<_>, Vec<_>) =
		witness.estimates.iter().copied().partition(|(pos, _)| included(pos));

	let recorder = Recorder::<Hasher>::default();
	let mut extensions = Extensions::new();
	extensions.register(ProofSizeExt::new(recorder.clone()));
	let backend: ValidatorBackend =
		TrieBackendBuilder::new(witness.proof.clone().into_memory_db(), witness.pre_root)
			.with_recorder(recorder.clone())
			.build();
	let mut overlay = OverlayedChanges::<Hasher>::default();
	let mut ctx = Ctx::new(config, true, block_index);
	mark(format_args!("validator pass, block {block_index}"));

	let _ = take_warnings();
	let quiet = QUIET_PANICS.with(|q| q.replace(true));
	let result = panic::catch_unwind(AssertUnwindSafe(|| {
		let mut ext = Ext::new(
			&mut overlay,
			&backend,
			Some(&mut extensions),
			config.root_version.state_version(),
		);
		sp_externalities::set_and_run_with_externalities(&mut ext, || {
			ctx.initialize_block(&block.init);
			for (xt_index, xt) in block.xts.iter().enumerate() {
				if xt.client_outcome == Outcome::Commit {
					ctx.apply_extrinsic(xt_index, xt);
				}
			}
			ctx.finalize_block(&block.finalize).1
		})
	}));
	QUIET_PANICS.with(|q| q.set(quiet));
	let warnings = take_warnings();

	let fail = |kind, position, detail| Err(Failure { kind, block: block_index, detail, position });
	let context = || {
		format!(
			"proof has {} nodes; validator estimates so far {}; collator {}",
			witness.proof.len(),
			fmt_estimates(&ctx.trace),
			fmt_estimates(&expected)
		)
	};
	let root = match result {
		Ok(root) => root,
		Err(payload) => {
			let location = PANIC_LOCATION.with(|l| l.take()).unwrap_or_default();
			return fail(
				FailureKind::ValidatorMissingNode,
				ctx.first_warning_at,
				format!(
					"validator panicked: {} at {location}; warnings: [{}]; {}",
					panic_message(&*payload),
					warnings.join("; "),
					context()
				),
			);
		},
	};
	if !warnings.is_empty() {
		// `validate_block` logs these and continues; the estimation value then lacks the nodes
		// the proof does not have.
		return fail(
			FailureKind::ValidatorMissingNode,
			ctx.first_warning_at,
			format!(
				"validator warnings (first after estimation {}): [{}]; {}",
				ctx.first_warning_at.map_or_else(|| "none".into(), |pos| format!("{pos:?}")),
				warnings.join("; "),
				context()
			),
		);
	}
	if root != witness.post_root.as_ref() {
		return fail(
			FailureKind::ValidatorRootMismatch,
			None,
			format!(
				"collator root {:?}, validator root 0x{}",
				witness.post_root,
				root.iter().map(|b| format!("{b:02x}")).collect::<String>()
			),
		);
	}
	if ctx.trace != expected {
		let first = (0..expected.len().max(ctx.trace.len()))
			.find(|&i| expected.get(i) != ctx.trace.get(i))
			.expect("the sequences differ; qed");
		let show = |v: Option<&(EstimatePos, u64)>| {
			v.map_or_else(|| "none".to_string(), |(pos, v)| format!("{pos:?} = {v}"))
		};
		return fail(
			FailureKind::ValidatorEstimateMismatch,
			expected.get(first).or(ctx.trace.get(first)).map(|(pos, _)| *pos),
			format!(
				"first difference at #{first}: collator {}, validator {}; collator {}; \
				 validator {}; dropped extrinsics' values {}",
				show(expected.get(first)),
				show(ctx.trace.get(first)),
				fmt_estimates(&expected),
				fmt_estimates(&ctx.trace),
				fmt_estimates(&dropped)
			),
		);
	}
	Ok(expected.len())
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
	payload
		.downcast_ref::<&str>()
		.map(|s| s.to_string())
		.or_else(|| payload.downcast_ref::<String>().cloned())
		.unwrap_or_else(|| "<non-string panic payload>".into())
}

fn execute_caught(script: &Script, estimate: bool) -> Result<Vec<BlockRun>, Failure> {
	install_hooks();
	panic::catch_unwind(AssertUnwindSafe(|| execute(script, estimate))).unwrap_or_else(|payload| {
		let location = PANIC_LOCATION.with(|l| l.take()).unwrap_or_default();
		Err(Failure {
			kind: FailureKind::Panic,
			block: CURRENT_BLOCK.with(|b| b.get()),
			detail: format!("{} at {location}", panic_message(&*payload)),
			position: None,
		})
	})
}

/// Executes the script with estimations. Panics are classified by re-running the script
/// without estimations.
fn estimation_pass(script: &Script) -> Result<Vec<BlockRun>, Failure> {
	execute_caught(script, true).map_err(|mut failure| {
		if failure.kind == FailureKind::Panic {
			if let Err(without) = execute_caught(script, false) {
				failure.kind = FailureKind::PanicAlsoWithoutEstimation;
				failure.detail =
					format!("{}; without estimation: {}", failure.detail, without.detail);
			}
		}
		failure
	})
}

/// Checks the hard invariants and computes the soft statistics. Returns the first hard
/// invariant violation as `Err`.
pub fn try_run_script(script: &Script) -> Result<Report, Failure> {
	let runs = estimation_pass(script)?;
	let truth = execute_caught(script, false)?;
	for (block, (run, truth)) in runs.iter().zip(truth.iter()).enumerate() {
		if run.after < truth.after {
			return Err(Failure {
				kind: FailureKind::IncompleteProof,
				block,
				detail: format!(
					"after = {}, truth = {} ({} bytes missing)",
					run.after,
					truth.after,
					truth.after - run.after
				),
				position: None,
			});
		}
	}
	Ok(Report {
		blocks: runs
			.iter()
			.zip(truth.iter())
			.map(|(run, truth)| BlockReport {
				e_last: run.e_last,
				after: run.after,
				truth: truth.after,
				estimates: run.estimates,
				validated_estimates: run.validated_estimates,
				dropped_xts: run.dropped_xts,
			})
			.collect(),
	})
}

/// Runs the script and panics with the full script on any hard invariant violation.
pub fn run_script(script: &Script) -> Report {
	match try_run_script(script) {
		Ok(report) => report,
		Err(failure) => panic!("PoV estimation invariant violated: {failure}\n{script:#?}"),
	}
}

/// Greedy shrinker: repeatedly removes single blocks, extrinsics, initial entries and ops
/// (also inside nested transactions) while the same kind of hard failure persists.
///
/// Returns the script unchanged if it does not fail.
pub fn shrink(script: Script) -> Script {
	let Err(original) = try_run_script(&script) else { return script };
	shrink_with(
		script,
		|candidate| matches!(try_run_script(candidate), Err(f) if f.kind == original.kind),
	)
}

/// Greedy shrinker for an arbitrary predicate: keeps any single removal for which
/// `still_fails` returns `true`. Panics inside the predicate are not caught; the default panic
/// output is suppressed while it runs.
pub fn shrink_with(script: Script, still_fails: impl Fn(&Script) -> bool) -> Script {
	let check = |candidate: &Script| {
		QUIET_PANICS.with(|q| q.set(true));
		let res = still_fails(candidate);
		QUIET_PANICS.with(|q| q.set(false));
		res
	};

	let mut current = script;
	loop {
		let mut changed = false;
		let mut index = 0;
		loop {
			let candidates = candidates(&current);
			let Some(candidate) = candidates.into_iter().nth(index) else { break };
			if check(&candidate) {
				current = candidate;
				changed = true;
			} else {
				index += 1;
			}
		}
		if !changed {
			return current;
		}
	}
}

fn candidates(script: &Script) -> Vec<Script> {
	let mut out = Vec::new();
	let with = |f: &dyn Fn(&mut Script)| {
		let mut s = script.clone();
		f(&mut s);
		s
	};

	// Smaller key space and tries first: everything else gets cheaper.
	if script.config.scale > Scale::Small {
		out.push(with(&|s| s.set_scale(Scale::Small)));
	}
	if script.config.scale > Scale::Medium {
		out.push(with(&|s| s.set_scale(Scale::Medium)));
	}

	// Whole blocks and extrinsics.
	if script.blocks.len() > 1 {
		for b in 0..script.blocks.len() {
			out.push(with(&|s| {
				s.blocks.remove(b);
			}));
		}
	}
	for (b, block) in script.blocks.iter().enumerate() {
		for x in 0..block.xts.len() {
			out.push(with(&|s| {
				s.blocks[b].xts.remove(x);
			}));
		}
	}

	// Initial state.
	let initial = &script.initial;
	if initial.top > 0 {
		out.push(with(&|s| s.initial.top = 0));
		out.push(with(&|s| s.initial.top /= 2));
		out.push(with(&|s| s.initial.top -= 1));
		// Same state with the generated entries listed explicitly, so that single entries can
		// be removed afterwards.
		out.push(with(&|s| {
			let mut extra = generated_entries(s.initial.seed, s.initial.top as usize);
			extra.append(&mut s.initial.extra_top);
			s.initial.extra_top = extra;
			s.initial.top = 0;
		}));
	}
	for c in 0..initial.children.len() {
		out.push(with(&|s| {
			s.initial.children.remove(c);
		}));
		let bulk = initial.children[c].bulk;
		if bulk > 0 {
			out.push(with(&|s| s.initial.children[c].bulk = 0));
			out.push(with(&|s| s.initial.children[c].bulk = bulk / 2));
			out.push(with(&|s| s.initial.children[c].bulk = bulk - 1));
			out.push(with(&|s| {
				let child = &mut s.initial.children[c];
				let mut extra =
					generated_entries(child_seed(s.initial.seed, child.id), bulk as usize);
				extra.append(&mut child.extra);
				child.extra = extra;
				child.bulk = 0;
			}));
		}
		for e in 0..initial.children[c].extra.len() {
			out.push(with(&|s| {
				s.initial.children[c].extra.remove(e);
			}));
		}
	}
	for e in 0..initial.extra_top.len() {
		out.push(with(&|s| {
			s.initial.extra_top.remove(e);
		}));
	}

	// Config simplifications.
	if script.config.cache {
		out.push(with(&|s| s.config.cache = false));
	}

	// Extrinsic flags.
	for (b, block) in script.blocks.iter().enumerate() {
		for (x, xt) in block.xts.iter().enumerate() {
			if xt.with_reclaim {
				out.push(with(&|s| s.blocks[b].xts[x].with_reclaim = false));
			}
			if xt.body_outcome == Outcome::Rollback {
				out.push(with(&|s| s.blocks[b].xts[x].body_outcome = Outcome::Commit));
			}
			if xt.client_outcome == Outcome::Rollback {
				out.push(with(&|s| s.blocks[b].xts[x].client_outcome = Outcome::Commit));
			}
		}
	}

	// Single ops.
	for (b, block) in script.blocks.iter().enumerate() {
		for v in op_list_variants(&block.init) {
			out.push(with(&|s| s.blocks[b].init = v.clone()));
		}
		for v in op_list_variants(&block.finalize) {
			out.push(with(&|s| s.blocks[b].finalize = v.clone()));
		}
		for (x, xt) in block.xts.iter().enumerate() {
			for v in op_list_variants(&xt.pre) {
				out.push(with(&|s| s.blocks[b].xts[x].pre = v.clone()));
			}
			for v in op_list_variants(&xt.body) {
				out.push(with(&|s| s.blocks[b].xts[x].body = v.clone()));
			}
			for v in op_list_variants(&xt.post) {
				out.push(with(&|s| s.blocks[b].xts[x].post = v.clone()));
			}
		}
	}
	out
}

/// All lists obtained by removing one op, inlining one `Tx`, or shrinking inside one `Tx`.
fn op_list_variants(ops: &[Op]) -> Vec<Vec<Op>> {
	let mut out = Vec::new();
	for i in 0..ops.len() {
		let mut removed = ops.to_vec();
		removed.remove(i);
		out.push(removed);
		if let Op::Tx(inner, outcome) = &ops[i] {
			let mut inlined = ops.to_vec();
			inlined.splice(i..=i, inner.iter().cloned());
			out.push(inlined);
			for inner_variant in op_list_variants(inner) {
				let mut nested = ops.to_vec();
				nested[i] = Op::Tx(inner_variant, *outcome);
				out.push(nested);
			}
		}
	}
	out
}

/// Number of ops in a script, counting nested ones.
pub fn op_count(script: &Script) -> usize {
	fn count(ops: &[Op]) -> usize {
		ops.iter()
			.map(|op| match op {
				Op::Tx(inner, _) => 1 + count(inner),
				_ => 1,
			})
			.sum()
	}
	script
		.blocks
		.iter()
		.map(|b| {
			count(&b.init) +
				count(&b.finalize) +
				b.xts
					.iter()
					.map(|x| count(&x.pre) + count(&x.body) + count(&x.post))
					.sum::<usize>()
		})
		.sum()
}

fn splitmix(x: u64) -> u64 {
	let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
	z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
	z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
	z ^ (z >> 31)
}

/// `len` hash-like bytes for `(domain, n)`.
fn pseudo_bytes(domain: u64, n: u64, len: usize) -> Vec<u8> {
	let seed = splitmix(domain.wrapping_mul(0x1_0000_0001) ^ splitmix(n));
	(0..len.div_ceil(8) as u64)
		.flat_map(|chunk| splitmix(seed ^ chunk.wrapping_mul(0xA24B_AED4_963E_E407)).to_le_bytes())
		.take(len)
		.collect()
}

/// Xorshift64* byte stream, used to feed `arbitrary::Unstructured` deterministically.
struct XorShift(u64);

impl XorShift {
	fn new(seed: u64) -> Self {
		XorShift(splitmix(seed) | 1)
	}

	fn next_u64(&mut self) -> u64 {
		self.0 ^= self.0 >> 12;
		self.0 ^= self.0 << 25;
		self.0 ^= self.0 >> 27;
		self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
	}

	fn bytes(&mut self, len: usize) -> Vec<u8> {
		(0..len.div_ceil(8))
			.flat_map(|_| self.next_u64().to_le_bytes())
			.take(len)
			.collect()
	}
}

/// The script generated from `seed`: 1 KiB to 16 KiB of xorshift bytes fed to
/// [`Script::arbitrary`], with the scale capped at `max_scale`.
pub fn script_from_seed(seed: u64, max_scale: Scale) -> Script {
	let mut rng = XorShift::new(seed);
	let len = 1024 + (rng.next_u64() % (15 * 1024)) as usize;
	let bytes = rng.bytes(len);
	let mut script = Script::arbitrary(&mut Unstructured::new(&bytes))
		.expect("`Script::arbitrary` falls back to defaults on exhaustion; qed");
	if script.config.scale > max_scale {
		script.set_scale(max_scale);
	}
	script
}

/// A 64 byte value; `n` in `0..3` selects distinct contents.
fn val64(n: u8) -> Val {
	Val(Size::Large, 31 + 96 * (n % 3))
}

fn free_v1() -> Config {
	Config {
		scale: Scale::Small,
		backend_version: Version::V1,
		root_version: Version::V1,
		cache: false,
		mode: Mode::Free,
	}
}

fn only_block(init: Vec<Op>) -> Vec<Block> {
	vec![Block { init, xts: vec![], finalize: vec![] }]
}

fn raw(bytes: &[u8]) -> Key {
	Key::Raw(bytes.to_vec())
}

/// Hand-written scripts: the three review reproductions, further failures found by the fuzzer,
/// directed validator-pass cases (all expected to fail on the reviewed head), and happy paths
/// taken from the PR's own `sp-state-machine` tests.
///
/// Names starting with `review_` are the review reproductions, `fuzz_` the fuzzer findings,
/// `validator_` the directed tests of the collator/validator layer difference.
pub fn pinned_scripts() -> Vec<(&'static str, Script)> {
	// 200 map entries spread over all pallets and items, with large values.
	let many_top: Vec<(Key, Val)> = (0..200u8)
		.map(|i| (Key::Map(i % 4, i % 3, i as u16), Val(Size::Large, i)))
		.collect();

	vec![
		(
			// pr-10215.md P1 "Incremental deletions miss branch-collapse nodes", main trie:
			// delete two of three siblings across two estimations.
			"review_incremental_main_deletions",
			Script {
				config: free_v1(),
				initial: InitialTrie {
					seed: 0,
					top: 0,
					extra_top: vec![
						(raw(&[0x10]), val64(0)),
						(raw(&[0x20]), val64(1)),
						(raw(&[0x30]), val64(2)),
					],
					children: vec![],
				},
				blocks: only_block(vec![
					Op::Clear(raw(&[0x10])),
					Op::Estimate,
					Op::Clear(raw(&[0x20])),
					Op::Estimate,
				]),
			},
		),
		(
			// Same P1, child-root variant: empty two of three children (storage keys 0x10,
			// 0x20, 0x30) across two estimations.
			"review_incremental_child_root_deletions",
			Script {
				config: free_v1(),
				initial: InitialTrie {
					seed: 0,
					top: 0,
					extra_top: vec![],
					children: (0..3)
						.map(|id| ChildInit { id, bulk: 0, extra: vec![(raw(&[1]), val64(id))] })
						.collect(),
				},
				blocks: only_block(vec![
					Op::ChildClear(0, raw(&[1])),
					Op::Estimate,
					Op::ChildClear(1, raw(&[1])),
					Op::Estimate,
				]),
			},
		),
		(
			// pr-10215.md P1 "Parent dirty entries overwrite newer nested deletions":
			// update in the parent layer, delete in a nested transaction, estimate, commit,
			// estimate.
			"review_parent_update_hides_nested_delete",
			Script {
				config: free_v1(),
				initial: InitialTrie {
					seed: 0,
					top: 0,
					extra_top: vec![(raw(&[0x10]), val64(0)), (raw(&[0x20]), val64(1))],
					children: vec![],
				},
				blocks: only_block(vec![
					Op::Set(raw(&[0x10]), Val(Size::Large, 0)),
					Op::Tx(vec![Op::Clear(raw(&[0x10])), Op::Estimate], Outcome::Commit),
					Op::Estimate,
				]),
			},
		),
		(
			// Same P1 in executive shape, found by `random_smoke` (seed 0x5eed0f1022d, shrunk
			// and simplified): `on_initialize` overwrites a key, `on_finalize` deletes it, one
			// estimation at the end. Each runtime call runs in its own client transaction, so the
			// overwrite sits in the parent layer and hides the deletion. No intermediate
			// estimation and no runtime transaction is needed. The validator has both ops in one
			// layer and estimates 141 instead of 106 (`ValidatorEstimateMismatch` at `Final(0)`,
			// reported before the collator's `Overshoot`). Production has no estimation in
			// `finalize`, so this mismatch needs a pallet that estimates in a hook.
			"fuzz_client_layer_update_hides_finalize_delete",
			Script {
				config: Config { mode: Mode::Executive, ..free_v1() },
				initial: InitialTrie {
					seed: 0,
					top: 0,
					extra_top: vec![(raw(&[0x10]), val64(0)), (raw(&[0x20]), val64(1))],
					children: vec![],
				},
				blocks: vec![Block {
					init: vec![Op::Set(raw(&[0x10]), Val(Size::Large, 0))],
					xts: vec![],
					finalize: vec![Op::Clear(raw(&[0x10]))],
				}],
			},
		),
		(
			// Found by `random_smoke` (seeds 0x5eed0f10275, 0x5eed0f102a4, 0x5eed0f102f7, shrunk
			// and simplified): a child trie is emptied and a new key is inserted into it before
			// the only estimation. `storage_root` records 2 more bytes than estimated, in V0
			// and V1.
			"fuzz_child_emptied_then_refilled",
			Script {
				config: Config { mode: Mode::Executive, ..free_v1() },
				initial: InitialTrie {
					seed: 0,
					top: 0,
					extra_top: vec![(raw(&[0x50]), Val(Size::Small, 1))],
					children: vec![ChildInit {
						id: 1,
						bulk: 0,
						extra: vec![(raw(&[0x01]), Val(Size::Small, 138))],
					}],
				},
				blocks: vec![Block {
					init: vec![
						Op::ChildKill(1, None),
						Op::ChildSet(1, raw(&[0x77]), Val(Size::Small, 3)),
					],
					xts: vec![],
					finalize: vec![],
				}],
			},
		),
		(
			// Found by `random_smoke` (seed 0x5eed0f1026a, shrunk): an extrinsic clears the
			// absent key `[0xa1]`, a strict prefix of `[0xa1, 0xfb]` which is stored in a
			// `NibbledBranch` with partial key `fb` directly below the root branch. The
			// post-dispatch estimation removes `[0xa1]` from the trie and trie-db 0.31
			// (`TrieDBMut::remove_inspector`, arm `(NibbledBranch(n, ..), true)` ignores `n`)
			// panics in `fix`. The client transaction is rolled back, so `storage_root` never
			// removes the key and the same script without estimation does not panic. The
			// trie-db bug itself predates the PR; committing the extrinsic makes
			// `storage_root` panic as well (or silently delete the value of `[0xa1, 0xfb]`).
			"fuzz_estimation_hits_trie_db_remove_panic",
			Script {
				config: Config { mode: Mode::Executive, ..free_v1() },
				initial: InitialTrie {
					seed: 0,
					top: 0,
					extra_top: vec![
						(raw(&[0xa1, 0xfb, 0x86]), Val(Size::Large, 38)),
						(raw(&[0xa1, 0xfb]), Val(Size::Small, 57)),
						(raw(&[0xac, 0xf6, 0xdb]), Val(Size::Small, 1)),
					],
					children: vec![],
				},
				blocks: vec![Block {
					init: vec![],
					xts: vec![Xt {
						pre: vec![],
						body: vec![Op::Clear(raw(&[0xa1]))],
						body_outcome: Outcome::Commit,
						post: vec![],
						client_outcome: Outcome::Rollback,
						with_reclaim: true,
					}],
					finalize: vec![],
				}],
			},
		),
		(
			// Found by `random_smoke` (seeds 0x5eed0f10485, 0x5eed0f107ba, shrunk and extended):
			// the post-dispatch estimation of an extrinsic that writes child 0 looks up the
			// child root, which `TrieBackendEssence` caches. The block builder rolls the
			// extrinsic back, which drops the recorded top trie nodes but not the cached root.
			// The next extrinsic reads child 0, hits the cache and does not record the top trie
			// path again: the proof misses nodes. The validator pass cannot even open the top
			// trie (`ValidatorMissingNode`, `Invalid state root`); the truth comparison alone
			// reports it as `IncompleteProof`.
			"fuzz_child_root_cache_survives_client_rollback",
			Script {
				config: Config { mode: Mode::Executive, ..free_v1() },
				initial: InitialTrie {
					seed: 0,
					top: 0,
					extra_top: vec![(raw(&[0x10]), val64(0)), (raw(&[0x20]), val64(1))],
					children: vec![ChildInit {
						id: 0,
						bulk: 0,
						extra: vec![(raw(&[1]), val64(2))],
					}],
				},
				blocks: vec![Block {
					init: vec![],
					xts: vec![
						Xt {
							pre: vec![],
							body: vec![Op::ChildSet(0, raw(&[2]), Val(Size::Small, 1))],
							body_outcome: Outcome::Commit,
							post: vec![],
							client_outcome: Outcome::Rollback,
							with_reclaim: true,
						},
						Xt {
							pre: vec![],
							body: vec![Op::ChildGet(0, raw(&[1]))],
							body_outcome: Outcome::Commit,
							post: vec![],
							client_outcome: Outcome::Commit,
							with_reclaim: false,
						},
					],
					finalize: vec![],
				}],
			},
		),
		(
			// Directed test of the layer-structure hypothesis (collator and validator see
			// different transaction layers). `init` overwrites `[0x10]`; extrinsic 0 has no
			// reclaim, so nothing estimates before its `pre` ops delete the key and estimate.
			// Collator: the delete sits in the extrinsic's host layer, the write in the block
			// layer below, and `take_delta` lets the older `Updated` overwrite the `Deleted`; the
			// estimation reads the key (106). Validator: no host layers, both ops are in one
			// layer, the delta says `Deleted`, the estimation removes the key and records the
			// collapsing sibling (141). `ValidatorEstimateMismatch`, plus `Overshoot` on the
			// collator. This is the order of the deprecated
			// `cumulus_primitives_storage_weight_reclaim::StorageWeightReclaim`, which estimates
			// in `prepare` after the preceding extensions' writes.
			"validator_pre_delete_after_init_write",
			Script {
				config: free_v1(),
				initial: InitialTrie {
					seed: 0,
					top: 0,
					extra_top: vec![(raw(&[0x10]), val64(0)), (raw(&[0x20]), val64(1))],
					children: vec![],
				},
				blocks: vec![Block {
					init: vec![Op::Set(raw(&[0x10]), Val(Size::Large, 0))],
					xts: vec![Xt {
						pre: vec![Op::Clear(raw(&[0x10])), Op::Estimate],
						body: vec![],
						body_outcome: Outcome::Commit,
						post: vec![],
						client_outcome: Outcome::Commit,
						with_reclaim: false,
					}],
					finalize: vec![],
				}],
			},
		),
		(
			// The mirror image: `init` deletes `[0x10]`, extrinsic 0 re-inserts it before the
			// estimation. Collator: the older `Deleted` overwrites the newer `Updated`, the
			// estimation removes the key (141). Validator: `Updated`, reads it (106). Every
			// collator-side check passes (the collator only overestimates); only the validator
			// pass sees the failure.
			"validator_pre_reinsert_after_init_delete",
			Script {
				config: free_v1(),
				initial: InitialTrie {
					seed: 0,
					top: 0,
					extra_top: vec![(raw(&[0x10]), val64(0)), (raw(&[0x20]), val64(1))],
					children: vec![],
				},
				blocks: vec![Block {
					init: vec![Op::Clear(raw(&[0x10]))],
					xts: vec![Xt {
						pre: vec![Op::Set(raw(&[0x10]), Val(Size::Large, 0)), Op::Estimate],
						body: vec![],
						body_outcome: Outcome::Commit,
						post: vec![],
						client_outcome: Outcome::Commit,
						with_reclaim: false,
					}],
					finalize: vec![],
				}],
			},
		),
		(
			// Estimation inside a nested runtime transaction, the delete outside of it. The
			// validator's runtime transaction is one layer above the merged write and delete;
			// the collator's sits above a host layer with the delete and a block layer with the
			// write. Collator 106, validator 141. With the delete inside the nested transaction
			// both sides see the same layers and agree (both overshoot).
			"validator_nested_estimate_after_delete",
			Script {
				config: free_v1(),
				initial: InitialTrie {
					seed: 0,
					top: 0,
					extra_top: vec![(raw(&[0x10]), val64(0)), (raw(&[0x20]), val64(1))],
					children: vec![],
				},
				blocks: vec![Block {
					init: vec![Op::Set(raw(&[0x10]), Val(Size::Large, 0))],
					xts: vec![Xt {
						pre: vec![
							Op::Clear(raw(&[0x10])),
							Op::Tx(vec![Op::Estimate], Outcome::Commit),
						],
						body: vec![],
						body_outcome: Outcome::Commit,
						post: vec![],
						client_outcome: Outcome::Commit,
						with_reclaim: false,
					}],
					finalize: vec![],
				}],
			},
		),
		(
			// Found by `random_smoke` (seed 0x5eed0f1027b, shrunk). Each child write also marks
			// the child root key in the main trie: `Updated` in extrinsic 0 (reported by its
			// post-dispatch estimation), `Deleted` in extrinsic 1 (pending), `Updated` in
			// `finalize`. The final estimation suppresses the newest `Updated` as already reported
			// and then reports the older pending `Deleted`: the collator removes the root key, the
			// validator, with all three in one layer, does not (124 against 122).
			"fuzz_child_writes_around_absent_delete",
			Script {
				config: Config { mode: Mode::Executive, ..free_v1() },
				initial: InitialTrie {
					seed: 8802166379867635177,
					top: 0,
					extra_top: vec![],
					children: vec![ChildInit {
						id: 0,
						bulk: 0,
						extra: vec![(Key::Map(151, 67, 42483), Val(Size::Small, 39))],
					}],
				},
				blocks: vec![Block {
					init: vec![],
					xts: vec![
						Xt {
							pre: vec![],
							body: vec![],
							body_outcome: Outcome::Commit,
							post: vec![Op::ChildSet(
								0,
								Key::Map(145, 35, 12337),
								Val(Size::Exact32, 142),
							)],
							client_outcome: Outcome::Commit,
							with_reclaim: true,
						},
						Xt {
							pre: vec![],
							body: vec![],
							body_outcome: Outcome::Commit,
							post: vec![Op::ChildClear(0, Key::Map(202, 180, 46417))],
							client_outcome: Outcome::Commit,
							with_reclaim: false,
						},
					],
					finalize: vec![Op::ChildSet(
						0,
						Key::Map(127, 165, 12844),
						Val(Size::Large, 192),
					)],
				}],
			},
		),
		(
			// Found by `random_smoke` (seed 0x5f65cac, shrunk). The mid-block child root of the
			// emptied child writes `None` for its root key into the main overlay. The final root
			// applies that stale `None` and then the refilled child's root: it removes and
			// reinserts the root key, reading a sibling the estimation never sees (+87 bytes).
			"fuzz_child_emptied_root_computed_then_refilled",
			Script {
				config: Config {
					mode: Mode::Executive,
					backend_version: Version::V1,
					root_version: Version::V0,
					..free_v1()
				},
				initial: InitialTrie {
					seed: 13069962731770108540,
					top: 0,
					extra_top: vec![(
						Key::Sibling(105, 200, 4261, 100, 132),
						Val(Size::Exact32, 220),
					)],
					children: vec![ChildInit {
						id: 1,
						bulk: 0,
						extra: vec![(Key::Map(204, 225, 20958), Val(Size::Exact32, 244))],
					}],
				},
				blocks: vec![Block {
					init: vec![],
					xts: vec![],
					finalize: vec![
						Op::ChildKill(1, None),
						Op::ChildRoot(1),
						Op::ChildSet(1, Key::Map(45, 219, 41671), Val(Size::Empty, 191)),
					],
				}],
			},
		),
		(
			// Found by `random_smoke` (seed 0x612359a, shrunk). The kill removes nothing, which
			// leaves an empty child change set and marks the child root key `Deleted`. Only the
			// collator rolls back the dropped extrinsic, which discards the empty change set, so
			// only the validator's later child root writes the root key (127 against 125).
			"fuzz_dropped_extrinsic_discards_empty_child_changes",
			Script {
				config: Config { mode: Mode::Executive, backend_version: Version::V0, ..free_v1() },
				initial: InitialTrie {
					seed: 9535333911132529172,
					top: 0,
					extra_top: vec![],
					children: vec![ChildInit {
						id: 1,
						bulk: 0,
						extra: vec![(Key::Map(72, 88, 26398), Val(Size::Small, 72))],
					}],
				},
				blocks: vec![Block {
					init: vec![Op::ChildKill(1, Some(10624))],
					xts: vec![Xt {
						pre: vec![],
						body: vec![],
						body_outcome: Outcome::Commit,
						post: vec![],
						client_outcome: Outcome::Rollback,
						with_reclaim: false,
					}],
					finalize: vec![Op::ChildRoot(1)],
				}],
			},
		),
		(
			// Found by `random_smoke` (seed 0x5fde0d7, shrunk). The mid-block storage root fills
			// the overlay's root cache; only the collator's rollback of the dropped extrinsic
			// clears it. The validator's child root then comes from the cache and writes nothing,
			// so the root key stays `Deleted` and its estimation reads nodes missing from the
			// proof: the validator panics.
			"fuzz_dropped_extrinsic_clears_root_cache",
			Script {
				config: Config { mode: Mode::Executive, backend_version: Version::V0, ..free_v1() },
				initial: InitialTrie {
					seed: 16601158210426672330,
					top: 0,
					extra_top: vec![(Key::Short(55, 0), Val(Size::Large, 162))],
					children: vec![],
				},
				blocks: vec![
					Block {
						init: vec![],
						xts: vec![],
						finalize: vec![Op::ChildSet(
							2,
							Key::Sibling(167, 202, 475, 15, 109),
							Val(Size::Small, 49),
						)],
					},
					Block {
						init: vec![Op::ChildClear(2, Key::Short(0, 0)), Op::Root],
						xts: vec![
							Xt {
								pre: vec![],
								body: vec![],
								body_outcome: Outcome::Commit,
								post: vec![],
								client_outcome: Outcome::Rollback,
								with_reclaim: false,
							},
							Xt {
								pre: vec![Op::ChildRoot(2)],
								body: vec![],
								body_outcome: Outcome::Commit,
								post: vec![],
								client_outcome: Outcome::Commit,
								with_reclaim: false,
							},
						],
						finalize: vec![],
					},
				],
			},
		),
		(
			// ext.rs `calculating_storage_root_should_not_change_storage_proof_for_main_trie_
			// changes`: updates, an insert and deletions, estimated once.
			"pr_main_trie_changes",
			Script {
				config: free_v1(),
				initial: InitialTrie {
					seed: 1,
					top: 0,
					extra_top: many_top.clone(),
					children: vec![ChildInit { id: 0, bulk: 100, extra: vec![] }],
				},
				blocks: only_block(vec![
					Op::Set(Key::Map(1, 1, 1), Val(Size::Small, 30)),
					Op::Set(Key::Map(3, 0, 250), Val(Size::Small, 40)),
					Op::Clear(Key::Map(0, 0, 0)),
					Op::Clear(Key::Map(2, 2, 2)),
				]),
			},
		),
		(
			// ext.rs `..._when_child_is_created`.
			"pr_child_is_created",
			Script {
				config: free_v1(),
				initial: InitialTrie {
					seed: 2,
					top: 0,
					extra_top: many_top.clone(),
					children: vec![],
				},
				blocks: only_block(vec![Op::ChildSet(
					0,
					raw(&1u32.to_le_bytes()),
					Val(Size::Small, 30),
				)]),
			},
		),
		(
			// ext.rs `..._when_child_is_emptied`.
			"pr_child_is_emptied",
			Script {
				config: free_v1(),
				initial: InitialTrie {
					seed: 3,
					top: 0,
					extra_top: many_top.clone(),
					children: vec![ChildInit {
						id: 0,
						bulk: 0,
						extra: (0..3u32)
							.map(|i| (raw(&i.to_le_bytes()), Val(Size::Small, 30)))
							.collect(),
					}],
				},
				blocks: only_block(
					(0..3u32).map(|i| Op::ChildClear(0, raw(&i.to_le_bytes()))).collect(),
				),
			},
		),
		(
			// Executive shape, inserts and updates only, two blocks.
			"executive_inserts_and_updates",
			Script {
				config: Config { mode: Mode::Executive, cache: true, ..free_v1() },
				initial: InitialTrie {
					seed: 4,
					top: 300,
					extra_top: vec![],
					children: vec![ChildInit { id: 1, bulk: 50, extra: vec![] }],
				},
				blocks: vec![
					Block {
						init: vec![Op::Set(Key::Value(0, 0), Val(Size::Small, 1))],
						xts: (0..4u8)
							.map(|i| Xt {
								pre: vec![Op::Set(Key::Map(1, 0, i as u16), Val(Size::Exact32, i))],
								body: vec![
									Op::Set(Key::Map(2, 1, 100 + i as u16), Val(Size::Large, i)),
									Op::ChildSet(1, Key::Map(0, 0, i as u16), Val(Size::Large, i)),
								],
								body_outcome: Outcome::Commit,
								post: vec![Op::Append(Key::Value(3, 2), Val(Size::Small, i))],
								client_outcome: if i == 2 {
									Outcome::Rollback
								} else {
									Outcome::Commit
								},
								with_reclaim: true,
							})
							.collect(),
						finalize: vec![Op::Set(Key::Value(0, 1), Val(Size::Exact32, 9))],
					};
					2
				],
			},
		),
	]
}
