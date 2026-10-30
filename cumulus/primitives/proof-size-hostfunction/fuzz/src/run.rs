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

//! Estimation runner.
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

use crate::{
	child_info, install_hooks, limit, mark, panic_message, shrink_with, take_warnings, with_ext,
	Block, BlockEnv, ChildInit, Config, Hasher, InitialTrie, Key, Mode, Op, Outcome, Scale, Script,
	Size, State, Val, Version, Xt, PANIC_LOCATION, QUIET_PANICS, WARNINGS,
};
use cumulus_primitives_proof_size_hostfunction::storage_proof_size;
use sp_core::{storage::StateVersion, H256};
use sp_externalities::Extensions;
use sp_state_machine::{Ext, OverlayedChanges, TrieBackend, TrieBackendBuilder};
use sp_trie::{
	cache::{CacheSize, LocalTrieCache, SharedTrieCache},
	proof_size_extension::ProofSizeExt,
	recorder::Recorder,
	MemoryDB, StorageProof,
};
use std::{
	cell::Cell,
	fmt,
	panic::{self, AssertUnwindSafe},
};

/// Backend of the validator pass: the collator's proof as the only database.
type ValidatorBackend =
	TrieBackend<MemoryDB<Hasher>, Hasher, LocalTrieCache<Hasher>, Recorder<Hasher>>;

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
	static CURRENT_BLOCK: Cell<usize> = const { Cell::new(0) };
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
			Op::Append(k, v) => with_ext(|ext| ext.storage_append_v2(k.bytes(scale), v.bytes())),
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
