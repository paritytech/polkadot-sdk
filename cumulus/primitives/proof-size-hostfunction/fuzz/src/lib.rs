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
//! execution. The estimation runner (`run`, behind the default feature `estimation`, see
//! `try_run_script`) estimates as version 2 of `storage_proof_size` does and checks the
//! estimation invariants. It needs `Externalities::record_proof_for_dirty_keys`, which only
//! exists on the PR branch; without the feature the crate builds against master.
//!
//! [`trace`] runs a script as a runtime built against master does, with version 1 of
//! `storage_proof_size`, and returns a text trace. The `trace` binary does the same for a script
//! read from stdin; built on master, it is the reference for `tests/v1_compat.rs`.

use arbitrary::{Arbitrary, Result as ArbitraryResult, Unstructured};
use codec::{Decode, Encode};
use sp_core::{
	storage::{ChildInfo, StateVersion, Storage, StorageChild},
	Blake2Hasher,
};
use sp_externalities::{Extensions, TransactionType};
use sp_state_machine::{Ext, InMemoryBackend, OverlayedChanges, TrieBackend, TrieBackendBuilder};
use sp_trie::{
	cache::{LocalTrieCache, SharedTrieCache},
	proof_size_extension::ProofSizeExt,
	recorder::Recorder,
	PrefixedMemoryDB,
};
use std::{
	cell::{Cell, RefCell},
	collections::BTreeMap,
	fmt, panic,
	sync::{Once, OnceLock},
};

#[cfg(feature = "estimation")]
mod run;
#[cfg(feature = "estimation")]
pub use run::*;
mod trace;

pub use trace::{trace, MAX_SHOWN_BYTES};

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

/// State version selector.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
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
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
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
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum Mode {
	/// Estimations only at the fixed points of the executive: before and after each extrinsic
	/// with the reclaim extension, and at the end of the block. [`Op::Estimate`] is a no-op.
	Executive,
	/// Like [`Mode::Executive`], and additionally every [`Op::Estimate`] estimates.
	Free,
}

/// Outcome of a transaction.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum Outcome {
	Commit,
	Rollback,
}

/// Size class of a value.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
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
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
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
#[derive(Arbitrary, Debug, Clone, PartialEq, Eq, Encode, Decode)]
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ChildInit {
	/// Child id (`% 4`). The child storage key is `[0x10 * (id % 4 + 1)]`.
	pub id: u8,
	/// Number of generated entries (from the script seed).
	pub bulk: u32,
	/// Explicit entries.
	pub extra: Vec<(Key, Val)>,
}

/// Initial state.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
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
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
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

	/// Removes every [`Op::Root`] and [`Op::ChildRoot`], also inside transactions, so the
	/// `storage_root` at the end of each block is the only root computation, as in FRAME.
	pub fn drop_mid_block_roots(&mut self) {
		fn drop_roots(ops: &mut Vec<Op>) {
			ops.retain(|op| !matches!(op, Op::Root | Op::ChildRoot(_)));
			for op in ops {
				if let Op::Tx(inner, _) = op {
					drop_roots(inner);
				}
			}
		}
		for block in &mut self.blocks {
			drop_roots(&mut block.init);
			drop_roots(&mut block.finalize);
			for xt in &mut block.xts {
				drop_roots(&mut xt.pre);
				drop_roots(&mut xt.body);
				drop_roots(&mut xt.post);
			}
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

thread_local! {
	static WARNINGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
	static QUIET_PANICS: Cell<bool> = const { Cell::new(false) };
	static PANIC_LOCATION: Cell<Option<String>> = const { Cell::new(None) };
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

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
	payload
		.downcast_ref::<&str>()
		.map(|s| s.to_string())
		.or_else(|| payload.downcast_ref::<String>().cloned())
		.unwrap_or_else(|| "<non-string panic payload>".into())
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
