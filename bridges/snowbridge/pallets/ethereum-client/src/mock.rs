// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate as ethereum_beacon_client;
use crate::config;
use frame_support::{derive_impl, parameter_types};
use hex_literal::hex;
use pallet_timestamp;
use primitives::{CompactExecutionHeader, Fork, ForkVersions};
use snowbridge_core::inbound::{Log, Proof};
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

pub fn load_execution_header_update_fixture() -> primitives::ExecutionHeaderUpdate {
	load_fixture("execution-header-update.json".to_string()).unwrap()
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
	(
		Log {
			address: hex!("ee9170abfbf9421ad6dd07f6bdec9d89f2b581e0").into(),
			topics: vec![
				hex!("1b11dcf133cc240f682dab2d3a8e4cd35c5da8c9cf99adac4336f8512584c5ad").into(),
				hex!("00000000000000000000000000000000000000000000000000000000000003e8").into(),
				hex!("0000000000000000000000000000000000000000000000000000000000000001").into(),
			],
			data: hex!("0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000004b000f000000000000000100d184c103f7acc340847eee82a0b909e3358bc28d440edffa1352b13227e8ee646f3ea37456dec701345772617070656420457468657210574554481235003511000000000000000000000000000000000000000000").into(),
		},
		Proof {
			block_hash: hex!("05aaa60b0f27cce9e71909508527264b77ee14da7b5bf915fcc4e32715333213").into(),
			tx_index: 0,
			data: (vec![
				hex!("cf0d1c1ba57d1e0edfb59786c7e30c2b7e12bd54612b00cd21c4eaeecedf44fb").to_vec(),
				hex!("d21fc4f68ab05bc4dcb23c67008e92c4d466437cdd6ed7aad0c008944c185510").to_vec(),
				hex!("b9890f91ca0d77aa2a4adfaf9b9e40c94cac9e638b6d9797923865872944b646").to_vec(),
			], vec![
				hex!("f90131a0b601337b3aa10a671caa724eba641e759399979856141d3aea6b6b4ac59b889ba00c7d5dd48be9060221a02fb8fa213860b4c50d47046c8fa65ffaba5737d569e0a094601b62a1086cd9c9cb71a7ebff9e718f3217fd6e837efe4246733c0a196f63a06a4b0dd0aefc37b3c77828c8f07d1b7a2455ceb5dbfd3c77d7d6aeeddc2f7e8ca0d6e8e23142cdd8ec219e1f5d8b56aa18e456702b195deeaa210327284d42ade4a08a313d4c87023005d1ab631bbfe3f5de1e405d0e66d0bef3e033f1e5711b5521a0bf09a5d9a48b10ade82b8d6a5362a15921c8b5228a3487479b467db97411d82fa0f95cccae2a7c572ef3c566503e30bac2b2feb2d2f26eebf6d870dcf7f8cf59cea0d21fc4f68ab05bc4dcb23c67008e92c4d466437cdd6ed7aad0c008944c1855108080808080808080").to_vec(),
				hex!("f851a0b9890f91ca0d77aa2a4adfaf9b9e40c94cac9e638b6d9797923865872944b646a060a634b9280e3a23fb63375e7bbdd9ab07fd379ab6a67e2312bbc112195fa358808080808080808080808080808080").to_vec(),
				hex!("f9030820b9030402f90300018301d6e2b9010000000000000800000000000020040008000000000000000000000000400000008000000000000000000000000000000000000000000000000000000000042010000000001000000000000000000000000000000000040000000000000000000000000000000000000000000000008000000000000000002000000000000000000000000200000000000000200000000000100000000040000001000200008000000000000200000000000000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000000f901f5f87a942ffa5ecdbe006d30397c7636d3e015eee251369ff842a0c965575a00553e094ca7c5d14f02e107c258dda06867cbf9e0e69f80e71bbcc1a000000000000000000000000000000000000000000000000000000000000003e8a000000000000000000000000000000000000000000000000000000000000003e8f9011c94ee9170abfbf9421ad6dd07f6bdec9d89f2b581e0f863a01b11dcf133cc240f682dab2d3a8e4cd35c5da8c9cf99adac4336f8512584c5ada000000000000000000000000000000000000000000000000000000000000003e8a00000000000000000000000000000000000000000000000000000000000000001b8a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000004b000f000000000000000100d184c103f7acc340847eee82a0b909e3358bc28d440edffa1352b13227e8ee646f3ea37456dec701345772617070656420457468657210574554481235003511000000000000000000000000000000000000000000f858948cf6147918a5cbb672703f879f385036f8793a24e1a01449abf21e49fd025f33495e77f7b1461caefdd3d4bb646424a3f445c4576a5ba0000000000000000000000000440edffa1352b13227e8ee646f3ea37456dec701").to_vec(),
			]),
		}
	)
}

pub fn get_message_verification_header() -> CompactExecutionHeader {
	CompactExecutionHeader {
		parent_hash: hex!("04a7f6ab8282203562c62f38b0ab41d32aaebe2c7ea687702b463148a6429e04")
			.into(),
		block_number: 55,
		state_root: hex!("894d968712976d613519f973a317cb0781c7b039c89f27ea2b7ca193f7befdb3").into(),
		receipts_root: hex!("cf0d1c1ba57d1e0edfb59786c7e30c2b7e12bd54612b00cd21c4eaeecedf44fb")
			.into(),
	}
}

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system::{Pallet, Call, Storage, Event<T>},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
		EthereumBeaconClient: ethereum_beacon_client::{Pallet, Call, Storage, Event<T>},
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
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
	pub const ExecutionHeadersPruneThreshold: u32 = 8192;
}

impl ethereum_beacon_client::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ForkVersions = ChainForkVersions;
	type MaxExecutionHeadersToKeep = ExecutionHeadersPruneThreshold;
	type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_tester() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	let _ = ext.execute_with(|| Timestamp::set(RuntimeOrigin::signed(1), 30_000));
	ext
}
