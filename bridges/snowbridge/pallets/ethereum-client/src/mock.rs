// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate as ethereum_beacon_client;
use crate::config;
use frame_support::{
	derive_impl, dispatch::DispatchResult, pallet_prelude::Weight, parameter_types,
};
use snowbridge_beacon_primitives::{Fork, ForkVersions};
use snowbridge_core::inbound::{Log, Proof};
use sp_std::default::Default;
use std::{fs::File, path::PathBuf};

type Block = frame_system::mocking::MockBlock<Test>;
use frame_support::{
	migrations::MultiStepMigrator,
	traits::{ConstU32, OnFinalize, OnInitialize},
};
use sp_runtime::BuildStorage;

fn load_fixture<T>(basename: String) -> Result<T, serde_json::Error>
where
	T: for<'de> serde::Deserialize<'de>,
{
	let filepath: PathBuf =
		[env!("CARGO_MANIFEST_DIR"), "tests", "fixtures", &basename].iter().collect();
	serde_json::from_reader(File::open(filepath).unwrap())
}

pub fn load_execution_proof_fixture() -> snowbridge_beacon_primitives::ExecutionProof {
	load_fixture("execution-proof.json".to_string()).unwrap()
}

pub fn load_checkpoint_update_fixture(
) -> snowbridge_beacon_primitives::CheckpointUpdate<{ config::SYNC_COMMITTEE_SIZE }> {
	load_fixture("initial-checkpoint.json".to_string()).unwrap()
}

pub fn load_sync_committee_update_fixture() -> snowbridge_beacon_primitives::Update<
	{ config::SYNC_COMMITTEE_SIZE },
	{ config::SYNC_COMMITTEE_BITS_SIZE },
> {
	load_fixture("sync-committee-update.json".to_string()).unwrap()
}

pub fn load_finalized_header_update_fixture() -> snowbridge_beacon_primitives::Update<
	{ config::SYNC_COMMITTEE_SIZE },
	{ config::SYNC_COMMITTEE_BITS_SIZE },
> {
	load_fixture("finalized-header-update.json".to_string()).unwrap()
}

pub fn load_next_sync_committee_update_fixture() -> snowbridge_beacon_primitives::Update<
	{ config::SYNC_COMMITTEE_SIZE },
	{ config::SYNC_COMMITTEE_BITS_SIZE },
> {
	load_fixture("next-sync-committee-update.json".to_string()).unwrap()
}

pub fn load_next_finalized_header_update_fixture() -> snowbridge_beacon_primitives::Update<
	{ config::SYNC_COMMITTEE_SIZE },
	{ config::SYNC_COMMITTEE_BITS_SIZE },
> {
	load_fixture("next-finalized-header-update.json".to_string()).unwrap()
}

pub fn load_sync_committee_update_period_0() -> Box<
	snowbridge_beacon_primitives::Update<
		{ config::SYNC_COMMITTEE_SIZE },
		{ config::SYNC_COMMITTEE_BITS_SIZE },
	>,
> {
	Box::new(load_fixture("sync-committee-update-period-0.json".to_string()).unwrap())
}

pub fn load_sync_committee_update_period_0_older_fixture() -> Box<
	snowbridge_beacon_primitives::Update<
		{ config::SYNC_COMMITTEE_SIZE },
		{ config::SYNC_COMMITTEE_BITS_SIZE },
	>,
> {
	Box::new(load_fixture("sync-committee-update-period-0-older.json".to_string()).unwrap())
}

pub fn load_sync_committee_update_period_0_newer_fixture() -> Box<
	snowbridge_beacon_primitives::Update<
		{ config::SYNC_COMMITTEE_SIZE },
		{ config::SYNC_COMMITTEE_BITS_SIZE },
	>,
> {
	Box::new(load_fixture("sync-committee-update-period-0-newer.json".to_string()).unwrap())
}

pub fn get_message_verification_payload() -> (Log, Proof) {
	let inbound_fixture = snowbridge_pallet_ethereum_client_fixtures::make_inbound_fixture();
	(inbound_fixture.message.event_log, inbound_fixture.message.proof)
}

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		EthereumBeaconClient: crate,
		Migrator: pallet_migrations,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type MultiBlockMigrator = Migrator;
}

parameter_types! {
	pub const ChainForkVersions: ForkVersions = ForkVersions {
		genesis: Fork {
			version: [0, 0, 0, 0], // 0x00000000
			epoch: 0,
		},
		altair: Fork {
			version: [1, 0, 0, 0], // 0x01000000
			epoch: 0,
		},
		bellatrix: Fork {
			version: [2, 0, 0, 0], // 0x02000000
			epoch: 0,
		},
		capella: Fork {
			version: [3, 0, 0, 0], // 0x03000000
			epoch: 0,
		},
		deneb: Fork {
			version: [4, 0, 0, 0], // 0x90000073
			epoch: 0,
		}
	};
}

pub const FREE_SLOTS_INTERVAL: u32 = config::SLOTS_PER_EPOCH as u32;

impl ethereum_beacon_client::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ForkVersions = ChainForkVersions;
	type FreeHeadersInterval = ConstU32<FREE_SLOTS_INTERVAL>;
	type WeightInfo = ();
}

parameter_types! {
	pub storage ExecutionHeaderCount: u32 = 100;
	pub storage MigratorServiceWeight: Weight = Weight::from_parts(100, 100);
}

#[derive_impl(pallet_migrations::config_preludes::TestDefaultConfig)]
impl pallet_migrations::Config for Test {
	#[cfg(not(feature = "runtime-benchmarks"))]
	type Migrations = (
		crate::migration::v0_to_v1::EthereumExecutionHeaderCleanup<Test, (), ExecutionHeaderCount>,
	);
	#[cfg(feature = "runtime-benchmarks")]
	type Migrations = pallet_migrations::mock_helpers::MockedMigrations;
	type MaxServiceWeight = MigratorServiceWeight;
}

// Build genesis storage according to the mock runtime.
pub fn new_tester() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let ext = sp_io::TestExternalities::new(t);
	ext
}

pub fn initialize_storage() -> DispatchResult {
	let inbound_fixture = snowbridge_pallet_ethereum_client_fixtures::make_inbound_fixture();
	EthereumBeaconClient::store_finalized_header(
		inbound_fixture.finalized_header,
		inbound_fixture.block_roots_root,
	)
}

pub fn run_to_block_with_migrator(n: u64) {
	assert!(System::block_number() < n);
	while System::block_number() < n {
		let b = System::block_number();
		AllPalletsWithSystem::on_finalize(b);
		// Done by Executive:
		<Test as frame_system::Config>::MultiBlockMigrator::step();
		System::set_block_number(b + 1);
		AllPalletsWithSystem::on_initialize(b + 1);
	}
}
