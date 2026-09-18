// This file is part of Cumulus.

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

#![cfg(feature = "jam")]

use anyhow::Context;
pub use cumulus_jam_zombienet_tests::collators::Para;
use cumulus_jam_zombienet_tests::{
	env::binaries_or_err,
	genesis_build::{build_jam_genesis, polkavm_env, JamGenesis},
	harness::TINY_CORES,
	network::PARACHAIN_SERVICE_ID,
};
use std::{
	path::{Path, PathBuf},
	sync::atomic::{AtomicU64, Ordering},
	time::{SystemTime, UNIX_EPOCH},
};
use zombienet_configuration::{para_states, ParachainConfigBuilder};
use zombienet_sdk::{Arg, Buildable, NetworkConfigBuilder, RegistrationStrategy};

/// The validators each JAM core carries; a network with `cores` cores has `cores * 3`.
const VALIDATORS_PER_CORE: usize = 3;
const ORDINARY_NODE: &str = "jam-or";

/// Everything a test needs to root a zombienet network in a JAM chain and start a para on it.
pub struct JamSetup {
	/// The genesis the JAM chain is generated from: its overrides are handed to zombienet, and
	/// the authorizer blob is what every collator names.
	genesis: JamGenesis,
	paras: Vec<Para>,
	/// The number of cores the JAM chain was generated with, which fixes its validator count.
	cores: u16,
	/// Copies of the para specs, in para order. `spawn_parachains` writes bootnodes into the spec
	/// it is handed, so the file a para's genesis head was derived from must never be that file.
	para_specs: Vec<PathBuf>,
	/// zombienet's base dir, a `zombienet/` subdir of the run's work dir.
	base_dir: PathBuf,
	jam_node: String,
	genspec_node: Option<String>,
	omni_node: String,
	authorizer_blob: String,
	/// Holds the work dir alive for the run when `JAM_TEST_BASE_DIR` is not set.
	_temp: Option<tempfile::TempDir>,
}

/// Resolve the JAM artifacts, build the genesis for `paras` on a tiny two-core network, and lay
/// out the run's work dir.
pub fn setup(test_name: &str, paras: &[Para]) -> anyhow::Result<JamSetup> {
	setup_with_cores(test_name, paras, TINY_CORES)
}

/// `setup` for a network with `cores` cores: the chain carries `cores * 3` validators and its
/// protocol parameters are widened to match, so a para may be placed on any core below `cores`.
///
/// Missing artifacts are a hard error: a test that silently passes because a blob was absent is
/// exactly what this path exists to prevent.
pub fn setup_with_cores(test_name: &str, paras: &[Para], cores: u16) -> anyhow::Result<JamSetup> {
	let binaries = binaries_or_err().context("resolving the JAM test artifacts")?;
	let (work_dir, temp) = work_dir(test_name)?;
	let genesis = build_jam_genesis(&binaries, &work_dir, paras, cores)
		.context("building the JAM genesis")?;

	let base_dir = work_dir.join("zombienet");
	std::fs::create_dir_all(&base_dir)
		.with_context(|| format!("creating {}", base_dir.display()))?;

	let para_specs = genesis
		.para_specs
		.iter()
		.enumerate()
		.map(|(index, original)| {
			let copy =
				work_dir.join(format!("jam-parachain-{}-spec.zombienet.json", paras[index].id));
			std::fs::copy(original, &copy)
				.with_context(|| format!("copying {} to {}", original.display(), copy.display()))?;
			Ok(copy)
		})
		.collect::<anyhow::Result<Vec<_>>>()?;

	let jam_node = path_str(&binaries.jam_node)?;
	let genspec_node = binaries.genspec_node.as_deref().map(path_str).transpose()?;
	let omni_node = path_str(&binaries.omni_node)?;
	let authorizer_blob = path_str(&genesis.authorizer_blob)?;

	Ok(JamSetup {
		genesis,
		paras: paras.to_vec(),
		cores,
		para_specs,
		base_dir,
		jam_node,
		genspec_node,
		omni_node,
		authorizer_blob,
		_temp: temp,
	})
}

/// One para of a run: the id it collates under, the core its work packages are authorized on, and
/// the node names that collate for it.
pub fn para(id: u32, core: u32, collator_names: &[&str]) -> Para {
	para_on_cores(id, core, &[], collator_names)
}

/// One para of a run whose authorizer is queued on `core` and every core in `also_cores` besides.
pub fn para_on_cores(id: u32, core: u32, also_cores: &[u32], collator_names: &[&str]) -> Para {
	Para {
		id,
		core,
		also_cores: also_cores.to_vec(),
		collators: collator_names.iter().map(|name| name.to_string()).collect(),
	}
}

impl JamSetup {
	/// A fresh network builder with the JAM chain this setup describes already configured.
	pub fn jamchain(&self) -> NetworkConfigBuilder<Buildable> {
		let jam_node = self.jam_node.as_str();
		let genspec_node = self.genspec_node.as_deref();
		let overrides = self.genesis.overrides.clone();
		let validators = self.cores as usize * VALIDATORS_PER_CORE;

		NetworkConfigBuilder::new().with_jamchain(|jam| {
			// The id must be `jam`: zombienet rebuilds each para's chain spec anchored to this
			// chain and overwrites `relay_chain` with this id, and the omni-node refuses JAM mode
			// unless the spec it runs says `relay_chain: "jam"`.
			let jam = jam.with_id("jam").with_default_command(jam_node);
			// Only a different build runs `gen-spec`: the node build does not always understand
			// the genesis keys.
			let jam = match genspec_node {
				Some(command) if command != jam_node => jam.with_chain_spec_command(command),
				_ => jam,
			};
			let jam = jam.with_genesis_overrides(overrides);
			let jam = jam.with_validator(|node| node.with_name("jam0").with_env(polkavm_env()));
			let jam = (1..validators).fold(jam, |jam, index| {
				jam.with_validator(|node| {
					node.with_name(&format!("jam{index}")).with_env(polkavm_env())
				})
			});
			jam.with_ordinary(|node| node.with_name(ORDINARY_NODE).with_env(polkavm_env()))
		})
	}

	/// Configure the para `self.paras[para_index]` describes.
	pub fn parachain(
		&self,
		builder: ParachainConfigBuilder<para_states::Initial, para_states::Bootstrap>,
		para_index: usize,
	) -> ParachainConfigBuilder<para_states::WithAtLeastOneCollator, para_states::Bootstrap> {
		let para = &self.paras[para_index];
		let builder = builder
			.with_id(para.id)
			// zombienet cannot auto-register a para on a JAM chain: without this it only logs a
			// warning and the para is never registered.
			.with_registration_strategy(RegistrationStrategy::Manual)
			.with_chain_spec_path(self.para_specs[para_index].clone())
			.with_default_command(self.omni_node.as_str());

		// zombienet derives a collator's key from its node name, and the authorizer hash commits to
		// the keys genesis wrote into the spec. The spec was patched with the same derivation, so
		// any name works as long as both paths agree on it.
		let builder = builder.with_collator(|node| {
			node.with_name(para.collators[0].as_str())
				.with_env(polkavm_env())
				.with_args(self.collator_args())
		});

		para.collators[1..].iter().fold(builder, |builder, name| {
			builder.with_collator(|node| {
				node.with_name(name.as_str())
					.with_env(polkavm_env())
					.with_args(self.collator_args())
			})
		})
	}

	/// The run's zombienet base dir, as the global settings take it.
	pub fn base_dir(&self) -> String {
		self.base_dir.to_string_lossy().into_owned()
	}

	/// The arguments every collator is started with: where the JAM node's RPC is, which service
	/// hosts the authorizer, and the blob whose hash the para's core was assigned from.
	fn collator_args(&self) -> Vec<Arg> {
		vec![
			"--force-authoring".into(),
			Arg::Option("--jam-rpc-urls".into(), "ws://{{ZOMBIE:jam-or:rpc_uri}}".into()),
			Arg::Option("--jam-service-id".into(), PARACHAIN_SERVICE_ID.to_string()),
			Arg::Option("--jam-authorizer-blob".into(), self.authorizer_blob.clone()),
			"--no-mdns".into(),
			"-ljam-collator=debug,jam-rpc-interface=debug".into(),
		]
	}
}

/// The run's work dir: `JAM_TEST_BASE_DIR` when set, so the logs survive, a temp dir otherwise.
fn work_dir(test_name: &str) -> anyhow::Result<(PathBuf, Option<tempfile::TempDir>)> {
	let name = run_name(test_name);
	match std::env::var_os("JAM_TEST_BASE_DIR") {
		Some(base) => {
			let dir = PathBuf::from(base).join(&name);
			std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
			Ok((dir, None))
		},
		None => {
			let temp = tempfile::Builder::new()
				.prefix(&format!("{name}."))
				.tempdir()
				.context("creating the run's temporary work dir")?;
			let dir = temp.path().to_path_buf();
			Ok((dir, Some(temp)))
		},
	}
}

/// A run dir named after the test, with a stamp and a counter so parallel runs do not collide.
fn run_name(test_name: &str) -> String {
	static NEXT_RUN: AtomicU64 = AtomicU64::new(0);
	let stamp = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|elapsed| elapsed.as_secs())
		.unwrap_or_default();
	let seq = NEXT_RUN.fetch_add(1, Ordering::Relaxed);
	format!("jam-collator-test-{test_name}-{stamp}-{seq}")
}

fn path_str(path: &Path) -> anyhow::Result<String> {
	path.to_str()
		.map(str::to_string)
		.with_context(|| format!("{} is not utf-8", path.display()))
}
