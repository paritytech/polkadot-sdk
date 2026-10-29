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

//! Version 1 trace runner.
//!
//! [`trace`] executes a [`Script`] the way a runtime built against master does: plain `sp_io`
//! calls with proof recording on, version 1 of `storage_proof_size` for [`Op::Estimate`] and the
//! weight-reclaim points, and never `Externalities::record_proof_for_dirty_keys`. Blocks are
//! built as in the estimation runner, including the block builder's rollback of dropped
//! extrinsics. Every observable result goes into a text trace, so that the same script run on
//! master and on the branch can be compared line by line:
//!
//! - `script <hash of the encoded script> <config> seed .. top .. extra .. children .. blocks ..`
//! - `<n> <operation> -> <return value> | proof <storage_proof_size after it>`, one per host call
//!   in the runtime and one per end of a client transaction;
//! - `block <i> root .. proof nodes .. size .. hash .. changes main .. child .. hash .. transaction
//!   .. hash ..` after the overlay is drained;
//! - `warning: ..` for each `trie` or `state` warning;
//! - `panic: <message>` as the last line if anything panicked.
//!
//! Byte strings longer than [`MAX_SHOWN_BYTES`] and all sets are shown as a length and a hash.

use crate::{
	child_info, install_hooks, limit, mark, panic_message, take_warnings, BlockEnv, Hasher, Op,
	Outcome, Scale, Script, State, Xt, QUIET_PANICS,
};
use codec::Encode;
use cumulus_primitives_proof_size_hostfunction::storage_proof_size;
use sp_core::{hexdisplay::HexDisplay, storage::StateVersion, H256};
use sp_io::{hashing::blake2_256, KillStorageResult};
use sp_trie::{
	cache::{CacheSize, SharedTrieCache},
	PrefixedMemoryDB,
};
use std::{
	fmt,
	panic::{self, AssertUnwindSafe},
};

/// Longest byte string shown in full.
pub const MAX_SHOWN_BYTES: usize = 64;

/// The trace of `script`, one line per entry. Never panics because of the script.
pub fn trace(script: &Script) -> Vec<String> {
	install_hooks();
	let mut tracer = Tracer {
		scale: script.config.scale,
		root_version: script.config.root_version.state_version(),
		lines: vec![header(script)],
		next_op: 0,
	};
	let quiet = QUIET_PANICS.with(|q| q.replace(true));
	let result = panic::catch_unwind(AssertUnwindSafe(|| tracer.run(script)));
	QUIET_PANICS.with(|q| q.set(quiet));
	if let Err(payload) = result {
		tracer.lines.push(format!("panic: {}", panic_message(&*payload)));
	}
	tracer.lines
}

fn header(script: &Script) -> String {
	let initial = &script.initial;
	format!(
		"script {} {:?} seed {:#x} top {} extra {} children {} blocks {}",
		short_hash(&script.encode()),
		script.config,
		initial.seed,
		initial.top,
		initial.extra_top.len(),
		initial.children.len(),
		script.blocks.len()
	)
}

struct Tracer {
	scale: Scale,
	root_version: StateVersion,
	lines: Vec<String>,
	next_op: usize,
}

impl Tracer {
	fn run(&mut self, script: &Script) {
		let config = &script.config;
		let mut state: State =
			(script.initial.storage(config.scale), config.backend_version.state_version()).into();
		let shared_cache = config
			.cache
			.then(|| SharedTrieCache::<Hasher>::new(CacheSize::new(64 << 20), None));
		let _ = take_warnings();

		for (block_index, block) in script.blocks.iter().enumerate() {
			mark(format_args!("trace, block {block_index}"));
			let mut env = BlockEnv::new(&state, shared_cache.as_ref(), self.root_version);
			self.runtime_call(&mut env, "Init", Outcome::Commit, |tracer| {
				tracer.exec_ops(&block.init)
			});
			for (xt_index, xt) in block.xts.iter().enumerate() {
				self.runtime_call(
					&mut env,
					&format!("Xt({xt_index})"),
					xt.client_outcome,
					|tracer| tracer.apply_extrinsic(xt),
				);
			}
			self.runtime_call(&mut env, "Finalize", Outcome::Commit, |tracer| {
				tracer.exec_ops(&block.finalize);
				let root_version = tracer.root_version;
				tracer.step("FinalRoot", || show_bytes(&sp_io::storage::root(root_version)));
			});
			let (root, transaction) = self.finish_block(block_index, env);
			state.apply_transaction(root, transaction);
		}
	}

	/// One runtime call in its own client transaction, as `sp-api` runs it. The block builder
	/// rolls the transaction back for an extrinsic it drops.
	fn runtime_call(
		&mut self,
		env: &mut BlockEnv,
		call: &str,
		outcome: Outcome,
		body: impl FnOnce(&mut Self),
	) {
		env.client_start();
		env.in_runtime(|| body(self));
		env.client_end(outcome);
		self.push(
			format_args!("Client{outcome:?}({call})"),
			"()",
			env.recorder.estimate_encoded_size(),
		);
	}

	/// Drains the overlay as `sc-block-builder` does and records what the node keeps of the
	/// block. Returns the post-block root and the backend transaction.
	fn finish_block(
		&mut self,
		block_index: usize,
		env: BlockEnv,
	) -> (H256, PrefixedMemoryDB<Hasher>) {
		let backend = env.backend();
		let BlockEnv { mut overlay, recorder, .. } = env;
		let mut changes = overlay
			.drain_storage_changes(&backend, self.root_version)
			.expect("in-memory backend does not fail; qed");
		drop(backend);

		let proof = recorder.to_storage_proof();
		let proof_size = proof.encoded_size();
		let nodes = proof.into_nodes();
		// Child changes are drained from a `HashMap`.
		changes.child_storage_changes.sort();
		let mut transaction: Vec<_> = changes
			.transaction
			.clone()
			.drain()
			.into_iter()
			// Entries with reference count zero do nothing when the transaction is applied.
			.filter(|(_, (_, rc))| *rc != 0)
			.collect();
		transaction.sort();
		self.lines.push(format!(
			"block {block_index} root {} proof nodes {} size {proof_size} hash {} changes main {} \
			 child {} hash {} transaction {} hash {}",
			show_bytes(changes.transaction_storage_root.as_ref()),
			nodes.len(),
			short_hash(&nodes.encode()),
			changes.main_storage_changes.len(),
			changes.child_storage_changes.len(),
			short_hash(&(&changes.main_storage_changes, &changes.child_storage_changes).encode()),
			transaction.len(),
			short_hash(&transaction.encode()),
		));
		(changes.transaction_storage_root, changes.transaction)
	}

	/// Runtime side of `apply_extrinsic`, as in the estimation runner.
	fn apply_extrinsic(&mut self, xt: &Xt) {
		if xt.with_reclaim {
			self.step("ReclaimPre", proof_size);
		}
		self.exec_ops(&xt.pre);
		self.step("BodyStart", || {
			sp_io::storage::start_transaction();
			"()".into()
		});
		self.exec_ops(&xt.body);
		self.end_transaction("Body", xt.body_outcome);
		self.exec_ops(&xt.post);
		if xt.with_reclaim {
			self.step("ReclaimPost", proof_size);
		}
	}

	fn exec_ops(&mut self, ops: &[Op]) {
		for op in ops {
			self.exec_op(op);
		}
	}

	fn exec_op(&mut self, op: &Op) {
		match op {
			Op::Tx(ops, outcome) => {
				self.step("TxStart", || {
					sp_io::storage::start_transaction();
					"()".into()
				});
				self.exec_ops(ops);
				self.end_transaction("Tx", *outcome);
			},
			_ => {
				let (scale, root_version) = (self.scale, self.root_version);
				self.step(format_args!("{op:?}"), || call(op, scale, root_version));
			},
		}
	}

	fn end_transaction(&mut self, layer: &str, outcome: Outcome) {
		self.step(format_args!("{layer}{outcome:?}"), || {
			match outcome {
				Outcome::Commit => sp_io::storage::commit_transaction(),
				Outcome::Rollback => sp_io::storage::rollback_transaction(),
			}
			"()".into()
		});
	}

	/// Runs `host_call` in the runtime and records its result and the proof size after it.
	fn step(&mut self, description: impl fmt::Display, host_call: impl FnOnce() -> String) {
		let result = host_call();
		self.push(description, result, storage_proof_size::storage_proof_size());
	}

	fn push(
		&mut self,
		description: impl fmt::Display,
		result: impl fmt::Display,
		proof_size: impl fmt::Display,
	) {
		self.lines
			.push(format!("{} {description} -> {result} | proof {proof_size}", self.next_op));
		self.next_op += 1;
		self.lines
			.extend(take_warnings().into_iter().map(|warning| format!("warning: {warning}")));
	}
}

fn proof_size() -> String {
	storage_proof_size::storage_proof_size().to_string()
}

/// Executes one storage op, not [`Op::Tx`], through `sp_io` and shows its return value.
fn call(op: &Op, scale: Scale, root_version: StateVersion) -> String {
	use sp_io::{default_child_storage as child, storage};
	match op {
		Op::Set(k, v) => {
			storage::set(&k.bytes(scale), &v.bytes());
			"()".into()
		},
		Op::Clear(k) => {
			storage::clear(&k.bytes(scale));
			"()".into()
		},
		Op::Append(k, v) => {
			storage::append(&k.bytes(scale), v.bytes());
			"()".into()
		},
		Op::Get(k) => show_option(storage::get(&k.bytes(scale)).as_deref()),
		Op::Exists(k) => storage::exists(&k.bytes(scale)).to_string(),
		Op::NextKey(k) => show_option(storage::next_key(&k.bytes(scale)).as_deref()),
		Op::ClearPrefix(p, l) => show_kill(storage::clear_prefix(&p.bytes(scale), limit(l, scale))),
		Op::ChildSet(c, k, v) => {
			child::set(child_info(*c).storage_key(), &k.bytes(scale), &v.bytes());
			"()".into()
		},
		Op::ChildClear(c, k) => {
			child::clear(child_info(*c).storage_key(), &k.bytes(scale));
			"()".into()
		},
		Op::ChildGet(c, k) => {
			show_option(child::get(child_info(*c).storage_key(), &k.bytes(scale)).as_deref())
		},
		Op::ChildKill(c, l) => {
			show_kill(child::storage_kill(child_info(*c).storage_key(), limit(l, scale)))
		},
		Op::ChildClearPrefix(c, p, l) => show_kill(child::clear_prefix(
			child_info(*c).storage_key(),
			&p.bytes(scale),
			limit(l, scale),
		)),
		Op::ChildRoot(c) => show_bytes(&child::root(child_info(*c).storage_key(), root_version)),
		Op::Root => show_bytes(&storage::root(root_version)),
		Op::Estimate => proof_size(),
		Op::Tx(..) => unreachable!("`Tracer::exec_op` traces transactions op by op; qed"),
	}
}

fn show_bytes(bytes: &[u8]) -> String {
	if bytes.len() <= MAX_SHOWN_BYTES {
		format!("0x{}", HexDisplay::from(&bytes))
	} else {
		format!("{} bytes {}", bytes.len(), short_hash(bytes))
	}
}

fn show_option(value: Option<&[u8]>) -> String {
	value.map_or_else(|| "None".into(), show_bytes)
}

fn show_kill(result: KillStorageResult) -> String {
	match result {
		KillStorageResult::AllRemoved(removed) => format!("AllRemoved({removed})"),
		KillStorageResult::SomeRemaining(removed) => format!("SomeRemaining({removed})"),
	}
}

/// The first 8 bytes of the BLAKE2-256 hash, enough to tell traces apart.
fn short_hash(data: &[u8]) -> String {
	format!("#{}", HexDisplay::from(&&blake2_256(data)[..8]))
}
