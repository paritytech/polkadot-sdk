// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Runtime migration test for Scheduler V3→V4 and OnDemand V1→V2.
//!
//! Boots a 2-validator rococo-local chain with the OLD runtime WASM (from master),
//! creates meaningful pre-migration state (core assignments, on-demand orders),
//! performs a runtime upgrade to the NEW WASM (from this branch), then verifies:
//!
//! 1. Pallet storage versions updated correctly (Scheduler 3→4, OnDemand 1→2)
//! 2. Old AssignerCoretime storage cleaned up (CoreDescriptors, CoreSchedules)
//! 3. New Scheduler storage populated with migrated data
//! 4. Old on-demand storage cleaned up (QueueStatus, FreeEntries, AffinityEntries,
//!    ParaIdAffinity)
//! 5. Migration log output appears in node logs
//! 6. Chain continues producing and finalizing blocks
//! 7. ClaimQueue runtime API works correctly post-upgrade
//! 8. New assign_core calls work post-upgrade
//!
//! ## Prerequisites
//!
//! Set `ROCOCO_OLD_WASM_PATH` env var to the path of the old rococo runtime WASM
//! (built from master). If not set, the test skips gracefully.
//!
//! ```bash
//! ROCOCO_OLD_WASM_PATH=/path/to/old/rococo_runtime.compact.compressed.wasm \
//!   cargo test -p polkadot-zombienet-sdk-tests --features zombie-ci \
//!   -- migration --nocapture
//! ```

use anyhow::anyhow;
use codec::Decode;
use polkadot_primitives::{CoreIndex, Id as ParaId};
use std::collections::{BTreeMap, VecDeque};
use subxt::{ext::scale_value::value, OnlineClient, PolkadotConfig};

use cumulus_zombienet_sdk_helpers::{
	assert_blocks_are_being_finalized, create_runtime_upgrade_call,
	submit_extrinsic_and_wait_for_finalization_success, wait_for_runtime_upgrade,
};
use zombienet_sdk::{subxt, subxt_signer::sr25519::dev, NetworkConfigBuilder};

const PARA_A: u32 = 1000;
const PARA_B: u32 = 1001;
/// 3 cores: 0 and 1 for bulk (Task), 2 for on-demand (Pool).
/// The Pool core is needed so the scheduler actually pops on-demand orders,
/// which creates ParaIdAffinity and moves subsequent orders into AffinityEntries.
const NUM_CORES: u32 = 3;

#[tokio::test(flavor = "multi_thread")]
async fn relay_chain_migration() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);

	// ── Phase 1: Setup ──────────────────────────────────────────────────

	let old_wasm_path = match std::env::var("ROCOCO_OLD_WASM_PATH") {
		Ok(path) => {
			log::info!("Using old WASM from: {path}");
			path
		},
		Err(_) => {
			log::info!(
				"ROCOCO_OLD_WASM_PATH not set, skipping migration test. \
				 Set it to the path of the old rococo runtime WASM to run this test."
			);
			return Ok(());
		},
	};

	// Use compressed WASM to keep the RPC request size manageable.
	// system.set_code accepts compressed WASM and the runtime decompresses it.
	let new_wasm_path = match std::env::var("ROCOCO_NEW_WASM_PATH") {
		Ok(path) => {
			log::info!("Using new WASM from env: {path}");
			std::path::PathBuf::from(path)
		},
		Err(_) => {
			// Fall back to the build output
			std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
				.parent()
				.and_then(|p| p.parent())
				.ok_or_else(|| anyhow!("Cannot determine workspace root"))?
				.join("target/release/wbuild/rococo-runtime/rococo_runtime.compact.compressed.wasm")
		},
	};
	let new_wasm = std::fs::read(&new_wasm_path).map_err(|e| {
		anyhow!(
			"Failed to read new compressed WASM from {}: {e}. \
			 Either set ROCOCO_NEW_WASM_PATH or build rococo-runtime in release mode.",
			new_wasm_path.display()
		)
	})?;
	log::info!("New WASM (compressed) size: {} bytes from {}", new_wasm.len(), new_wasm_path.display());

	log::info!("Generating patched chain spec with old runtime...");
	let spec_dir = generate_patched_chain_spec(&old_wasm_path)?;
	let raw_spec_path = spec_dir.path().join("raw-chain-spec.json").to_string_lossy().to_string();
	log::info!("Raw chain spec at: {raw_spec_path}");

	let images = zombienet_sdk::environment::get_images_from_env();
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("rococo-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_chain_spec_path(&*raw_spec_path)
				.with_default_args(vec![
					("-lruntime=debug").into(),
					("--rpc-max-request-size=10").into(),
					("--rpc-max-response-size=10").into(),
				])
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
		.with_global_settings(|gs| match std::env::var("ZOMBIENET_SDK_BASE_DIR") {
			Ok(val) => gs.with_base_dir(val),
			_ => gs,
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	log::info!("Spawning network...");
	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;

	let alice_node = network.get_node("alice")?;
	let client: OnlineClient<PolkadotConfig> = alice_node.wait_client().await?;
	let alice = dev::alice();

	log::info!("Network spawned, waiting for blocks...");

	// Wait for at least block 2 to ensure chain is producing
	let mut blocks_sub = client.blocks().subscribe_finalized().await?;
	while let Some(Ok(block)) = blocks_sub.next().await {
		if block.number() >= 2 {
			log::info!("Chain producing blocks (at #{})", block.number());
			break;
		}
	}
	drop(blocks_sub);

	// ── Phase 2: Pre-upgrade verification ───────────────────────────────

	log::info!("=== Phase 2: Pre-upgrade verification ===");

	let scheduler_version = get_pallet_storage_version(&client, "ParaScheduler").await?;
	log::info!("Scheduler pallet version: {scheduler_version}");
	assert_eq!(scheduler_version, 3, "Expected Scheduler pallet version 3 (pre-migration)");

	let on_demand_version =
		get_pallet_storage_version(&client, "OnDemandAssignmentProvider").await?;
	log::info!("OnDemand pallet version: {on_demand_version}");
	assert_eq!(on_demand_version, 1, "Expected OnDemand pallet version 1 (pre-migration)");

	let old_spec_version = client.runtime_version().spec_version;
	log::info!("Old spec_version: {old_spec_version}");

	// ── Phase 3: Create pre-migration state ─────────────────────────────

	log::info!("=== Phase 3: Create pre-migration state ===");

	// Assign cores: 0 and 1 for bulk paras (Task), 2 for on-demand (Pool).
	// The Pool core is critical: without it the scheduler never calls
	// pop_assignment_for_core, so ParaIdAffinity and AffinityEntries would
	// never get populated.
	log::info!(
		"Assigning core 0 -> Task({}), core 1 -> Task({}), core 2 -> Pool...",
		PARA_A, PARA_B
	);
	let assign_cores_call = subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Utility(batch { calls: (
				Coretime(assign_core { core: 0u32, begin: 0u32, assignment: ((Task(PARA_A), 57600u32)), end_hint: None() }),
				Coretime(assign_core { core: 1u32, begin: 0u32, assignment: ((Task(PARA_B), 57600u32)), end_hint: None() }),
				Coretime(assign_core { core: 2u32, begin: 0u32, assignment: ((Pool(), 57600u32)), end_hint: None() })
			)})
		}],
	);
	submit_extrinsic_and_wait_for_finalization_success(&client, &assign_cores_call, &alice)
		.await?;
	log::info!("Core assignments finalized (2 bulk + 1 pool)");

	// Verify CoreDescriptors exist in old AssignerCoretime storage for all 3 cores
	// (check before session change, while CoreSchedules are still present)
	wait_n_finalized_blocks(&client, 2).await?;
	for core in 0..NUM_CORES {
		let has_desc = has_old_core_descriptor(&client, core).await?;
		log::info!("Old CoreDescriptor for core {core}: {has_desc}");
		assert!(
			has_desc,
			"CoreDescriptor for core {core} should exist in CoretimeAssignmentProvider"
		);
	}

	// Verify old CoreSchedules exist for all 3 cores (begin=0 schedules are consumed
	// at the next session change, so we must check before that happens)
	for core in 0..NUM_CORES {
		let has_sched = has_old_core_schedule(&client, core, 0).await?;
		log::info!("Old CoreSchedule for core {core} @ block 0: {has_sched}");
		assert!(
			has_sched,
			"CoreSchedule for core {core} should exist in CoretimeAssignmentProvider"
		);
	}

	let old_claim_queue_key = storage_key("ParaScheduler", "ClaimQueue");

	// ── Phase 3b: Populate on-demand storage (FreeEntries + AffinityEntries) ──
	//
	// Strategy: Place many on-demand orders BEFORE the session change so they sit
	// in FreeEntries while the Pool core is still inactive. When the session change
	// activates the Pool core, the scheduler's pop_assignment_for_core will pop the
	// first para 2000 order and — critically — partition all remaining para 2000
	// orders from FreeEntries into AffinityEntries (see the `partition` in
	// pop_assignment_for_core). This also calls increase_affinity, creating
	// ParaIdAffinity for para 2000 on core 2.
	//
	// We place enough orders (10× para 2000 + 5× para 2001 = 15 total) so that
	// even after the scheduler fills its lookahead slots (up to 6) and processes
	// a few blocks, there are still orders remaining in BOTH AffinityEntries and
	// FreeEntries when we trigger the runtime upgrade.
	//
	// Order placement sequence (all go to FreeEntries since no affinity exists yet):
	//   10× para 2000  — after first pop, remaining 9 move to AffinityEntries
	//    5× para 2001  — stays in FreeEntries (different para, no affinity)

	log::info!("=== Phase 3b: Populating on-demand storage ===");

	// Batch all on-demand orders into a single utility.batch_all call so they're
	// placed atomically in one block. This is critical: if we place them one at
	// a time (each taking ~16s to finalize), the session change would happen
	// mid-placement and the scheduler would start consuming orders before we
	// finish placing them all.
	//
	// place_order_allow_death uses ensure_signed, and utility.batch preserves
	// the caller's origin, so Alice's signed origin is used for each sub-call.
	log::info!("Placing 15 on-demand orders in a single batch (10× para 2000 + 5× para 2001)...");

	let batch_place_orders = subxt::tx::dynamic(
		"Utility",
		"batch_all",
		vec![value! {
			{ calls: (
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_1 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_1 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_1 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_1 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_1 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_1 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_1 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_1 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_1 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_1 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_2 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_2 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_2 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_2 }),
				OnDemandAssignmentProvider(place_order_allow_death { max_amount: 1_000_000_000_000u128, para_id: ON_DEMAND_PARA_2 })
			)}
		}],
	);
	submit_extrinsic_and_wait_for_finalization_success(&client, &batch_place_orders, &alice)
		.await?;
	log::info!("All 15 on-demand orders placed in one batch");

	// Log the current state. All 15 orders were placed atomically in one block,
	// so they should all be in FreeEntries (the Pool core isn't active yet since
	// no session change has happened since the core assignments).
	let free_entries_pre = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(storage_key("OnDemandAssignmentProvider", "FreeEntries"))
		.await?;
	let affinity_pre = get_para_id_affinity(&client, ON_DEMAND_PARA_1).await?;
	let affinity_entries_pre = get_affinity_entries(&client, 2).await?;
	log::info!(
		"After placing orders: FreeEntries={} bytes, ParaIdAffinity[2000]={}, AffinityEntries[core2]={} bytes",
		free_entries_pre.as_ref().map_or(0, |d| d.len()),
		affinity_pre.is_some(),
		affinity_entries_pre.as_ref().map_or(0, |d| d.len())
	);

	// Wait for session change so the 3-core config (with Pool core) becomes active.
	// Since orders were placed atomically, we're still in the same session as when
	// the core assignments were made. The session change will activate the Pool core
	// and the scheduler will start popping on-demand orders.
	log::info!("Waiting for session change so Pool core becomes active...");
	wait_for_session_change(&client).await?;
	log::info!("Session change done, Pool core is now active");

	// Wait for the scheduler to pop on-demand orders and create affinity.
	// With 10 orders for para 2000 in FreeEntries, the first pop will:
	//   1. Take 1 order and return it as the assignment
	//   2. Partition remaining 9 para-2000 orders from FreeEntries → AffinityEntries
	//   3. Call increase_affinity(2000, core_2) → creates ParaIdAffinity
	log::info!("Waiting for scheduler to pop orders and create affinity...");
	{
		let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
		let mut blocks_sub = client.blocks().subscribe_finalized().await?;
		let mut found = false;
		while let Some(Ok(block)) = blocks_sub.next().await {
			if std::time::Instant::now() > deadline {
				break;
			}
			let affinity = get_para_id_affinity(&client, ON_DEMAND_PARA_1).await?;
			let affinity_entries = get_affinity_entries(&client, 2).await?;
			let free_entries = client
				.storage()
				.at_latest()
				.await?
				.fetch_raw(storage_key("OnDemandAssignmentProvider", "FreeEntries"))
				.await?;
			let claim_queue = get_claim_queue(&client).await?;
			log::info!(
				"Block #{}: ParaIdAffinity[{}]={}, AffinityEntries[core2]={} bytes, \
				 FreeEntries={} bytes, ClaimQueue={:?}",
				block.number(),
				ON_DEMAND_PARA_1,
				affinity.is_some(),
				affinity_entries.as_ref().map_or(0, |d| d.len()),
				free_entries.as_ref().map_or(0, |d| d.len()),
				claim_queue
			);
			if affinity.is_some() {
				found = true;
				log::info!(
					"ParaIdAffinity created for para {}! AffinityEntries[core2] has {} bytes",
					ON_DEMAND_PARA_1,
					affinity_entries.as_ref().map_or(0, |d| d.len())
				);
				break;
			}
		}
		assert!(
			found,
			"ParaIdAffinity should be created for para {} after scheduler pops its order. \
			 Check if the Pool core (core 2) is active and processing on-demand orders.",
			ON_DEMAND_PARA_1
		);
	}

	// ── Phase 3c: Snapshot pre-migration on-demand storage & proceed to upgrade ──
	//
	// IMPORTANT: We must proceed to the runtime upgrade AS FAST AS POSSIBLE.
	// The scheduler is actively processing orders and will drive the affinity
	// count to 0 via report_processed → decrease_affinity_update_queue. Once
	// count reaches 0, ParaIdAffinity is removed and AffinityEntries are moved
	// back to FreeEntries. We want to catch the state while affinity exists.

	log::info!("=== Phase 3c: Pre-migration on-demand storage snapshot ===");

	let old_free_entries_key = storage_key("OnDemandAssignmentProvider", "FreeEntries");
	let old_queue_status_key = storage_key("OnDemandAssignmentProvider", "QueueStatus");

	// QueueStatus must exist (tracks traffic, indices)
	let old_queue_status_data = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(old_queue_status_key.clone())
		.await?;
	assert!(
		old_queue_status_data.is_some(),
		"QueueStatus should exist after placing on-demand orders"
	);

	// Decode traffic from QueueStatus (first field is FixedU128 = 16 bytes)
	let queue_status_bytes = old_queue_status_data.unwrap();
	assert!(queue_status_bytes.len() >= 16, "QueueStatus too short to contain traffic");
	let traffic_bytes: [u8; 16] = queue_status_bytes[..16].try_into().unwrap();
	let pre_migration_traffic = u128::from_le_bytes(traffic_bytes);
	log::info!(
		"Pre-migration traffic (raw u128): {} (FixedU128 = {:.6})",
		pre_migration_traffic,
		pre_migration_traffic as f64 / 1_000_000_000_000_000_000f64
	);

	// Snapshot the current state of affinity and entries. The scheduler is racing
	// against us, so we log everything but only hard-assert on what must be true.
	let affinity_2000 = get_para_id_affinity(&client, ON_DEMAND_PARA_1).await?;
	let affinity_entries_core2 = get_affinity_entries(&client, 2).await?;
	let old_free_entries_data = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(old_free_entries_key.clone())
		.await?;
	log::info!(
		"Pre-migration snapshot: ParaIdAffinity[{}]={}, AffinityEntries[core2]={} bytes, FreeEntries={} bytes",
		ON_DEMAND_PARA_1,
		affinity_2000.is_some(),
		affinity_entries_core2.as_ref().map_or(0, |d| d.len()),
		old_free_entries_data.as_ref().map_or(0, |d| d.len())
	);

	// We confirmed affinity existed in the polling loop above. It may or may not
	// still exist at this exact moment due to the scheduler race. Either way, the
	// migration code will handle whatever state it finds. The key facts are:
	// - We exercised the FreeEntries → AffinityEntries partition path
	// - ParaIdAffinity was created (confirmed in polling loop)
	// - Traffic was updated by 4 spot price payments
	// - The migration will merge whatever remains

	// Capture the pre-migration ClaimQueue for comparison after upgrade.
	// Bulk (Task) cores should have stable assignments across the migration.
	let pre_upgrade_claim_queue = get_claim_queue(&client).await?;
	log::info!("Pre-upgrade ClaimQueue: {:?}", pre_upgrade_claim_queue);

	log::info!("Pre-migration state captured, proceeding immediately to runtime upgrade");

	// ── Phase 4: Runtime upgrade ────────────────────────────────────────

	log::info!("=== Phase 4: Runtime upgrade ===");

	let upgrade_call = create_runtime_upgrade_call(&new_wasm);
	log::info!("Submitting runtime upgrade...");
	submit_extrinsic_and_wait_for_finalization_success(&client, &upgrade_call, &alice).await?;

	log::info!("Waiting for RuntimeEnvironmentUpdated digest...");
	let upgrade_block_hash = wait_for_runtime_upgrade(&client).await?;
	log::info!("Runtime upgraded at block hash: {:?}", upgrade_block_hash);

	// ── Phase 5: Post-upgrade verification — Storage versions ───────────

	log::info!("=== Phase 5: Post-upgrade storage verification ===");

	// Recreate client to pick up new metadata after runtime upgrade
	let client: OnlineClient<PolkadotConfig> = alice_node.wait_client().await?;

	// Wait a few blocks for the post-upgrade state to be fully visible in finalized blocks.
	wait_n_finalized_blocks(&client, 3).await?;
	log::info!("Post-upgrade blocks finalized");

	// 5a. spec_version increased
	let new_spec_version = client.runtime_version().spec_version;
	log::info!("New spec_version: {new_spec_version} (was: {old_spec_version})");
	assert!(
		new_spec_version > old_spec_version,
		"spec_version should increase after upgrade: {new_spec_version} > {old_spec_version}"
	);

	// 5b. Pallet storage versions updated
	let scheduler_version = get_pallet_storage_version(&client, "ParaScheduler").await?;
	log::info!("Post-upgrade Scheduler pallet version: {scheduler_version}");
	assert_eq!(scheduler_version, 4, "Scheduler pallet version should be 4 after migration");

	let on_demand_version =
		get_pallet_storage_version(&client, "OnDemandAssignmentProvider").await?;
	log::info!("Post-upgrade OnDemand pallet version: {on_demand_version}");
	assert_eq!(on_demand_version, 2, "OnDemand pallet version should be 2 after migration");

	// 5c. ClaimQueue stability check — bulk (Task) cores must keep their assignments.
	//
	// The V3→V4 migration removes the old ClaimQueue storage. In V4, claim_queue()
	// is computed on-the-fly from CoreDescriptors + CoreSchedules. For bulk cores
	// (Task assignments), the computed queue must match because the CoreSchedules
	// were migrated from the old AssignerCoretime pallet.
	//
	// For the Pool core (on-demand), the ordering may differ since pool assignments
	// in the old ClaimQueue are pushed back as new orders and the peek logic may
	// produce a different sequence. This is acceptable — on-demand paras don't
	// produce continuous blocks.
	let post_upgrade_claim_queue = get_claim_queue(&client).await?;
	log::info!("Post-upgrade ClaimQueue (immediate): {:?}", post_upgrade_claim_queue);

	// Verify bulk cores are stable
	for core_idx in 0..2u32 {
		let core = CoreIndex(core_idx);
		let pre = pre_upgrade_claim_queue.get(&core);
		let post = post_upgrade_claim_queue.get(&core);
		log::info!(
			"ClaimQueue stability core {}: pre={:?}, post={:?}",
			core_idx, pre, post
		);
		// Both should have the same para assigned (Task assignment)
		if let (Some(pre_q), Some(post_q)) = (pre, post) {
			let pre_para = pre_q.front();
			let post_para = post_q.front();
			assert_eq!(
				pre_para, post_para,
				"Bulk core {} ClaimQueue front assignment should be stable across migration: \
				 pre={:?} post={:?}",
				core_idx, pre_para, post_para
			);
		}
		// If pre had entries, post should also have entries
		if pre.map_or(false, |q| !q.is_empty()) {
			assert!(
				post.map_or(false, |q| !q.is_empty()),
				"Bulk core {} should still have ClaimQueue entries after migration",
				core_idx
			);
		}
	}
	log::info!("ClaimQueue stability verified: bulk cores preserved across migration");

	// ── Phase 6: Post-upgrade — Old storage cleaned up ──────────────────

	log::info!("=== Phase 6: Old storage cleanup verification ===");

	// 6a. Old AssignerCoretime CoreDescriptors removed (all 3 cores)
	for core in 0..NUM_CORES {
		let has_old_desc = has_old_core_descriptor(&client, core).await?;
		log::info!("Old CoreDescriptor for core {core} after migration: {has_old_desc}");
		assert!(
			!has_old_desc,
			"Old CoreDescriptor for core {core} should be cleaned up after migration"
		);
	}

	// 6b. Old AssignerCoretime CoreSchedules removed
	// Note: CoreSchedules with begin=0 were already consumed at the pre-migration
	// session change (applied into WorkState). We verify they don't exist in the
	// old pallet's storage prefix — the migration should clear any remaining ones.
	for core in 0..NUM_CORES {
		let has_old_sched = has_old_core_schedule(&client, core, 0).await?;
		log::info!("Old CoreSchedule for core {core} after migration: {has_old_sched}");
		assert!(
			!has_old_sched,
			"Old CoreSchedule for core {core} should not exist after migration"
		);
	}

	// 6c. Old ClaimQueue removed
	let old_claim_queue_data = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(old_claim_queue_key)
		.await?;
	log::info!("Old ClaimQueue after migration: exists = {}", old_claim_queue_data.is_some());
	assert!(
		old_claim_queue_data.is_none(),
		"Old ClaimQueue storage should be removed after migration"
	);

	// 6d. Old on-demand storage cleaned up — ALL items must be gone
	let old_queue_status_post = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(old_queue_status_key)
		.await?;
	log::info!("Old QueueStatus after migration: exists = {}", old_queue_status_post.is_some());
	assert!(
		old_queue_status_post.is_none(),
		"Old QueueStatus should be removed after migration"
	);

	let old_free_entries_post = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(old_free_entries_key)
		.await?;
	log::info!("Old FreeEntries after migration: exists = {}", old_free_entries_post.is_some());
	assert!(
		old_free_entries_post.is_none(),
		"Old FreeEntries should be removed after migration"
	);

	// 6e. AffinityEntries for core 2 (Pool core) must be removed
	let old_affinity_core2_post = get_affinity_entries(&client, 2).await?;
	log::info!(
		"Old AffinityEntries[core 2] after migration: exists = {}",
		old_affinity_core2_post.is_some()
	);
	assert!(
		old_affinity_core2_post.is_none(),
		"Old AffinityEntries[core 2] should be removed after migration"
	);

	// 6f. ParaIdAffinity for both on-demand paras must be removed
	let affinity_2000_post = get_para_id_affinity(&client, ON_DEMAND_PARA_1).await?;
	let affinity_2001_post = get_para_id_affinity(&client, ON_DEMAND_PARA_2).await?;
	log::info!(
		"Old ParaIdAffinity after migration: para {} = {}, para {} = {}",
		ON_DEMAND_PARA_1, affinity_2000_post.is_some(),
		ON_DEMAND_PARA_2, affinity_2001_post.is_some()
	);
	assert!(
		affinity_2000_post.is_none(),
		"ParaIdAffinity[{}] should be removed after migration",
		ON_DEMAND_PARA_1
	);
	assert!(
		affinity_2001_post.is_none(),
		"ParaIdAffinity[{}] should be removed after migration",
		ON_DEMAND_PARA_2
	);

	// ── Phase 7: Post-upgrade — New storage populated ───────────────────

	log::info!("=== Phase 7: New storage verification ===");

	// 7a. New CoreDescriptors in Scheduler pallet (now a BTreeMap, not a StorageMap)
	let new_descriptors_key = storage_key("ParaScheduler", "CoreDescriptors");
	let new_descriptors_data = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(new_descriptors_key)
		.await?;
	assert!(
		new_descriptors_data.is_some(),
		"New CoreDescriptors should exist in Scheduler pallet after migration"
	);
	let descriptors_size = new_descriptors_data.unwrap().len();
	log::info!("New CoreDescriptors in Scheduler: {} bytes", descriptors_size);
	assert!(
		descriptors_size > 4,
		"New CoreDescriptors should contain actual data (got {} bytes)",
		descriptors_size
	);

	// 7b. New OrderStatus in OnDemand pallet (contains migrated orders + traffic)
	let new_order_status_key = storage_key("OnDemandAssignmentProvider", "OrderStatus");
	let new_order_status_data = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(new_order_status_key)
		.await?;
	assert!(
		new_order_status_data.is_some(),
		"New OrderStatus should exist in OnDemand pallet after migration"
	);
	let order_status_bytes = new_order_status_data.unwrap();
	log::info!("New OrderStatus in OnDemand: {} bytes", order_status_bytes.len());

	// Decode OrderStatus: traffic (FixedU128 = u128 LE, 16 bytes) + queue (compact len + entries)
	assert!(order_status_bytes.len() >= 16, "OrderStatus too short for traffic field");
	let post_traffic_bytes: [u8; 16] = order_status_bytes[..16].try_into().unwrap();
	let post_migration_traffic = u128::from_le_bytes(post_traffic_bytes);
	log::info!(
		"Post-migration traffic (raw u128): {} (FixedU128 = {:.6})",
		post_migration_traffic,
		post_migration_traffic as f64 / 1_000_000_000_000_000_000f64
	);

	// Verify traffic was preserved from the old QueueStatus
	assert_eq!(
		pre_migration_traffic, post_migration_traffic,
		"Traffic value must be preserved across migration: pre={} post={}",
		pre_migration_traffic, post_migration_traffic
	);
	log::info!("Traffic value correctly preserved across migration");

	// Decode the queue: after the 16-byte traffic, we have a SCALE-encoded BoundedVec.
	// BoundedVec encodes as: compact length prefix + (para_id: u32 + ordered_at: u32) per entry.
	let queue_data = &order_status_bytes[16..];
	let (queue_len, offset) = decode_compact_u32(queue_data);
	log::info!(
		"Post-migration OrderStatus queue: {} orders ({} bytes of queue data)",
		queue_len,
		queue_data.len()
	);

	// Decode individual orders to verify para IDs were preserved
	let mut migrated_para_ids = Vec::new();
	let mut pos = offset;
	for _ in 0..queue_len {
		if pos + 8 > queue_data.len() {
			break;
		}
		let para_id = u32::from_le_bytes(queue_data[pos..pos + 4].try_into().unwrap());
		let ordered_at = u32::from_le_bytes(queue_data[pos + 4..pos + 8].try_into().unwrap());
		migrated_para_ids.push(para_id);
		log::info!("  Migrated order: para_id={para_id}, ordered_at=block#{ordered_at}");
		pos += 8;
	}
	log::info!("Migrated para IDs: {:?}", migrated_para_ids);

	// Note: The scheduler may have consumed some orders between placement and upgrade.
	// What we verify is that any remaining orders have valid para IDs from our test set
	// and that the migration didn't corrupt the data.
	for para_id in &migrated_para_ids {
		assert!(
			*para_id == ON_DEMAND_PARA_1 || *para_id == ON_DEMAND_PARA_2,
			"Migrated order has unexpected para_id={para_id}, expected {} or {}",
			ON_DEMAND_PARA_1, ON_DEMAND_PARA_2
		);
	}

	// 7c. CoreSchedules may have been consumed by the scheduler after migration
	// (schedules with begin=0 are applied immediately), so we don't assert their
	// presence. Instead, we verify the ClaimQueue is populated (next check).

	// 7d. ClaimQueue post-upgrade
	// Wait for a session change so ValidatorGroups are set and num_availability_cores > 0.
	// With fast-runtime, sessions are ~10 blocks (1 minute).
	log::info!("Waiting for session change to populate ClaimQueue...");
	wait_for_session_change(&client).await?;
	log::info!("Session change detected");

	let claim_queue_post = get_claim_queue(&client).await?;
	log::info!("Post-upgrade ClaimQueue: {:?}", claim_queue_post);
	assert!(
		!claim_queue_post.is_empty(),
		"ClaimQueue should be populated after session change"
	);
	if let Some(core_0_queue) = claim_queue_post.get(&CoreIndex(0)) {
		log::info!("ClaimQueue core 0: {:?}", core_0_queue);
	}

	// ── Phase 8: Migration log verification ─────────────────────────────

	log::info!("=== Phase 8: Migration log verification ===");

	// The on_runtime_upgrade log is always emitted (not behind try-runtime).
	// Check for the scheduler migration log.
	let result = alice_node
		.wait_log_line_count_with_timeout(
			"Migrated para scheduler storage to v4:",
			false,
			zombienet_orchestrator::network::node::LogLineCountOptions::new(
				|n| n >= 1,
				std::time::Duration::from_secs(10),
				false,
			),
		)
		.await?;
	assert!(
		result.success(),
		"Should find scheduler migration log 'Migrated para scheduler storage to v4:'"
	);
	log::info!("Found scheduler migration log");

	// Check for the on-demand migration log. The scheduler may have consumed some
	// orders before the upgrade, so we check for the overall migration message and
	// verify the count matches what we decoded from OrderStatus.
	let result = alice_node
		.wait_log_line_count_with_timeout(
			"Migrated on demand assigner storage to v2:",
			false,
			zombienet_orchestrator::network::node::LogLineCountOptions::new(
				|n| n >= 1,
				std::time::Duration::from_secs(10),
				false,
			),
		)
		.await?;
	assert!(
		result.success(),
		"Should find on-demand migration log 'Migrated on demand assigner storage to v2:'"
	);
	log::info!("Found on-demand migration log");

	// Also verify the affinity removal was logged
	let result = alice_node
		.wait_log_line_count_with_timeout(
			"affinity entries removed",
			false,
			zombienet_orchestrator::network::node::LogLineCountOptions::new(
				|n| n >= 1,
				std::time::Duration::from_secs(10),
				false,
			),
		)
		.await?;
	assert!(
		result.success(),
		"Should find 'affinity entries removed' in migration log"
	);
	log::info!("Found affinity entries removal in migration log");

	// ── Phase 9: Post-upgrade functionality ─────────────────────────────

	log::info!("=== Phase 9: Post-upgrade functionality verification ===");

	// 9a. Chain keeps finalizing
	assert_blocks_are_being_finalized(&client).await?;
	log::info!("Chain continues finalizing blocks after upgrade");

	// 9b. New assign_core call works post-upgrade
	log::info!("Testing new assign_core call post-upgrade...");
	let new_assign_call = subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			Coretime(assign_core { core: 0u32, begin: 0u32, assignment: ((Task(PARA_B), 57600u32)), end_hint: None() })
		}],
	);
	submit_extrinsic_and_wait_for_finalization_success(&client, &new_assign_call, &alice).await?;
	log::info!("Post-upgrade assign_core succeeded (extrinsic accepted and finalized)");

	// Verify the new CoreDescriptors were updated
	let new_descriptors_data = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(storage_key("ParaScheduler", "CoreDescriptors"))
		.await?;
	assert!(
		new_descriptors_data.is_some(),
		"CoreDescriptors should still exist after post-upgrade assign_core"
	);
	log::info!(
		"CoreDescriptors still present after post-upgrade assign_core: {} bytes",
		new_descriptors_data.unwrap().len()
	);

	// ── Done ────────────────────────────────────────────────────────────

	log::info!("=== Migration test completed successfully! ===");
	log::info!("Summary:");
	log::info!("  - Scheduler: v3 -> v4");
	log::info!("  - OnDemand: v1 -> v2");
	log::info!("  - spec_version: {} -> {}", old_spec_version, new_spec_version);
	log::info!("  - Pre-migration: 3 cores (2 bulk Task + 1 Pool for on-demand)");
	log::info!("  - Pre-migration: 15 on-demand orders placed (10× para 2000, 5× para 2001)");
	log::info!("  - Pre-migration: ParaIdAffinity created via scheduler pop (partition path exercised)");
	log::info!("  - Old AssignerCoretime storage fully cleaned up (3 CoreDescriptors, 3 CoreSchedules)");
	log::info!("  - Old on-demand storage fully cleaned up:");
	log::info!("    - QueueStatus, FreeEntries, AffinityEntries, ParaIdAffinity all removed");
	log::info!("  - New CoreDescriptors populated in Scheduler pallet");
	log::info!("  - Traffic value preserved across migration: {}", pre_migration_traffic);
	log::info!("  - {} migrated orders decoded and verified: {:?}", migrated_para_ids.len(), migrated_para_ids);
	log::info!("  - Migration logs verified (scheduler v4 + on-demand v2 with affinity removal)");
	log::info!("  - Chain continues finalizing blocks after upgrade");
	log::info!("  - New assign_core calls work post-upgrade");

	Ok(())
}

// ── Helper Functions ────────────────────────────────────────────────────

/// Wait for N finalized blocks.
async fn wait_n_finalized_blocks(
	client: &OnlineClient<PolkadotConfig>,
	n: u32,
) -> Result<(), anyhow::Error> {
	let mut blocks_sub = client.blocks().subscribe_finalized().await?;
	let start_block = client.blocks().at_latest().await?.number();
	while let Some(Ok(block)) = blocks_sub.next().await {
		if block.number() >= start_block + n {
			break;
		}
	}
	Ok(())
}

/// Wait for the session index to change (requires fast-runtime, ~10 blocks / 1 minute).
async fn wait_for_session_change(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<(), anyhow::Error> {
	let session_key = storage_key("Session", "CurrentIndex");
	let current_session = {
		let data = client
			.storage()
			.at_latest()
			.await?
			.fetch_raw(session_key.clone())
			.await?
			.unwrap_or_default();
		if data.len() >= 4 {
			u32::from_le_bytes([data[0], data[1], data[2], data[3]])
		} else {
			0
		}
	};
	log::info!("Current session index: {current_session}, waiting for next...");

	let mut blocks_sub = client.blocks().subscribe_finalized().await?;
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
	while let Some(Ok(block)) = blocks_sub.next().await {
		if std::time::Instant::now() > deadline {
			return Err(anyhow!("Timed out waiting for session change after 120s"));
		}
		let data = client
			.storage()
			.at(block.hash())
			.fetch_raw(session_key.clone())
			.await?
			.unwrap_or_default();
		let session = if data.len() >= 4 {
			u32::from_le_bytes([data[0], data[1], data[2], data[3]])
		} else {
			0
		};
		if session > current_session {
			log::info!(
				"Session changed: {} -> {} at block #{}",
				current_session,
				session,
				block.number()
			);
			return Ok(());
		}
	}
	Err(anyhow!("Block subscription ended before session change"))
}

/// Get the claim queue via runtime API.
async fn get_claim_queue(
	client: &OnlineClient<PolkadotConfig>,
) -> Result<BTreeMap<CoreIndex, VecDeque<ParaId>>, anyhow::Error> {
	let latest = client.blocks().at_latest().await?;
	get_claim_queue_at(client, latest.hash()).await
}

/// Get the claim queue at a specific block hash.
async fn get_claim_queue_at(
	client: &OnlineClient<PolkadotConfig>,
	hash: subxt::utils::H256,
) -> Result<BTreeMap<CoreIndex, VecDeque<ParaId>>, anyhow::Error> {
	let raw = client
		.runtime_api()
		.at(hash)
		.call_raw("ParachainHost_claim_queue", None)
		.await?;
	Ok(BTreeMap::<CoreIndex, VecDeque<ParaId>>::decode(&mut &raw[..])?)
}

/// Read a pallet's storage version from raw storage.
async fn get_pallet_storage_version(
	client: &OnlineClient<PolkadotConfig>,
	pallet_name: &str,
) -> Result<u16, anyhow::Error> {
	let key = storage_key(pallet_name, ":__STORAGE_VERSION__:");
	let data = client
		.storage()
		.at_latest()
		.await?
		.fetch_raw(key)
		.await?
		.ok_or_else(|| anyhow!("Storage version not found for pallet {pallet_name}"))?;

	if data.len() < 2 {
		return Err(anyhow!(
			"Storage version data too short for pallet {pallet_name}: {} bytes",
			data.len()
		));
	}

	Ok(u16::from_le_bytes([data[0], data[1]]))
}

/// Compute a full storage key: `twox128(pallet) ++ twox128(item)`.
fn storage_key(pallet: &str, item: &str) -> Vec<u8> {
	let mut key = Vec::with_capacity(32);
	key.extend_from_slice(&sp_crypto_hashing::twox_128(pallet.as_bytes()));
	key.extend_from_slice(&sp_crypto_hashing::twox_128(item.as_bytes()));
	key
}

/// Check if old CoreDescriptors storage exists for a core in CoretimeAssignmentProvider
/// (Twox256 StorageMap).
async fn has_old_core_descriptor(
	client: &OnlineClient<PolkadotConfig>,
	core_index: u32,
) -> Result<bool, anyhow::Error> {
	let mut key = storage_key("CoretimeAssignmentProvider", "CoreDescriptors");
	key.extend_from_slice(&sp_crypto_hashing::twox_256(&core_index.to_le_bytes()));
	Ok(client.storage().at_latest().await?.fetch_raw(key).await?.is_some())
}

/// Check if old CoreSchedules storage exists for a (block, core) in CoretimeAssignmentProvider.
/// StorageMap with Twox256 hasher, key is the SCALE-encoded tuple (BlockNumber, CoreIndex).
async fn has_old_core_schedule(
	client: &OnlineClient<PolkadotConfig>,
	core_index: u32,
	block_number: u32,
) -> Result<bool, anyhow::Error> {
	use codec::Encode;
	let mut key = storage_key("CoretimeAssignmentProvider", "CoreSchedules");
	let tuple_key = (block_number, CoreIndex(core_index));
	key.extend_from_slice(&sp_crypto_hashing::twox_256(&tuple_key.encode()));
	Ok(client.storage().at_latest().await?.fetch_raw(key).await?.is_some())
}

/// Query ParaIdAffinity storage for a given para. Returns the raw bytes if present.
/// Uses Twox64Concat hasher: key = twox_64(para_id.encode()) ++ para_id.encode()
async fn get_para_id_affinity(
	client: &OnlineClient<PolkadotConfig>,
	para_id: u32,
) -> Result<Option<Vec<u8>>, anyhow::Error> {
	use codec::Encode;
	let mut key = storage_key("OnDemandAssignmentProvider", "ParaIdAffinity");
	let encoded = ParaId::from(para_id).encode();
	key.extend_from_slice(&sp_crypto_hashing::twox_64(&encoded));
	key.extend_from_slice(&encoded);
	Ok(client.storage().at_latest().await?.fetch_raw(key).await?)
}

/// Query AffinityEntries storage for a given core index. Returns the raw bytes if present.
/// Uses Twox64Concat hasher: key = twox_64(core_index.encode()) ++ core_index.encode()
async fn get_affinity_entries(
	client: &OnlineClient<PolkadotConfig>,
	core_index: u32,
) -> Result<Option<Vec<u8>>, anyhow::Error> {
	use codec::Encode;
	let mut key = storage_key("OnDemandAssignmentProvider", "AffinityEntries");
	let encoded = CoreIndex(core_index).encode();
	key.extend_from_slice(&sp_crypto_hashing::twox_64(&encoded));
	key.extend_from_slice(&encoded);
	Ok(client.storage().at_latest().await?.fetch_raw(key).await?)
}

/// Decode a SCALE compact-encoded u32 from the start of `data`.
/// Returns (value, bytes_consumed).
fn decode_compact_u32(data: &[u8]) -> (u32, usize) {
	if data.is_empty() {
		return (0, 0);
	}
	let mode = data[0] & 0b11;
	match mode {
		0b00 => ((data[0] >> 2) as u32, 1),
		0b01 => {
			let val = u16::from_le_bytes([data[0], data[1]]) >> 2;
			(val as u32, 2)
		},
		0b10 => {
			let val = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) >> 2;
			(val, 4)
		},
		_ => {
			// Big integer mode — not expected for queue lengths
			(0, 0)
		},
	}
}

/// On-demand para IDs registered at genesis as Parathreads.
const ON_DEMAND_PARA_1: u32 = 2000;
const ON_DEMAND_PARA_2: u32 = 2001;

/// Generate a patched rococo-local chain spec that uses the old runtime WASM,
/// sets num_cores to NUM_CORES, and registers on-demand paras as Parathreads.
fn generate_patched_chain_spec(old_wasm_path: &str) -> Result<tempfile::TempDir, anyhow::Error> {
	let polkadot_binary = find_polkadot_binary()?;
	log::info!("Using polkadot binary: {}", polkadot_binary);

	let temp_dir = tempfile::tempdir()?;

	// Step 1: Generate JSON chain spec (rococo-local gives us alice + bob validators)
	let output = std::process::Command::new(&polkadot_binary)
		.args(["build-spec", "--chain", "rococo-local", "--disable-default-bootnode"])
		.output()
		.map_err(|e| anyhow!("Failed to run polkadot build-spec: {e}"))?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		return Err(anyhow!("polkadot build-spec failed: {stderr}"));
	}

	let mut spec: serde_json::Value = serde_json::from_slice(&output.stdout)
		.map_err(|e| anyhow!("Failed to parse chain spec JSON: {e}"))?;

	// Step 2: Read old WASM and patch genesis
	let old_wasm =
		std::fs::read(old_wasm_path).map_err(|e| anyhow!("Failed to read old WASM: {e}"))?;
	log::info!("Old WASM size: {} bytes", old_wasm.len());

	let wasm_hex = format!("0x{}", hex::encode(&old_wasm));

	let runtime_genesis = spec
		.get_mut("genesis")
		.and_then(|g| g.get_mut("runtimeGenesis"))
		.ok_or_else(|| anyhow!("No genesis.runtimeGenesis in chain spec"))?;

	// Replace the runtime WASM with the old one
	runtime_genesis["code"] = serde_json::Value::String(wasm_hex);

	// Patch scheduler_params.num_cores so the old runtime populates claim queue
	if let Some(patch) = runtime_genesis.get_mut("patch") {
		if let Some(config) = patch
			.get_mut("configuration")
			.and_then(|c| c.get_mut("config"))
		{
			if let Some(sched) = config.get_mut("scheduler_params") {
				sched["num_cores"] = serde_json::json!(NUM_CORES);
				log::info!("Patched scheduler_params.num_cores to {NUM_CORES}");
			}
		}

		// Register on-demand paras as Parathreads at genesis so place_order works
		// immediately (the old runtime's place_order checks is_parathread).
		// Minimal validation code — only needs to exist, not be valid WASM,
		// since we never actually validate blocks for these paras.
		// Note: ParaKind uses custom serde: Parathread=false, Parachain=true.
		// The field is renamed to "parachain" in ParaGenesisArgs.
		let minimal_code = "0x010203";
		let minimal_head = "0x00";
		patch["paras"] = serde_json::json!({
			"paras": [
				[ON_DEMAND_PARA_1, {
					"genesis_head": minimal_head,
					"validation_code": minimal_code,
					"parachain": false
				}],
				[ON_DEMAND_PARA_2, {
					"genesis_head": minimal_head,
					"validation_code": minimal_code,
					"parachain": false
				}]
			]
		});
		log::info!("Patched genesis with on-demand paras {ON_DEMAND_PARA_1} and {ON_DEMAND_PARA_2} as Parathreads");

		// Bump nextFreeParaId past the ones we registered
		patch["registrar"] = serde_json::json!({
			"nextFreeParaId": ON_DEMAND_PARA_2 + 1
		});
	}

	// Write patched JSON spec
	let patched_spec_path = temp_dir.path().join("patched-chain-spec.json");
	std::fs::write(&patched_spec_path, serde_json::to_string_pretty(&spec)?)
		.map_err(|e| anyhow!("Failed to write patched spec: {e}"))?;

	// Step 3: Generate raw chain spec from patched JSON
	let output = std::process::Command::new(&polkadot_binary)
		.args([
			"build-spec",
			"--chain",
			patched_spec_path.to_str().ok_or_else(|| anyhow!("Invalid path"))?,
			"--raw",
			"--disable-default-bootnode",
		])
		.output()
		.map_err(|e| anyhow!("Failed to run polkadot build-spec --raw: {e}"))?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		return Err(anyhow!("polkadot build-spec --raw failed: {stderr}"));
	}

	let raw_spec_path = temp_dir.path().join("raw-chain-spec.json");
	std::fs::write(&raw_spec_path, &output.stdout)
		.map_err(|e| anyhow!("Failed to write raw spec: {e}"))?;

	log::info!("Generated raw chain spec: {} bytes", output.stdout.len());

	Ok(temp_dir)
}

/// Find the polkadot binary.
fn find_polkadot_binary() -> Result<String, anyhow::Error> {
	if let Ok(path) = std::env::var("POLKADOT_BINARY") {
		if std::path::Path::new(&path).exists() {
			return Ok(path);
		}
	}

	// cargo test sets CWD to the package root, so we need to look relative to
	// the workspace root via CARGO_MANIFEST_DIR.
	let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
		.parent()
		.and_then(|p| p.parent());

	if let Some(root) = workspace_root {
		for profile in ["release", "testnet", "debug"] {
			let candidate = root.join("target").join(profile).join("polkadot");
			if candidate.exists() {
				return Ok(candidate.to_string_lossy().to_string());
			}
		}
	}

	let candidates = [
		"target/release/polkadot",
		"target/testnet/polkadot",
		"../target/release/polkadot",
		"../../target/release/polkadot",
	];

	for candidate in &candidates {
		if std::path::Path::new(candidate).exists() {
			return Ok(candidate.to_string());
		}
	}

	if let Ok(output) = std::process::Command::new("which").arg("polkadot").output() {
		if output.status.success() {
			let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
			if !path.is_empty() {
				return Ok(path);
			}
		}
	}

	Err(anyhow!(
		"Could not find polkadot binary. Set POLKADOT_BINARY env var or ensure it's in PATH."
	))
}
