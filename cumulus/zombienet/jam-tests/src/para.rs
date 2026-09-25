// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The parachains a JAM run carries: what each is, which core it sits on, and the collators that
//! author it.
//!
//! This is the pure description a genesis build and the control lane both read — no process is
//! spawned here.

use crate::chain_spec;
use std::{
	path::{Path, PathBuf},
	time::Duration,
};

/// The service id the genesis creates the parachain service under. The network is freshly
/// spawned and private to one test, so a fixed id is always free — and it is the id the
/// collators are started with (`--jam-service-id`) and the one the authorizer config commits
/// to.
pub use parachain_service_core::PARACHAIN_SERVICE_ID;

/// The JAM network the tiny tests run on: two cores, six validators.
pub const TINY_CORES: u16 = 2;

/// The whole run — network spin-up and block production — has to fit in this.
///
/// A healthy JAM network produces one parachain block per 6s slot, which would make 30 blocks
/// three minutes. A zombienet-spawned one is slower and lumpier: it records the wrong port for
/// every validator in genesis, so a work package whose guarantor set has just rotated sometimes
/// cannot be reported, and each such failure drops three in-flight blocks that have to be
/// rebuilt. Measured average is ~22s per block rather than 6s. This budget is sized for that,
/// and can come back down to a few minutes once the SDK generates matching addresses.
pub const DEADLINE: Duration = Duration::from_secs(25 * 60);

/// One JAM slot: how often a healthy network accumulates a parachain head.
///
/// A test that needs to wait a fixed number of slots — long enough for a resubmitted work package
/// to be reported, not so long that a stuck one passes — writes it as `N * JAM_SLOT` rather than
/// a bare wall-clock constant, so the budget tracks the chain's own cadence.
pub const JAM_SLOT: Duration = Duration::from_secs(6);

/// One parachain of a run: the id it collates under, the core its work packages are authorized
/// on, and the node names that collate for it.
#[derive(Clone, Debug)]
pub struct Para {
	pub id: u32,
	pub core: u32,
	/// Every further core this para's authorizer is queued on, beyond [`Self::core`]. Empty for
	/// every one-core para, which is all of them but the elastic-scaling burst test.
	///
	/// An authorizer hash commits to the para id and collator set, not to a core, so the same
	/// hash can fill more than one core's queue — which is what makes a collator's turn burst
	/// one package per core. Genesis rejects two paras naming the same core, so the extra cores
	/// must be free.
	pub also_cores: Vec<u32>,
	/// Node names, in the order the AURA round-robin walks them. A collator's key is derived from
	/// its name by zombienet — see [`chain_spec::account_of`] — so a name is all the harness
	/// needs to know which key a running collator will hold.
	pub collators: Vec<String>,
	/// The PolkaVM runtime blob this para validates with *and* its collators execute. `None`
	/// means the run's default, [`Binaries::runtime_wasm`](crate::env::Binaries::runtime_wasm)
	/// from `RUNTIME_WASM` — the one blob every para shared before a para could choose its own.
	///
	/// It is threaded through both the chain spec the collators run and the service's
	/// `validation_code`, so the bytes JAM validates and the bytes the collators execute cannot
	/// disagree.
	pub runtime: Option<PathBuf>,
}

impl Para {
	/// One para of a run: `id` collated by `collators`, its authorizer queued on `core`.
	pub fn new(id: u32, core: u32, collators: &[&str]) -> Self {
		Para {
			id,
			core,
			also_cores: Vec::new(),
			collators: collators.iter().map(|name| name.to_string()).collect(),
			runtime: None,
		}
	}

	/// The single para the collator-progress tests run: para 0 on core 0, collated by the first
	/// `count` dev accounts.
	pub fn single(count: usize) -> Self {
		Para {
			id: 0,
			core: 0,
			also_cores: Vec::new(),
			collators: (0..count).map(chain_spec::dev_name).collect(),
			runtime: None,
		}
	}

	/// Validate this para with `runtime` — its JAM validation code and the blob its collators
	/// execute — instead of the run's default. `runtime` is the PolkaVM build; the chain spec and
	/// the service's `validation_code` are both built from it.
	pub fn with_runtime(mut self, runtime: impl Into<PathBuf>) -> Self {
		self.runtime = Some(runtime.into());
		self
	}

	/// The blob this para validates with: its own [`Self::runtime`] when it chose one, `default`
	/// (the run's `RUNTIME_WASM`) otherwise.
	pub fn runtime_blob<'a>(&'a self, default: &'a Path) -> &'a Path {
		self.runtime.as_deref().unwrap_or(default)
	}

	/// Queue this para's authorizer on `cores` in addition to [`Self::core`].
	///
	/// One collator's authorizer on two cores is the elastic-scaling burst: one turn then
	/// authors one package per core, chained, in the same JAM block.
	pub fn also_on(mut self, cores: impl IntoIterator<Item = u32>) -> Self {
		self.also_cores.extend(cores);
		self
	}

	/// The collator set as `parasim-tool --collators` spells it: the names in the order the
	/// runtime's `AuraApi::authorities()` returns them.
	///
	/// The core is assigned with this exact string, and a name's position in it is the collator
	/// index the authorizer hash commits to — so it has to be the runtime's order, not the order
	/// this harness happens to list its collators in. See [`chain_spec::in_authority_order`]:
	/// those two differ as soon as a para has more than one collator.
	pub fn collator_names(&self) -> anyhow::Result<String> {
		Ok(chain_spec::in_authority_order(&self.collators)?.join(","))
	}
}
