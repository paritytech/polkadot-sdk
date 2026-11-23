// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Driving the Coretime chain's broker on JAM.
//!
//! On the relay chain a test assigns a core with `sudo(Coretime::assign_core(..))` on the relay
//! node. On JAM the Coretime *parachain* runs `pallet-broker` with `pallet-jam-coretime`, which
//! sends every core assignment to the Parachain Service as an `AssignCore` upward message (§7.1),
//! queueing the authorizer hash registered with `JamCoretime::register_authorizer`. The calls here
//! drive that broker with Alice's sudo key: [`start_sales`] gets it going, and cores are then
//! assigned through [`crate::network::assign_cores`].
//!
//! The authorizer hashes the broker is given are the ones the harness computed for the paras'
//! genesis ([`crate::jam::genesis::authorizer_hash`]). A queue holding any other hash authorizes
//! no work package.

use crate::{
	find_event_and_decode_fields,
	jam::{genesis, para::Para, tx::submit_extrinsic_and_wait_for_best_block_success},
	ParaConfig, PolkadotLikeConfig,
};
use std::{path::Path, time::Duration};
use zombienet_sdk::{
	subxt::{
		ext::scale_value::{value, Value},
		tx::DynamicPayload,
		OnlineClient,
	},
	subxt_signer::sr25519::dev,
	LocalFileSystem, Network,
};

/// The Coretime chain's para id. The Parachain Service accepts an `AssignCore` upward message
/// only from this para (`CORETIME_PARA_ID` in `parachain-service-core`), so it is fixed.
pub const CORETIME_PARA_ID: u32 = 1;

/// The single collator the Coretime chain runs. One is enough: the chain only has to produce the
/// blocks its assignments ride in, and the harness's round-robin is not under test here.
pub const CORETIME_COLLATOR: &str = "coretime-collator";

/// How long a force-reserved core may take to be assigned: the broker assigns it at the next
/// timeslice, 20 slots on the `fast-runtime` Coretime chain.
const ASSIGNMENT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Force-reserve `cores` for `para_id` on the Coretime chain behind `client`, and wait until the
/// broker assigned every one of them.
pub async fn force_reserve<C: PolkadotLikeConfig>(
	client: &OnlineClient<C>,
	para_id: u32,
	cores: &[u32],
) -> Result<(), anyhow::Error> {
	let calls = cores
		.iter()
		.map(|core| {
			broker_call(
				"force_reserve",
				[("workload", full_core(para_id)), ("core", Value::u128((*core).into()))],
			)
		})
		.collect();
	// Subscribed before submitting, so the block assigning the cores cannot be missed.
	let mut blocks = client.blocks().subscribe_best().await?;

	log::info!("Force-reserving cores {cores:?} for para {para_id} on the Coretime chain");
	submit_extrinsic_and_wait_for_best_block_success(
		client,
		&sudo_call(batch_all(calls)),
		&dev::alice(),
	)
	.await?;

	let mut pending: Vec<u16> = cores.iter().map(|core| *core as u16).collect();
	tokio::time::timeout(ASSIGNMENT_TIMEOUT, async {
		while let Some(block) = blocks.next().await {
			let events = block?.events().await?;
			let assigned: Vec<u16> =
				find_event_and_decode_fields(&events, "Broker", "CoreAssigned")?;
			pending.retain(|core| !assigned.contains(core));
			if pending.is_empty() {
				log::info!("The broker assigned cores {cores:?} to para {para_id}");
				return Ok(());
			}
		}
		Err(anyhow::anyhow!("the Coretime chain's block stream ended"))
	})
	.await
	.map_err(|_| anyhow::anyhow!("cores {pending:?} were not assigned in time"))?
}

/// `Broker::<call>(fields)` as a `RuntimeCall` value.
pub fn broker_call<'a>(call: &str, fields: impl IntoIterator<Item = (&'a str, Value)>) -> Value {
	Value::unnamed_variant("Broker", [Value::named_variant(call, fields)])
}

/// A schedule giving the whole core to `task`.
pub fn full_core(task: u32) -> Value {
	Value::unnamed_composite([Value::named_composite([
		("mask", Value::unnamed_composite([Value::from_bytes([0xffu8; 10])])),
		("assignment", Value::unnamed_variant("Task", [Value::u128(task.into())])),
	])])
}

/// `Utility::batch_all(calls)`.
pub fn batch_all(calls: Vec<Value>) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic("Utility", "batch_all", vec![Value::unnamed_composite(calls)])
}

/// Wrap `inner` in `Sudo::sudo_unchecked_weight` with a nominal declared weight.
///
/// `Sudo::sudo` derives its dispatch weight from the inner call and charges it before dispatch;
/// a nominal outer weight keeps the signed extension's check from reading it at all. The real cost
/// is still accounted after dispatch.
pub fn sudo_call(inner: DynamicPayload) -> DynamicPayload {
	zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo_unchecked_weight",
		vec![inner.into_value(), value! { { ref_time: 1u64, proof_size: 1u64 } }],
	)
}

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
		calls.push(Value::unnamed_variant(
			"JamCoretime",
			[Value::named_variant(
				"register_authorizer",
				[
					("task", Value::u128(para.id.into())),
					("authorizer", Value::unnamed_variant("Some", [Value::from_bytes(hash.0)])),
				],
			)],
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
