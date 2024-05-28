// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate as ethereum_beacon_client;
use crate::config;
use frame_support::{derive_impl, dispatch::DispatchResult, parameter_types};
use pallet_timestamp;
use primitives::{Fork, ForkVersions};
use snowbridge_core::inbound::{Log, Proof};
use sp_std::default::Default;
use std::{fs::File, path::PathBuf};

type Block = frame_system::mocking::MockBlock<Test>;
use sp_runtime::BuildStorage;

fn load_fixture<T>(basename: String) -> Result<T, serde_json::Error>
where
	T: for<'de> serde::Deserialize<'de>,
{
	let filepath: PathBuf =
		[env!("CARGO_MANIFEST_DIR"), "tests", "fixtures", &basename].iter().collect();
	serde_json::from_reader(File::open(filepath).unwrap())
}

pub fn load_execution_proof_fixture() -> primitives::ExecutionProof {
	load_fixture("execution-proof.json".to_string()).unwrap()
}

pub fn load_checkpoint_update_fixture(
) -> primitives::CheckpointUpdate<{ config::SYNC_COMMITTEE_SIZE }> {
	load_fixture("initial-checkpoint.json".to_string()).unwrap()
}

pub fn load_sync_committee_update_fixture(
) -> primitives::Update<{ config::SYNC_COMMITTEE_SIZE }, { config::SYNC_COMMITTEE_BITS_SIZE }> {
	load_fixture("sync-committee-update.json".to_string()).unwrap()
}

pub fn load_finalized_header_update_fixture(
) -> primitives::Update<{ config::SYNC_COMMITTEE_SIZE }, { config::SYNC_COMMITTEE_BITS_SIZE }> {
	load_fixture("finalized-header-update.json".to_string()).unwrap()
}

pub fn load_next_sync_committee_update_fixture(
) -> primitives::Update<{ config::SYNC_COMMITTEE_SIZE }, { config::SYNC_COMMITTEE_BITS_SIZE }> {
	load_fixture("next-sync-committee-update.json".to_string()).unwrap()
}

pub fn load_next_finalized_header_update_fixture(
) -> primitives::Update<{ config::SYNC_COMMITTEE_SIZE }, { config::SYNC_COMMITTEE_BITS_SIZE }> {
	load_fixture("next-finalized-header-update.json".to_string()).unwrap()
}

pub fn get_message_verification_payload() -> (Log, Proof) {
	let inbound_fixture = snowbridge_pallet_ethereum_client_fixtures::make_inbound_fixture();
	(inbound_fixture.message.event_log, inbound_fixture.message.proof)
}

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system::{Pallet, Call, Storage, Event<T>},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
		EthereumBeaconClient: ethereum_beacon_client::{Pallet, Call, Storage, Event<T>},
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl pallet_timestamp::Config for Test {
	type Moment = u64;
	type OnTimestampSet = ();
	type MinimumPeriod = ();
	type WeightInfo = ();
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

impl ethereum_beacon_client::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ForkVersions = ChainForkVersions;
	type WeightInfo = ();
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
