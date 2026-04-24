//! E2E tests for the price oracle inherent pipeline.
//!
//! Uses `polkadot-test-client` to build real blocks against the `polkadot-test-runtime`,
//! including oracle inherent data. Verifies that the node-side inherent data format is correctly
//! accepted by the runtime and produces the expected on-chain price updates.
//!
//! This catches integration bugs that unit tests miss — for example, mismatches between the
//! slot the node signs nudges with and the slot the runtime checks against.

use polkadot_test_client::{
	construct_extrinsic, BlockBuilderExt, ClientBlockImportExt, DefaultTestClientBuilderExt,
	InitPolkadotBlockBuilder, TestClientBuilder, TestClientBuilderExt,
};
use sp_api::ProvideRuntimeApi;
use sp_consensus::BlockOrigin;
use sp_consensus_babe::AuthoritySignature;
use sp_consensus_slots::Slot;
use sp_core::crypto::Pair as PairT;
use sp_price_oracle::{Nudge, PriceOracleApi, SignedNudge};
use sp_runtime::{
	traits::{Block as BlockT, Zero},
	FixedU128,
};

fn make_signed_nudge(
	pair: &sp_core::sr25519::Pair,
	nudge: Nudge,
	slot: u64,
	authority_index: u32,
) -> SignedNudge {
	let slot = Slot::from(slot);
	let payload = SignedNudge::signing_payload(&nudge, slot);
	let sig = pair.sign(&payload);
	SignedNudge { nudge, slot, authority_index, signature: AuthoritySignature::from(sig) }
}

fn alice_babe_pair() -> sp_core::sr25519::Pair {
	sp_core::sr25519::Pair::from_string("//Alice", None).expect("valid seed")
}

fn bob_babe_pair() -> sp_core::sr25519::Pair {
	sp_core::sr25519::Pair::from_string("//Bob", None).expect("valid seed")
}

#[test]
fn block_with_oracle_inherent_builds_and_imports() {
	let client = TestClientBuilder::new().build();
	let price_before = client
		.runtime_api()
		.current_price(client.chain_info().best_hash, 0u8)
		.expect("price before");
	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let alice = alice_babe_pair();

	// MinNudges=1 requires at least one nudge for a valid block
	let nudges = vec![make_signed_nudge(&alice, Nudge::Up, slot, 0)];

	let block_builder = client.init_polkadot_block_builder_with_nudges(nudges);
	let block = block_builder.build().expect("Finalizes the block").block;

	futures::executor::block_on(client.import(BlockOrigin::Own, block)).expect("Imports the block");
	let price_after = client
		.runtime_api()
		.current_price(client.chain_info().best_hash, 0u8)
		.expect("price after");
	assert_eq!(price_after, price_before + FixedU128::from_rational(1, 100));
}

#[test]
fn block_with_nudges_updates_price() {
	let client = TestClientBuilder::new().build();

	let best = client.chain_info().best_hash;
	let slot: u64 = client
		.runtime_api()
		.current_price(best, 0u8)
		.map(|_| {
			// Get the slot that the next block will have — use a large enough value
			// that won't be stale. The test runtime's first block will have slot derived
			// from system time / slot_duration.
			let now_ms = std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_millis() as u64;
			// SlotDuration in test runtime is 6000ms (SLOT_DURATION constant)
			now_ms / 6000
		})
		.unwrap();

	let alice = alice_babe_pair();
	let bob = bob_babe_pair();

	let nudges = vec![
		make_signed_nudge(&alice, Nudge::Up, slot, 0),
		make_signed_nudge(&bob, Nudge::Up, slot, 1),
	];

	let block_builder = client.init_polkadot_block_builder_with_nudges(nudges);
	let block = block_builder.build().expect("Finalizes the block").block;
	let block_hash = block.hash();

	futures::executor::block_on(client.import(BlockOrigin::Own, block)).expect("Imports the block");

	let price = client.runtime_api().current_price(block_hash, 0u8).expect("queries price");
	// 2 Up nudges × epsilon(0.01) = 0.02
	assert_eq!(price, FixedU128::from_rational(2, 100));
}

#[test]
fn multiple_blocks_accumulate_price() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let alice = alice_babe_pair();
	let bob = bob_babe_pair();

	// Block 1: 2 Up nudges → price = 0.02
	let nudges = vec![
		make_signed_nudge(&alice, Nudge::Up, slot, 0),
		make_signed_nudge(&bob, Nudge::Up, slot, 1),
	];
	let block = client
		.init_polkadot_block_builder_with_nudges(nudges)
		.build()
		.expect("block 1")
		.block;
	let hash1 = block.hash();
	futures::executor::block_on(client.import(BlockOrigin::Own, block)).expect("import block 1");

	let price1 = client.runtime_api().current_price(hash1, 0u8).expect("price after block 1");
	assert_eq!(price1, FixedU128::from_rational(2, 100));

	// Block 2: 1 Up, 1 Down → net 0, price stays at 0.02
	let slot2 = slot + 1;
	let nudges2 = vec![
		make_signed_nudge(&alice, Nudge::Up, slot2, 0),
		make_signed_nudge(&bob, Nudge::Down, slot2, 1),
	];
	let block2 = client
		.init_polkadot_block_builder_with_nudges(nudges2)
		.build()
		.expect("block 2")
		.block;
	let hash2 = block2.hash();
	futures::executor::block_on(client.import(BlockOrigin::Own, block2)).expect("import block 2");

	let price2 = client.runtime_api().current_price(hash2, 0u8).expect("price after block 2");
	assert_eq!(price2, FixedU128::from_rational(2, 100));
}

#[test]
#[should_panic(expected = "BadMandatory")]
fn duplicate_authority_nudges_rejected() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let alice = alice_babe_pair();

	// Two nudges from alice (authority 0) — duplicate should cause an error
	let nudges = vec![
		make_signed_nudge(&alice, Nudge::Up, slot, 0),
		make_signed_nudge(&alice, Nudge::Up, slot - 1, 0),
	];

	let _block_builder = client.init_polkadot_block_builder_with_nudges(nudges);
}

#[test]
#[should_panic(expected = "BadMandatory")]
fn bad_signature_nudge_rejected() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let bob = bob_babe_pair();

	// Sign with bob's key but claim authority_index=0 (alice's) → bad sig
	let nudges = vec![
		make_signed_nudge(&bob, Nudge::Up, slot, 0),
		make_signed_nudge(&bob, Nudge::Up, slot, 1),
	];

	let _block_builder = client.init_polkadot_block_builder_with_nudges(nudges);
}

#[test]
fn single_nudge_updates_price() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let alice = alice_babe_pair();

	// MinNudges=0 allows a single nudge
	let nudges = vec![make_signed_nudge(&alice, Nudge::Up, slot, 0)];

	let block = client
		.init_polkadot_block_builder_with_nudges(nudges)
		.build()
		.expect("block")
		.block;
	let hash = block.hash();
	futures::executor::block_on(client.import(BlockOrigin::Own, block)).expect("import");

	// 1 Up nudge × epsilon(0.01) = 0.01
	let price = client.runtime_api().current_price(hash, 0u8).expect("price");
	assert_eq!(price, FixedU128::from_rational(1, 100));
}

/// Helper: build and import a block that toggles `inherent_mandatory=true` on pair 0 via sudo.
///
/// The block must include a valid nudge so that `InherentSeen[0] = true` — otherwise
/// `on_finalize` would read the freshly-updated `inherent_mandatory=true` and panic. The node
/// author is responsible for coupling config changes with an inherent entry.
fn make_pair_mandatory(client: &polkadot_test_client::Client) {
	use polkadot_test_client::runtime::RuntimeCall;
	use sp_price_oracle::PairConfig;

	let cfg = PairConfig {
		min_nudges: 1,
		nudge_validity: 10,
		inherent_mandatory: true,
		invalid_inherent_panics: false,
		epsilon: FixedU128::from_rational(1, 100),
	};

	let inner = RuntimeCall::PriceOracle(pallet_price_oracle::Call::update_pair_config {
		pair_id: 0u8,
		config: cfg,
	});
	let sudo = RuntimeCall::Sudo(pallet_sudo::Call::sudo { call: Box::new(inner) });
	let ext = construct_extrinsic(client, sudo, sp_keyring::Sr25519Keyring::Alice, 0);

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;
	let alice = alice_babe_pair();
	let nudges = vec![make_signed_nudge(&alice, Nudge::Up, slot, 0)];

	let mut block_builder = client.init_polkadot_block_builder_with_nudges(nudges);
	block_builder.push_polkadot_extrinsic(ext).expect("pushes sudo extrinsic");

	let block = block_builder.build().expect("builds block").block;
	futures::executor::block_on(client.import(BlockOrigin::Own, block))
		.expect("imports block that sets mandatory flag");
}

#[test]
fn mandatory_pair_with_inherent_does_not_panic() {
	let client = TestClientBuilder::new().build();
	make_pair_mandatory(&client);

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;
	let alice = alice_babe_pair();

	let block = client
		.init_polkadot_block_builder_with_nudges(vec![make_signed_nudge(
			&alice,
			Nudge::Up,
			slot,
			0,
		)])
		.build()
		.expect("block with inherent builds despite mandatory flag")
		.block;

	futures::executor::block_on(client.import(BlockOrigin::Own, block))
		.expect("block with inherent imports despite mandatory flag");
}

#[test]
fn mandatory_pair_without_inherent_panics() {
	let client = TestClientBuilder::new().build();
	make_pair_mandatory(&client);

	let result = client.init_polkadot_build_block_without_price_oracle_inherent().build();
	assert!(result.is_err(), "block without oracle inherent must fail when the pair is mandatory",);
}

#[test]
fn down_nudges_decrease_price() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let alice = alice_babe_pair();
	let bob = bob_babe_pair();

	// Block 1: 2 Up nudges → price = 0.02
	let nudges = vec![
		make_signed_nudge(&alice, Nudge::Up, slot, 0),
		make_signed_nudge(&bob, Nudge::Up, slot, 1),
	];
	let block = client
		.init_polkadot_block_builder_with_nudges(nudges)
		.build()
		.expect("block 1")
		.block;
	let hash1 = block.hash();
	futures::executor::block_on(client.import(BlockOrigin::Own, block)).expect("import block 1");

	let price1 = client.runtime_api().current_price(hash1, 0u8).expect("price after block 1");
	assert_eq!(price1, FixedU128::from_rational(2, 100));

	// Block 2: 2 Down nudges → price = 0.00
	let slot2 = slot + 1;
	let nudges2 = vec![
		make_signed_nudge(&alice, Nudge::Down, slot2, 0),
		make_signed_nudge(&bob, Nudge::Down, slot2, 1),
	];
	let block2 = client
		.init_polkadot_block_builder_with_nudges(nudges2)
		.build()
		.expect("block 2")
		.block;
	let hash2 = block2.hash();
	futures::executor::block_on(client.import(BlockOrigin::Own, block2)).expect("import block 2");

	// 0 ups, 2 downs → net 2 down → price = 0.02 - 0.02 = 0.00
	let price2 = client.runtime_api().current_price(hash2, 0u8).expect("price after block 2");
	assert_eq!(price2, FixedU128::zero());
}

#[test]
fn price_floor_at_zero_with_down_nudges() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let alice = alice_babe_pair();
	let bob = bob_babe_pair();

	// Down nudges when price is 0 → should stay at 0 (saturating)
	let nudges = vec![
		make_signed_nudge(&alice, Nudge::Down, slot, 0),
		make_signed_nudge(&bob, Nudge::Down, slot, 1),
	];
	let block = client
		.init_polkadot_block_builder_with_nudges(nudges)
		.build()
		.expect("block")
		.block;
	let hash = block.hash();
	futures::executor::block_on(client.import(BlockOrigin::Own, block)).expect("import");

	let price = client.runtime_api().current_price(hash, 0u8).expect("price");
	assert_eq!(price, FixedU128::zero());
}

#[test]
#[should_panic(expected = "BadMandatory")]
fn stale_nudge_rejected() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let alice = alice_babe_pair();

	// Nudge with slot older than validity window (10 slots) → stale
	let nudges = vec![make_signed_nudge(&alice, Nudge::Up, slot - 11, 0)];

	let _block_builder = client.init_polkadot_block_builder_with_nudges(nudges);
}

#[test]
#[should_panic(expected = "BadMandatory")]
fn invalid_authority_index_rejected() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let alice = alice_babe_pair();

	// authority_index=99 is out of range (only 0 and 1 exist)
	let nudges = vec![make_signed_nudge(&alice, Nudge::Up, slot, 99)];

	let _block_builder = client.init_polkadot_block_builder_with_nudges(nudges);
}

#[test]
fn runtime_api_returns_correct_values() {
	let client = TestClientBuilder::new().build();
	let best = client.chain_info().best_hash;

	let pairs = client.runtime_api().list_pairs(best).expect("list_pairs");
	assert_eq!(pairs, vec![0u8]);

	let cfg = client
		.runtime_api()
		.pair_config(best, 0u8)
		.expect("pair_config call")
		.expect("pair 0 exists");
	assert_eq!(cfg.epsilon, FixedU128::from_rational(1, 100));
	assert_eq!(cfg.nudge_validity, 10u64);
	assert_eq!(cfg.min_nudges, 1u32);

	let authorities = client.runtime_api().authorities(best).expect("authorities");
	assert_eq!(authorities.len(), 2);
}

#[test]
fn three_consecutive_up_blocks_accumulate() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let alice = alice_babe_pair();
	let bob = bob_babe_pair();

	// Block 1: 2 Up → price = 0.02
	let block = client
		.init_polkadot_block_builder_with_nudges(vec![
			make_signed_nudge(&alice, Nudge::Up, slot, 0),
			make_signed_nudge(&bob, Nudge::Up, slot, 1),
		])
		.build()
		.expect("block 1")
		.block;
	futures::executor::block_on(client.import(BlockOrigin::Own, block)).expect("import block 1");

	// Block 2: 2 Up → price = 0.04
	let block2 = client
		.init_polkadot_block_builder_with_nudges(vec![
			make_signed_nudge(&alice, Nudge::Up, slot + 1, 0),
			make_signed_nudge(&bob, Nudge::Up, slot + 1, 1),
		])
		.build()
		.expect("block 2")
		.block;
	futures::executor::block_on(client.import(BlockOrigin::Own, block2)).expect("import block 2");

	// Block 3: 2 Up → price = 0.06
	let block3 = client
		.init_polkadot_block_builder_with_nudges(vec![
			make_signed_nudge(&alice, Nudge::Up, slot + 2, 0),
			make_signed_nudge(&bob, Nudge::Up, slot + 2, 1),
		])
		.build()
		.expect("block 3")
		.block;
	let hash3 = block3.hash();
	futures::executor::block_on(client.import(BlockOrigin::Own, block3)).expect("import block 3");

	let price = client.runtime_api().current_price(hash3, 0u8).expect("price after block 3");
	assert_eq!(price, FixedU128::from_rational(6, 100));
}

#[test]
fn price_increases_then_decreases_across_blocks() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;

	let alice = alice_babe_pair();
	let bob = bob_babe_pair();

	// Block 1: 2 Up → price = 0.02
	let block = client
		.init_polkadot_block_builder_with_nudges(vec![
			make_signed_nudge(&alice, Nudge::Up, slot, 0),
			make_signed_nudge(&bob, Nudge::Up, slot, 1),
		])
		.build()
		.expect("block 1")
		.block;
	futures::executor::block_on(client.import(BlockOrigin::Own, block)).expect("import block 1");

	// Block 2: 2 Up → price = 0.04
	let block2 = client
		.init_polkadot_block_builder_with_nudges(vec![
			make_signed_nudge(&alice, Nudge::Up, slot + 1, 0),
			make_signed_nudge(&bob, Nudge::Up, slot + 1, 1),
		])
		.build()
		.expect("block 2")
		.block;
	futures::executor::block_on(client.import(BlockOrigin::Own, block2)).expect("import block 2");

	// Block 3: 2 Down → price = 0.04 - 0.02 = 0.02
	let block3 = client
		.init_polkadot_block_builder_with_nudges(vec![
			make_signed_nudge(&alice, Nudge::Down, slot + 2, 0),
			make_signed_nudge(&bob, Nudge::Down, slot + 2, 1),
		])
		.build()
		.expect("block 3")
		.block;
	let hash3 = block3.hash();
	futures::executor::block_on(client.import(BlockOrigin::Own, block3)).expect("import block 3");

	let price = client.runtime_api().current_price(hash3, 0u8).expect("price after block 3");
	assert_eq!(price, FixedU128::from_rational(2, 100));
}

/// Helper: register a second pair (id 1) with the same config as pair 0 via sudo.
fn register_pair_one(client: &polkadot_test_client::Client) {
	use polkadot_test_client::runtime::RuntimeCall;
	use sp_price_oracle::PairConfig;

	let cfg = PairConfig {
		min_nudges: 1,
		nudge_validity: 10,
		inherent_mandatory: false,
		invalid_inherent_panics: false,
		epsilon: FixedU128::from_rational(1, 100),
	};
	let inner = RuntimeCall::PriceOracle(pallet_price_oracle::Call::register_pair {
		pair_id: 1u8,
		config: cfg,
	});
	let sudo = RuntimeCall::Sudo(pallet_sudo::Call::sudo { call: Box::new(inner) });
	let ext = construct_extrinsic(client, sudo, sp_keyring::Sr25519Keyring::Alice, 0);

	let mut block_builder = client.init_polkadot_block_builder();
	block_builder
		.push_polkadot_extrinsic(ext)
		.expect("pushes register_pair extrinsic");
	let block = block_builder.build().expect("builds block").block;
	futures::executor::block_on(client.import(BlockOrigin::Own, block))
		.expect("imports register_pair block");
}

#[test]
fn block_with_two_pair_groups_updates_both_prices() {
	let client = TestClientBuilder::new().build();
	register_pair_one(&client);

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;
	let alice = alice_babe_pair();
	let bob = bob_babe_pair();

	let pair_nudges = vec![
		(0u8, vec![make_signed_nudge(&alice, Nudge::Up, slot, 0)]),
		(1u8, vec![make_signed_nudge(&bob, Nudge::Up, slot, 1)]),
	];

	let block = client
		.init_polkadot_block_builder_with_pair_nudges(pair_nudges)
		.build()
		.expect("multi-pair block builds")
		.block;
	let hash = block.hash();
	futures::executor::block_on(client.import(BlockOrigin::Own, block))
		.expect("import multi-pair block");

	let p0 = client.runtime_api().current_price(hash, 0u8).expect("pair 0 price");
	let p1 = client.runtime_api().current_price(hash, 1u8).expect("pair 1 price");
	assert_eq!(p0, FixedU128::from_rational(1, 100));
	assert_eq!(p1, FixedU128::from_rational(1, 100));
}

#[test]
#[should_panic(expected = "BadMandatory")]
fn unknown_pair_in_inherent_rejected() {
	let client = TestClientBuilder::new().build();

	let now_ms = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as u64;
	let slot = now_ms / 6000;
	let alice = alice_babe_pair();

	// Pair id 9 is not registered — block authoring must fail.
	let _b = client.init_polkadot_block_builder_with_pair_nudges(vec![(
		9u8,
		vec![make_signed_nudge(&alice, Nudge::Up, slot, 0)],
	)]);
}
