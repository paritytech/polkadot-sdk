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

//! Version 1 compatibility with master, by differential execution.
//!
//! Old runtimes call version 1 of `storage_proof_size` forever, and weight reclaim writes the
//! value it returns into `frame_system::BlockWeight`. So for every sequence of host calls this
//! tree must return what master returns, at every call, and end every block with the same root,
//! recorded proof and storage changes. `trace` executes a script as such a runtime does and
//! writes all of that into a text trace. Two builds of it are compared line by line:
//!
//! - this test, built on the branch, traces in-process;
//! - the `trace` binary of this crate, built on master with `--no-default-features` (the estimation
//!   runner needs API master does not have), reads the SCALE-encoded script on stdin and prints
//!   master's trace. The master tree must patch `trie-db` and `hash-db` to the same source as this
//!   tree, otherwise the comparison also covers the trie-db update. The README explains how to
//!   build it.
//!
//! `v1_compat_random` generates scripts as `random_smoke` does and fails at the first difference
//! with the seed, both lines, and the script shrunk to the smallest one whose traces still differ.
//! `v1_compat_pinned` compares hand-picked scripts with master traces stored in this file, so it
//! runs without the master binary.
//!
//! Environment variables:
//! - `POV_FUZZ_MASTER_BIN`: the master `trace` binary. `v1_compat_random` fails without it;
//!   `v1_compat_pinned` uses it to check that the stored traces are still what master prints.
//! - `POV_FUZZ_ITERS`, `POV_FUZZ_SEED`, `POV_FUZZ_SCALE`, `POV_FUZZ_ROOT_LAST`: as in `smoke.rs`.
//! - `POV_FUZZ_KEEP_GOING=1`: run all scripts, then print the differing seeds grouped by the
//!   operation at the first differing line and shrink the first seed of each group.

mod common;

use codec::Encode;
use common::{env_scale, env_u64, generate};
use cumulus_proof_size_hostfunction_fuzz::{
	op_count, pinned_scripts, script_from_seed, shrink_with, trace, Block, ChildInit, Config,
	InitialTrie, Key, Mode, Op, Outcome, Scale, Script, Size, Val, Version, Xt,
};
use std::{
	collections::BTreeMap,
	fmt,
	io::Write,
	process::{Child, Command, Stdio},
};

const MASTER_BIN: &str = "POV_FUZZ_MASTER_BIN";

/// Starts the master `trace` binary on `script`.
fn spawn_master(script: &Script) -> Child {
	let bin = std::env::var(MASTER_BIN)
		.unwrap_or_else(|_| panic!("{MASTER_BIN} must name the master `trace` binary, see README"));
	let mut child = Command::new(&bin)
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap_or_else(|error| panic!("cannot run {MASTER_BIN}={bin}: {error}"));
	child
		.stdin
		.take()
		.expect("stdin is piped; qed")
		.write_all(&script.encode())
		.unwrap_or_else(|error| panic!("cannot write the script to {bin}: {error}"));
	child
}

fn master_output(child: Child) -> Vec<String> {
	let output = child.wait_with_output().expect("the master binary was started; qed");
	assert!(
		output.status.success(),
		"the master binary failed with {}: {}",
		output.status,
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout)
		.expect("traces are UTF-8")
		.lines()
		.map(String::from)
		.collect()
}

/// The branch's and master's trace of `script`, master running while the branch traces.
fn traces(script: &Script) -> (Vec<String>, Vec<String>) {
	let master = spawn_master(script);
	let branch = trace(script);
	(branch, master_output(master))
}

fn master_trace(script: &Script) -> Vec<String> {
	master_output(spawn_master(script))
}

struct Difference {
	/// Zero-based line number.
	line: usize,
	branch: Option<String>,
	master: Option<String>,
}

impl Difference {
	/// The operation at the differing line, `Branch|Master` if the two lines are about different
	/// operations, for example `ClearPrefix|panic`.
	fn kind(&self) -> String {
		let branch = self.branch.as_deref().map_or("end", operation);
		let master = self.master.as_deref().map_or("end", operation);
		if branch == master {
			branch.to_string()
		} else {
			format!("{branch}|{master}")
		}
	}
}

impl fmt::Display for Difference {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let show = |line: &Option<String>| line.clone().unwrap_or_else(|| "<end of trace>".into());
		write!(
			f,
			"traces differ at line {}:\n  branch: {}\n  master: {}",
			self.line,
			show(&self.branch),
			show(&self.master)
		)
	}
}

/// The operation a trace line is about: `Append` for `12 Append(Map(..), ..) -> () | proof 0`,
/// `block`, `script`, `panic`, `warning`.
fn operation(line: &str) -> &str {
	let mut words = line.split(' ');
	let first = words.next().unwrap_or_default();
	let entry =
		if first.parse::<usize>().is_ok() { words.next().unwrap_or_default() } else { first };
	entry.split('(').next().unwrap_or_default().trim_end_matches(':')
}

fn first_difference(branch: &[String], master: &[String]) -> Option<Difference> {
	(0..branch.len().max(master.len()))
		.find(|&index| branch.get(index) != master.get(index))
		.map(|index| Difference {
			line: index,
			branch: branch.get(index).cloned(),
			master: master.get(index).cloned(),
		})
}

fn traces_differ(script: &Script) -> bool {
	let (branch, master) = traces(script);
	first_difference(&branch, &master).is_some()
}

/// Shrinks the script to the smallest one whose traces still differ and prints it with both
/// traces.
fn report_shrunk(seed: u64, script: Script, pretty: bool) {
	let ops = op_count(&script);
	let shrunk = shrink_with(script, traces_differ);
	let (branch, master) = traces(&shrunk);
	let difference = first_difference(&branch, &master)
		.map_or_else(|| "traces no longer differ?".into(), |difference| difference.to_string());
	let shrunk_ops = op_count(&shrunk);
	println!("shrunk seed {seed:#x} from {ops} to {shrunk_ops} ops, {difference}");
	if pretty {
		println!("{shrunk:#?}");
	} else {
		println!("{shrunk:?}");
	}
	println!("branch trace:\n{}\nmaster trace:\n{}", branch.join("\n"), master.join("\n"));
}

#[test]
fn v1_compat_random() {
	let iters = env_u64("POV_FUZZ_ITERS", 300);
	let base = env_u64("POV_FUZZ_SEED", 0x5eed_0f_10215);
	let keep_going = std::env::var("POV_FUZZ_KEEP_GOING").is_ok_and(|v| v == "1");
	let max_scale = env_scale();

	let mut compared_lines = 0;
	let mut same_panics = 0;
	let mut differences: Vec<(u64, String)> = Vec::new();
	for i in 0..iters {
		let seed = base.wrapping_add(i);
		let script = generate(seed, max_scale);
		let (branch, master) = traces(&script);
		compared_lines += branch.len().max(master.len());
		match first_difference(&branch, &master) {
			None if branch.last().is_some_and(|line| line.starts_with("panic:")) => {
				same_panics += 1
			},
			None => {},
			Some(difference) => {
				println!("seed {seed:#x} (iteration {i}): {difference}");
				if !keep_going {
					report_shrunk(seed, script, true);
					panic!("seed {seed:#x}: {difference}");
				}
				differences.push((seed, difference.kind()));
			},
		}
	}
	println!(
		"compared {iters} scripts, {compared_lines} trace lines; {} differ; {same_panics} traces \
		 end in the same panic on both sides",
		differences.len()
	);

	if differences.is_empty() {
		return;
	}
	let mut by_kind: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
	for (seed, kind) in &differences {
		by_kind.entry(kind).or_default().push(*seed);
	}
	println!(
		"{} of {iters} scripts differ, by operation at the first difference:",
		differences.len()
	);
	for (kind, seeds) in &by_kind {
		let seeds: Vec<_> = seeds.iter().map(|seed| format!("{seed:#x}")).collect();
		println!("  {kind}: {} ({})", seeds.len(), seeds.join(", "));
	}
	for seeds in by_kind.values() {
		report_shrunk(seeds[0], generate(seeds[0], max_scale), false);
	}
	panic!("{} of {iters} scripts differ", differences.len());
}

/// The branch must print the stored master trace of every pinned script.
#[test]
fn v1_compat_pinned() {
	let check_master = std::env::var(MASTER_BIN).is_ok();
	let mut failed = Vec::new();
	for (name, script, stored) in pinned() {
		let stored: Vec<String> = stored.trim().lines().map(String::from).collect();
		if let Some(difference) = first_difference(&trace(&script), &stored) {
			println!("pinned {name}: branch against stored master trace: {difference}");
			failed.push(format!("{name} (branch differs)"));
		}
		if check_master {
			let master = master_trace(&script);
			if first_difference(&master, &stored).is_some() {
				println!(
					"pinned {name}: stored trace is stale, master prints:\n{}",
					master.join("\n")
				);
				failed.push(format!("{name} (stored trace stale)"));
			}
		}
	}
	assert!(failed.is_empty(), "pinned scripts failed: {failed:?}");
}

/// Script encoding is what makes the two sides trace the same script: the master binary must
/// decode exactly the script the branch traces.
#[test]
fn script_encoding_round_trips() {
	use codec::DecodeAll;
	for seed in 0..200 {
		let script = script_from_seed(seed, Scale::Large);
		assert_eq!(Script::decode_all(&mut &script.encode()[..]), Ok(script), "seed {seed}");
	}
}

fn executive(scale: Scale, backend_version: Version, root_version: Version) -> Config {
	Config { scale, backend_version, root_version, cache: false, mode: Mode::Executive }
}

fn initial(seed: u64, extra_top: Vec<(Key, Val)>, children: Vec<ChildInit>) -> InitialTrie {
	InitialTrie { seed, top: 0, extra_top, children }
}

fn xt(body: Vec<Op>, post: Vec<Op>, client_outcome: Outcome) -> Xt {
	Xt {
		pre: vec![],
		body,
		body_outcome: Outcome::Commit,
		post,
		client_outcome,
		with_reclaim: false,
	}
}

fn raw(bytes: &[u8]) -> Key {
	Key::Raw(bytes.to_vec())
}

fn estimation_pinned(name: &str) -> Script {
	pinned_scripts()
		.into_iter()
		.find_map(|(pinned, script)| (pinned == name).then_some(script))
		.unwrap_or_else(|| panic!("`pinned_scripts` has no {name}"))
}

/// Child 1 (storage key `[0x20]`) with one entry, and main trie keys on both sides of its
/// prefixed root key `:child_storage:default:\x20`; `[0x30]` shares its first nibble.
fn one_child_between_two_keys() -> InitialTrie {
	initial(
		0,
		vec![(raw(&[0x30]), Val(Size::Large, 1)), (raw(&[0x50]), Val(Size::Large, 2))],
		vec![ChildInit { id: 1, bulk: 0, extra: vec![(raw(&[0x01]), Val(Size::Large, 3))] }],
	)
}

fn pinned() -> Vec<(&'static str, Script, &'static str)> {
	let only_init = |init| vec![Block { init, xts: vec![], finalize: vec![] }];
	vec![
		(
			// Seed 0x5f5e1a3, shrunk: an append after a mid-block storage root, then a dropped
			// extrinsic. Master's append keeps the cached root; the block builder's rollback
			// clears the cache.
			"append_after_mid_block_root_then_dropped_extrinsic",
			Script {
				config: executive(Scale::Small, Version::V0, Version::V0),
				initial: initial(15223672883219400523, vec![], vec![]),
				blocks: vec![Block {
					init: vec![],
					xts: vec![
						xt(
							vec![],
							vec![
								Op::Root,
								Op::Append(Key::Map(161, 135, 48440), Val(Size::Small, 88)),
							],
							Outcome::Commit,
						),
						xt(vec![], vec![], Outcome::Rollback),
					],
					finalize: vec![],
				}],
			},
			APPEND_AFTER_MID_BLOCK_ROOT_THEN_DROPPED_EXTRINSIC,
		),
		(
			// The same without the dropped extrinsic, as block import executes it: master's final
			// root is the cached root from before the append.
			"append_after_mid_block_root",
			Script {
				config: executive(Scale::Small, Version::V0, Version::V0),
				initial: initial(15223672883219400523, vec![], vec![]),
				blocks: vec![Block {
					init: vec![],
					xts: vec![xt(
						vec![],
						vec![Op::Root, Op::Append(Key::Map(161, 135, 48440), Val(Size::Small, 88))],
						Outcome::Commit,
					)],
					finalize: vec![],
				}],
			},
			APPEND_AFTER_MID_BLOCK_ROOT,
		),
		(
			// Seed 0x5f5e837, shrunk: storage root in an extrinsic, a dropped extrinsic, an
			// append in `finalize`.
			"append_in_finalize_after_root_and_dropped_extrinsic",
			Script {
				config: executive(Scale::Small, Version::V1, Version::V0),
				initial: initial(17946649861943345497, vec![], vec![]),
				blocks: vec![Block {
					init: vec![],
					xts: vec![
						xt(vec![Op::Root], vec![], Outcome::Commit),
						xt(vec![], vec![], Outcome::Rollback),
					],
					finalize: vec![Op::Append(Key::Short(104, 0), Val(Size::Empty, 51))],
				}],
			},
			APPEND_IN_FINALIZE_AFTER_ROOT_AND_DROPPED_EXTRINSIC,
		),
		(
			// Master writes the child root into the main overlay; after the storage root the
			// second child root comes from the root cache and reads that stale value.
			"child_root_child_write_root_child_root",
			Script {
				config: executive(Scale::Small, Version::V1, Version::V1),
				initial: one_child_between_two_keys(),
				blocks: only_init(vec![
					Op::ChildRoot(1),
					Op::ChildSet(1, raw(&[0x02]), Val(Size::Small, 4)),
					Op::Root,
					Op::ChildRoot(1),
				]),
			},
			CHILD_ROOT_CHILD_WRITE_ROOT_CHILD_ROOT,
		),
		(
			// The child root of the emptied child deletes its prefixed root key in the main
			// overlay on master, so `next_key` skips it.
			"next_key_across_emptied_child_root_key",
			Script {
				config: executive(Scale::Small, Version::V1, Version::V1),
				initial: one_child_between_two_keys(),
				blocks: only_init(vec![
					Op::ChildKill(1, None),
					Op::ChildRoot(1),
					Op::NextKey(raw(&[0x30])),
				]),
			},
			NEXT_KEY_ACROSS_EMPTIED_CHILD_ROOT_KEY,
		),
		(
			// On master the final root applies the stale deletion of the prefixed root key before
			// the refilled child's root and records the collapsing sibling `[0x30]`.
			"child_kill_child_root_refill",
			Script {
				config: executive(Scale::Small, Version::V1, Version::V1),
				initial: one_child_between_two_keys(),
				blocks: only_init(vec![
					Op::ChildKill(1, None),
					Op::ChildRoot(1),
					Op::ChildSet(1, raw(&[0x02]), Val(Size::Large, 4)),
				]),
			},
			CHILD_KILL_CHILD_ROOT_REFILL,
		),
		(
			"fuzz_child_emptied_root_computed_then_refilled",
			estimation_pinned("fuzz_child_emptied_root_computed_then_refilled"),
			FUZZ_CHILD_EMPTIED_ROOT_COMPUTED_THEN_REFILLED,
		),
		(
			"fuzz_dropped_extrinsic_discards_empty_child_changes",
			estimation_pinned("fuzz_dropped_extrinsic_discards_empty_child_changes"),
			FUZZ_DROPPED_EXTRINSIC_DISCARDS_EMPTY_CHILD_CHANGES,
		),
		(
			"fuzz_dropped_extrinsic_clears_root_cache",
			estimation_pinned("fuzz_dropped_extrinsic_clears_root_cache"),
			FUZZ_DROPPED_EXTRINSIC_CLEARS_ROOT_CACHE,
		),
	]
}

// Master traces, printed by the master binary (`v1_compat_pinned` prints them when they are
// stale).

const APPEND_AFTER_MID_BLOCK_ROOT_THEN_DROPPED_EXTRINSIC: &str = r#"
script #01d398cd3a98835d Config { scale: Small, backend_version: V0, root_version: V0, cache: false, mode: Executive } seed 0xd34559d42d0a074b top 0 extra 0 children 0 blocks 1
0 ClientCommit(Init) -> () | proof 0
1 BodyStart -> () | proof 0
2 BodyCommit -> () | proof 0
3 Root -> 0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314 | proof 0
4 Append(Map(161, 135, 48440), Val(Small, 88)) -> () | proof 2
5 ClientCommit(Xt(0)) -> () | proof 2
6 BodyStart -> () | proof 2
7 BodyCommit -> () | proof 2
8 ClientRollback(Xt(1)) -> () | proof 2
9 FinalRoot -> 0x5ae7ea38c7c76c0235210fc50323d2fec4f3ec4fd5d12b8dd655d5e9f201b8ec | proof 2
10 ClientCommit(Finalize) -> () | proof 2
block 0 root 0x5ae7ea38c7c76c0235210fc50323d2fec4f3ec4fd5d12b8dd655d5e9f201b8ec proof nodes 1 size 3 hash #1a4c669d48eeaefe changes main 1 child 0 hash #2d208b8e0df9fe7f transaction 1 hash #0e8bcb556bc32901
"#;

const APPEND_AFTER_MID_BLOCK_ROOT: &str = r#"
script #de93f694897ccdf9 Config { scale: Small, backend_version: V0, root_version: V0, cache: false, mode: Executive } seed 0xd34559d42d0a074b top 0 extra 0 children 0 blocks 1
0 ClientCommit(Init) -> () | proof 0
1 BodyStart -> () | proof 0
2 BodyCommit -> () | proof 0
3 Root -> 0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314 | proof 0
4 Append(Map(161, 135, 48440), Val(Small, 88)) -> () | proof 2
5 ClientCommit(Xt(0)) -> () | proof 2
6 FinalRoot -> 0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314 | proof 2
7 ClientCommit(Finalize) -> () | proof 2
block 0 root 0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314 proof nodes 1 size 3 hash #1a4c669d48eeaefe changes main 1 child 0 hash #2d208b8e0df9fe7f transaction 0 hash #03170a2e7597b7b7
"#;

const APPEND_IN_FINALIZE_AFTER_ROOT_AND_DROPPED_EXTRINSIC: &str = r#"
script #166eacf9144c8937 Config { scale: Small, backend_version: V1, root_version: V0, cache: false, mode: Executive } seed 0xf90f4ef6c9724159 top 0 extra 0 children 0 blocks 1
0 ClientCommit(Init) -> () | proof 0
1 BodyStart -> () | proof 0
2 Root -> 0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314 | proof 0
3 BodyCommit -> () | proof 0
4 ClientCommit(Xt(0)) -> () | proof 0
5 BodyStart -> () | proof 0
6 BodyCommit -> () | proof 0
7 ClientRollback(Xt(1)) -> () | proof 0
8 Append(Short(104, 0), Val(Empty, 51)) -> () | proof 2
9 FinalRoot -> 0x3a836c48e649f1be9bf9b2c46451c491508f32a0f59fed0e111f5bd040f52815 | proof 2
10 ClientCommit(Finalize) -> () | proof 2
block 0 root 0x3a836c48e649f1be9bf9b2c46451c491508f32a0f59fed0e111f5bd040f52815 proof nodes 1 size 3 hash #1a4c669d48eeaefe changes main 1 child 0 hash #d52da04bb3d73822 transaction 1 hash #2d8f2ab3b15b77e6
"#;

const CHILD_ROOT_CHILD_WRITE_ROOT_CHILD_ROOT: &str = r#"
script #4896d6fcf3010a46 Config { scale: Small, backend_version: V1, root_version: V1, cache: false, mode: Executive } seed 0x0 top 0 extra 2 children 1 blocks 1
0 ChildRoot(1) -> 0xc603d61b45483edccbd53b00f92787dcde3f216f9851b0b5beb30264cba51c75 | proof 200
1 ChildSet(1, Raw([2]), Val(Small, 4)) -> () | proof 200
2 Root -> 0xd5f6ae663ff495ea365d5d777013539d65fc64e2f37b26ea19588b7abae09eda | proof 235
3 ChildRoot(1) -> 0xc603d61b45483edccbd53b00f92787dcde3f216f9851b0b5beb30264cba51c75 | proof 235
4 ClientCommit(Init) -> () | proof 235
5 FinalRoot -> 0xd5f6ae663ff495ea365d5d777013539d65fc64e2f37b26ea19588b7abae09eda | proof 235
6 ClientCommit(Finalize) -> () | proof 235
block 0 root 0xd5f6ae663ff495ea365d5d777013539d65fc64e2f37b26ea19588b7abae09eda proof nodes 4 size 236 hash #12404ed49d6b3655 changes main 0 child 1 hash #02117e1850136aa3 transaction 9 hash #e8e8582b01a9c57f
"#;

const NEXT_KEY_ACROSS_EMPTIED_CHILD_ROOT_KEY: &str = r#"
script #bd0b9e9b5c521582 Config { scale: Small, backend_version: V1, root_version: V1, cache: false, mode: Executive } seed 0x0 top 0 extra 2 children 1 blocks 1
0 ChildKill(1, None) -> AllRemoved(1) | proof 235
1 ChildRoot(1) -> 0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314 | proof 235
2 NextKey(Raw([48])) -> 0x50 | proof 304
3 ClientCommit(Init) -> () | proof 304
4 FinalRoot -> 0x91af6820103010010eccbff3e072b172e328c886c2f66557e6ef5fdd5ea0bb3e | proof 304
5 ClientCommit(Finalize) -> () | proof 304
block 0 root 0x91af6820103010010eccbff3e072b172e328c886c2f66557e6ef5fdd5ea0bb3e proof nodes 6 size 305 hash #7735f7cd593c7e9c changes main 1 child 1 hash #0a76a1a91168398e transaction 8 hash #8341f6922e4be6b6
"#;

const CHILD_KILL_CHILD_ROOT_REFILL: &str = r#"
script #9f89395491930d25 Config { scale: Small, backend_version: V1, root_version: V1, cache: false, mode: Executive } seed 0x0 top 0 extra 2 children 1 blocks 1
0 ChildKill(1, None) -> AllRemoved(1) | proof 235
1 ChildRoot(1) -> 0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314 | proof 235
2 ChildSet(1, Raw([2]), Val(Large, 4)) -> () | proof 235
3 ClientCommit(Init) -> () | proof 235
4 FinalRoot -> 0x3d686f414abd85307ae476999abc47d06b55c34ee527b80b7bd8c03ee8a85e61 | proof 271
5 ClientCommit(Finalize) -> () | proof 271
block 0 root 0x3d686f414abd85307ae476999abc47d06b55c34ee527b80b7bd8c03ee8a85e61 proof nodes 6 size 272 hash #1c8f768cac33ba2d changes main 1 child 1 hash #7d9c1c012a277c66 transaction 10 hash #684b7bb0912a505a
"#;

const FUZZ_CHILD_EMPTIED_ROOT_COMPUTED_THEN_REFILLED: &str = r#"
script #b0154aa64a9b096f Config { scale: Small, backend_version: V1, root_version: V0, cache: false, mode: Executive } seed 0xb561d5db5ec05a7c top 0 extra 1 children 1 blocks 1
0 ClientCommit(Init) -> () | proof 0
1 ChildKill(1, None) -> AllRemoved(1) | proof 217
2 ChildRoot(1) -> 0x03170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c111314 | proof 217
3 ChildSet(1, Map(45, 219, 41671), Val(Empty, 191)) -> () | proof 217
4 FinalRoot -> 0xcdcf70d8116fe42169036e97e39db4c2dcc8e5022521724a601de0994c018724 | proof 304
5 ClientCommit(Finalize) -> () | proof 304
block 0 root 0xcdcf70d8116fe42169036e97e39db4c2dcc8e5022521724a601de0994c018724 proof nodes 4 size 305 hash #3763e5c7d43a826e changes main 1 child 1 hash #33a88c6c09557a97 transaction 6 hash #6d8192407e13ddaa
"#;

const FUZZ_DROPPED_EXTRINSIC_DISCARDS_EMPTY_CHILD_CHANGES: &str = r#"
script #7ce44e3a6a2c3a5e Config { scale: Small, backend_version: V0, root_version: V1, cache: false, mode: Executive } seed 0x84544fabfbca6214 top 0 extra 0 children 1 blocks 1
0 ChildKill(1, Some(10624)) -> SomeRemaining(0) | proof 125
1 ClientCommit(Init) -> () | proof 125
2 BodyStart -> () | proof 125
3 BodyCommit -> () | proof 125
4 ClientRollback(Xt(0)) -> () | proof 125
5 ChildRoot(1) -> 0xbcd030d480b71fb7cacb0bd450eaf804bb1b5cfe7ec9d01a50a40371799c445c | proof 125
6 FinalRoot -> 0x30e83069d058c36dc539420b56d9162fe6bb37c9a47b5a571d430b26a072fc78 | proof 125
7 ClientCommit(Finalize) -> () | proof 125
block 0 root 0x30e83069d058c36dc539420b56d9162fe6bb37c9a47b5a571d430b26a072fc78 proof nodes 2 size 126 hash #0216dcb5eba82747 changes main 0 child 0 hash #9ee6dfb61a2fb903 transaction 0 hash #03170a2e7597b7b7
"#;

const FUZZ_DROPPED_EXTRINSIC_CLEARS_ROOT_CACHE: &str = r#"
script #7b98c76fc10ec367 Config { scale: Small, backend_version: V0, root_version: V1, cache: false, mode: Executive } seed 0xe663296f888a94ca top 0 extra 1 children 0 blocks 2
0 ClientCommit(Init) -> () | proof 0
1 ChildSet(2, Sibling(167, 202, 475, 15, 109), Val(Small, 49)) -> () | proof 0
2 FinalRoot -> 0x0f0ea0d9285ba19bd1a0400c5ec5d6ba047e7bb5a82aa8c13ef05d67a015cbbf | proof 107
3 ClientCommit(Finalize) -> () | proof 107
block 0 root 0x0f0ea0d9285ba19bd1a0400c5ec5d6ba047e7bb5a82aa8c13ef05d67a015cbbf proof nodes 2 size 108 hash #e12d1a2647c3fc06 changes main 0 child 1 hash #afcd224ccd8c962d transaction 5 hash #8b4190987e5de564
4 ChildClear(2, Short(0, 0)) -> () | proof 0
5 Root -> 0x0f0ea0d9285ba19bd1a0400c5ec5d6ba047e7bb5a82aa8c13ef05d67a015cbbf | proof 204
6 ClientCommit(Init) -> () | proof 204
7 BodyStart -> () | proof 204
8 BodyCommit -> () | proof 204
9 ClientRollback(Xt(0)) -> () | proof 204
10 ChildRoot(2) -> 0x1b95d40c3861b51b4c2039f867aa9ac2f21c5d4cfae4e837bf6fb8d8db5a6dd1 | proof 204
11 BodyStart -> () | proof 204
12 BodyCommit -> () | proof 204
13 ClientCommit(Xt(1)) -> () | proof 204
14 FinalRoot -> 0x0f0ea0d9285ba19bd1a0400c5ec5d6ba047e7bb5a82aa8c13ef05d67a015cbbf | proof 204
15 ClientCommit(Finalize) -> () | proof 204
block 1 root 0x0f0ea0d9285ba19bd1a0400c5ec5d6ba047e7bb5a82aa8c13ef05d67a015cbbf proof nodes 3 size 205 hash #732f27d73c70e71d changes main 1 child 1 hash #c47fb00334f0485f transaction 0 hash #03170a2e7597b7b7
"#;
