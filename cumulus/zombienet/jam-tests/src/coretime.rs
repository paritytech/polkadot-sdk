// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Starting the Coretime chain's broker for a test.
//!
//! On JAM the Coretime *parachain* runs `pallet-jam-broker`, which sends every core assignment to
//! the Parachain Service as an `AssignCore` upward message (§7.1). [`start_sales`] gets it going;
//! cores are then assigned through `cumulus_zombienet_sdk_helpers::network::assign_cores`.
//!
//! The authorizer hashes the broker is given are the ones the harness computed for the paras'
//! genesis ([`crate::genesis::authorizer_hash`]). A queue holding any other hash authorizes no
//! work package.

use crate::{genesis, para::Para};
pub use cumulus_zombienet_sdk_helpers::coretime::{CORETIME_COLLATOR, CORETIME_PARA_ID};
use cumulus_zombienet_sdk_helpers::{
	coretime::{batch_all, broker_call, full_core, sudo_call},
	submit_extrinsic_and_wait_for_best_block_success, ParaConfig,
};
use std::path::Path;
use zombienet_sdk::{
	subxt::{
		ext::scale_value::{value, Value},
		OnlineClient,
	},
	subxt_signer::sr25519::dev,
	LocalFileSystem, Network,
};

/// The blob the Coretime chain validates with, from the environment.
///
/// It is not part of the harness's default build: the test that needs it builds
/// `coretime-westend-runtime` for riscv with `--cfg jam` and points this at the resulting
/// `.polkavm`.
pub fn coretime_runtime_from_env() -> anyhow::Result<std::path::PathBuf> {
	let path = std::env::var("CORETIME_RUNTIME_WASM").map_err(|_| {
		anyhow::anyhow!("CORETIME_RUNTIME_WASM must point at the Coretime chain's PolkaVM blob")
	})?;
	let path = std::path::PathBuf::from(path);
	anyhow::ensure!(
		path.exists(),
		"the Coretime chain's runtime blob {} does not exist",
		path.display()
	);
	Ok(path)
}

/// The Coretime chain's para description: para id 1, one collator, its own runtime blob.
pub fn coretime_para() -> anyhow::Result<Para> {
	Ok(Para::new(CORETIME_PARA_ID, 0, &[CORETIME_COLLATOR])
		.with_runtime(coretime_runtime_from_env()?))
}

/// Register the authorizer hashes of `paras`, configure the broker, reserve a core for the
/// Coretime chain and start the sales over `cores` cores.
pub async fn start_sales(
	network: &Network<LocalFileSystem>,
	authorizer_blob: &Path,
	paras: &[&Para],
	cores: u16,
) -> Result<(), anyhow::Error> {
	let client: OnlineClient<ParaConfig> =
		network.get_node(CORETIME_COLLATOR)?.wait_client().await?;
	let mut calls = Vec::new();
	for para in paras {
		let hash = genesis::authorizer_hash(para, authorizer_blob)?;
		calls.push(broker_call(
			"register_authorizer",
			[
				("task", Value::u128(para.id.into())),
				("authorizer", Value::unnamed_variant("Some", [Value::from_bytes(hash.0)])),
			],
		));
	}
	calls.push(broker_call(
		"configure",
		[(
			"config",
			value! {
				{
					advance_notice: 5u32,
					interlude_length: 1u32,
					leadin_length: 1u32,
					// No sale rotates while a test runs.
					region_length: 1000u32,
					ideal_bulk_proportion: 100u32,
					limit_cores_offered: None(),
					renewal_bump: 10u32,
					contribution_timeout: 5u32
				}
			},
		)],
	));
	calls.push(broker_call("reserve", [("workload", full_core(CORETIME_PARA_ID))]));
	calls.push(broker_call(
		"start_sales",
		[
			("end_price", Value::u128(1)),
			// The core reserved for the Coretime chain counts on top of these.
			("extra_cores", Value::u128(cores.saturating_sub(1).into())),
		],
	));

	log::info!("Starting the Coretime chain's sales over {cores} cores");
	submit_extrinsic_and_wait_for_best_block_success(
		&client,
		&sudo_call(batch_all(calls)),
		&dev::alice(),
	)
	.await?;
	Ok(())
}
