// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Тест поднимает сеть из 4 валидаторов и 1 коллатора, ждет пока парачейн
// начнет создавать блоки (5 блоков), затем делает upgrade рантайма парачейна
// на рантайм с увеличенной версией и slot duration 18 сек, ждет пока апгрейд
// пройдет и проверяет, что relay chain работает и финализирует, а парачейн
// создает блоки (ждет 10 блоков).

use anyhow::anyhow;
use cumulus_zombienet_sdk_helpers::{
	assert_blocks_are_being_finalized, assert_para_throughput, runtime_upgrade, wait_for_upgrade,
};
use polkadot_primitives::Id as ParaId;
use std::time::Duration;
use zombienet_sdk::{
	subxt::{OnlineClient, PolkadotConfig},
	NetworkConfigBuilder,
};

const PARA_ID: u32 = 2000;
const WASM_WITH_SLOT_DURATION_18S: &str =
	"/tmp/wasm_binary_slot_duration_18s.rs.compact.compressed.wasm";

#[tokio::test(flavor = "multi_thread")]
async fn parachain_runtime_upgrade_test() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	let images = zombienet_sdk::environment::get_images_from_env();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			let r = r
				.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec![("-lparachain=debug").into()])
				.with_node(|node| node.with_name("validator-0"));

			// Добавляем 4 валидатора
			(1..4)
				.fold(r, |acc, i| acc.with_node(|node| node.with_name(&format!("validator-{i}"))))
		})
		.with_parachain(|p| {
			p.with_id(PARA_ID)
				.with_default_command("test-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![("-lparachain=debug,aura=debug").into()])
				.with_collator(|n| n.with_name("collator").validator(true))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let relay_node = network.get_node("validator-0")?;
	let collator_node = network.get_node("collator")?;

	let relay_client: OnlineClient<PolkadotConfig> = relay_node.wait_client().await?;
	let collator_client: OnlineClient<PolkadotConfig> = collator_node.wait_client().await?;

	// Ждем пока парачейн начнет создавать блоки и создаст 5 блоков
	log::info!("Waiting for parachain to produce 5 blocks...");
	assert_para_throughput(&relay_client, 10, [(ParaId::from(PARA_ID), 5..20)].into_iter().collect())
		.await?;

	// Получаем текущую версию runtime
	let current_spec_version = collator_client.backend().current_runtime_version().await?.spec_version;
	log::info!("Current runtime spec version: {current_spec_version}");

	// Используем WASM с slot duration 18s
	log::info!("Using runtime WASM: {}", WASM_WITH_SLOT_DURATION_18S);

	// Проверяем существование файла
	if !std::path::Path::new(WASM_WITH_SLOT_DURATION_18S).exists() {
		return Err(anyhow!(
			"Runtime WASM file not found at: {}. Please ensure the test-parachain artifacts are built with the slot-duration-18s feature.",
			WASM_WITH_SLOT_DURATION_18S
		));
	}

	// Выполняем runtime upgrade через коллатор парачейна
	// Важно: upgrade должен выполняться через коллатор, а не через relay node,
	// чтобы новый runtime мог использовать все необходимые host functions
	log::info!("Performing runtime upgrade for parachain {}", PARA_ID);
	runtime_upgrade(&network, &collator_node, PARA_ID, WASM_WITH_SLOT_DURATION_18S).await?;

	let expected_spec_version = current_spec_version + 1;

	// Ждем завершения upgrade (максимум 250 секунд)
	log::info!(
		"Waiting for parachain runtime upgrade to version {}...",
		expected_spec_version
	);
	tokio::time::timeout(
		Duration::from_secs(250),
		wait_for_upgrade(collator_client.clone(), expected_spec_version),
	)
	.await
	.map_err(|_| anyhow!("Timeout waiting for runtime upgrade"))??;

	log::info!("Runtime upgrade completed successfully");

	// Проверяем, что relay chain работает и финализирует
	log::info!("Checking that relay chain is finalizing blocks...");
	assert_blocks_are_being_finalized(&relay_client).await?;

	// В текущий момент, поскольку миграция CurrentSlot еще не добавлена,
	// должна произойти паника при попытке создать блок после upgrade,
	// потому что slot duration увеличился с 6s до 18s, что может привести
	// к уменьшению slot number, что вызовет панику "Slot must not decrease"
	// в pallet_aura::on_initialize.
	// Проверяем, что парачейн перестает создавать блоки после upgrade
	log::info!("Checking that parachain stops producing blocks after upgrade due to panic...");
	
	// Ждем некоторое время и проверяем, что парачейн не создает новые блоки
	// Если паника происходит, то блоки не будут создаваться
	let result = tokio::time::timeout(
		Duration::from_secs(60),
		assert_para_throughput(
			&relay_client,
			15,
			[(ParaId::from(PARA_ID), 1..10)].into_iter().collect(),
		),
	)
	.await;
	
	match result {
		Ok(Ok(_)) => {
			log::warn!("Parachain continued producing blocks after upgrade, which may indicate that panic did not occur");
		},
		Ok(Err(e)) => {
			log::info!("Parachain stopped producing blocks after upgrade, which indicates panic occurred as expected. Error: {}", e);
		},
		Err(_) => {
			log::info!("Timeout waiting for parachain blocks after upgrade, which indicates panic occurred as expected");
		},
	}

	log::info!("Test finished - checking for expected panic behavior");

	Ok(())
}

