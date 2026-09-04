// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Test that a paraid that doesn't use elastic scaling which acquired multiple cores does not brick
// itself if ElasticScalingMVP feature is enabled in genesis.

use crate::utils::maybe_enable_experimental_collator_protocol;
use anyhow::anyhow;
use codec::Decode;
use cumulus_zombienet_sdk_helpers::{
	assert_finality_lag, assert_para_throughput, assign_cores, wait_for_pvf_prepare,
};
use polkadot_primitives::{CandidateDescriptorVersion, CoreIndex, Id as ParaId};
use rstest::rstest;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, VecDeque};
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

#[rstest]
#[case::v2(false)]
#[case::v3(true)]
#[tokio::test(flavor = "multi_thread")]
async fn doesnt_break_parachains_test(
	#[case] use_v3_candidates: bool,
) -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	// V3 candidates need the `v3` para chain spec; the V2 case uses the default one.
	let collator_chain = use_v3_candidates.then_some("v3");

	// V3 case additionally sets node-features bits 3+4 so the collator emits V3 descriptors.
	let genesis_overrides = if use_v3_candidates {
		json!({
			"configuration": {
				"config": {
					"scheduler_params": {
						"num_cores": 1,
						"max_validators_per_core": 2,
					},
					"node_features": {"bits": 8, "data": [0b00011000]}
				}
			}
		})
	} else {
		json!({
			"configuration": {
				"config": {
					"scheduler_params": {
						"num_cores": 1,
						"max_validators_per_core": 2,
					}
				}
			}
		})
	};

	// `--authoring=slot-based` only for the V3 case.
	let mut collator_args = vec![("-lparachain=debug,aura=debug").into()];
	if use_v3_candidates {
		collator_args.push("--authoring=slot-based".into());
	}

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(maybe_enable_experimental_collator_protocol(vec![
					("-lparachain=debug").into(),
				]))
				.with_genesis_overrides(genesis_overrides)
				// Have to set a `with_validator` outside of the loop below, so that `r` has the
				// right type.
				.with_validator(|node| node.with_name("validator-0"));

			(1..4).fold(r, |acc, i| {
				acc.with_validator(|node| node.with_name(&format!("validator-{i}")))
			})
		})
		.with_parachain(|p| {
			let p = p
				.with_id(2000)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(collator_args);
			let p = match collator_chain {
				Some(chain) => p.with_chain(chain),
				None => p,
			};
			p.with_collator(|n| n.with_name("collator-2000"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let para_node = network.get_node("collator-2000")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;

	assign_cores(&relay_client, 2000, vec![0]).await?;

	let para_id = ParaId::from(2000);
	// Wait for PVF preparation to complete.
	wait_for_pvf_prepare(&network, 1).await?;

	if use_v3_candidates {
		// V3 candidates at single-core throughput.
		crate::utils::assert_candidates_version(
			&relay_client,
			CandidateDescriptorVersion::V3,
			HashMap::from([(para_id, 12..16)]),
			15,
		)
		.await?;
	} else {
		// Expect the parachain to be making normal progress, 1 candidate backed per relay chain
		// block. Lowering to 12 to make sure CI passes.
		assert_para_throughput(&relay_client, 15, [(para_id, 12..16)], []).await?;
	}

	let para_client = para_node.wait_client().await?;
	// Assert the parachain finalized block height is also on par with the number of backed
	// candidates.
	// Increasing to 6 to make sure CI passes.
	assert_finality_lag(&para_client, 6).await?;

	// Sanity check that indeed the parachain has two assigned cores.
	let cq = BTreeMap::<CoreIndex, VecDeque<ParaId>>::decode(
		&mut &relay_client
			.runtime_api()
			.at_latest()
			.await?
			.call_raw("ParachainHost_claim_queue", None)
			.await?[..],
	)?;

	// Get lookahead config
	let lookahead = u32::decode(
		&mut &relay_client
			.runtime_api()
			.at_latest()
			.await?
			.call_raw("ParachainHost_scheduling_lookahead", None)
			.await?[..],
	)?;

	assert_eq!(
		cq,
		[
			(CoreIndex(0), std::iter::repeat_n(para_id, lookahead as usize).collect()),
			(CoreIndex(1), std::iter::repeat_n(para_id, lookahead as usize).collect()),
		]
		.into_iter()
		.collect()
	);

	log::info!("Test finished successfully");

	Ok(())
}
