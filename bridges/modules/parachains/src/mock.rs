// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use bp_header_chain::ChainWithGrandpa;
use bp_polkadot_core::parachains::ParaId;
use bp_runtime::{Chain, ChainId, Parachain};
use frame_support::{
	construct_runtime, derive_impl, parameter_types, traits::ConstU32, weights::Weight,
};
use sp_runtime::{
	testing::H256,
	traits::{BlakeTwo256, Header as HeaderT},
	MultiSignature,
};

use crate as pallet_bridge_parachains;

pub type AccountId = u64;

pub type RelayBlockHeader =
	sp_runtime::generic::Header<crate::RelayBlockNumber, crate::RelayBlockHasher>;

type Block = frame_system::mocking::MockBlock<TestRuntime>;

pub const PARAS_PALLET_NAME: &str = "Paras";
pub const UNTRACKED_PARACHAIN_ID: u32 = 10;
// use exact expected encoded size: `vec_len_size + header_number_size + state_root_hash_size`
pub const MAXIMAL_PARACHAIN_HEAD_DATA_SIZE: u32 = 1 + 8 + 32;
// total parachains that we use in tests
pub const TOTAL_PARACHAINS: u32 = 4;

pub type RegularParachainHeader = sp_runtime::testing::Header;
pub type RegularParachainHasher = BlakeTwo256;
pub type BigParachainHeader = sp_runtime::generic::Header<u128, BlakeTwo256>;

pub struct Parachain1;

impl Chain for Parachain1 {
	const ID: ChainId = *b"pch1";

	type BlockNumber = u64;
	type Hash = H256;
	type Hasher = RegularParachainHasher;
	type Header = RegularParachainHeader;
	type AccountId = u64;
	type Balance = u64;
	type Nonce = u64;
	type Signature = MultiSignature;

	fn max_extrinsic_size() -> u32 {
		0
	}
	fn max_extrinsic_weight() -> Weight {
		Weight::zero()
	}
}

impl Parachain for Parachain1 {
	const PARACHAIN_ID: u32 = 1;
}

pub struct Parachain2;

impl Chain for Parachain2 {
	const ID: ChainId = *b"pch2";

	type BlockNumber = u64;
	type Hash = H256;
	type Hasher = RegularParachainHasher;
	type Header = RegularParachainHeader;
	type AccountId = u64;
	type Balance = u64;
	type Nonce = u64;
	type Signature = MultiSignature;

	fn max_extrinsic_size() -> u32 {
		0
	}
	fn max_extrinsic_weight() -> Weight {
		Weight::zero()
	}
}

impl Parachain for Parachain2 {
	const PARACHAIN_ID: u32 = 2;
}

pub struct Parachain3;

impl Chain for Parachain3 {
	const ID: ChainId = *b"pch3";

	type BlockNumber = u64;
	type Hash = H256;
	type Hasher = RegularParachainHasher;
	type Header = RegularParachainHeader;
	type AccountId = u64;
	type Balance = u64;
	type Nonce = u64;
	type Signature = MultiSignature;

	fn max_extrinsic_size() -> u32 {
		0
	}
	fn max_extrinsic_weight() -> Weight {
		Weight::zero()
	}
}

impl Parachain for Parachain3 {
	const PARACHAIN_ID: u32 = 3;
}

// this parachain is using u128 as block number and stored head data size exceeds limit
pub struct BigParachain;

impl Chain for BigParachain {
	const ID: ChainId = *b"bpch";

	type BlockNumber = u128;
	type Hash = H256;
	type Hasher = RegularParachainHasher;
	type Header = BigParachainHeader;
	type AccountId = u64;
	type Balance = u64;
	type Nonce = u64;
	type Signature = MultiSignature;

	fn max_extrinsic_size() -> u32 {
		0
	}
	fn max_extrinsic_weight() -> Weight {
		Weight::zero()
	}
}

impl Parachain for BigParachain {
	const PARACHAIN_ID: u32 = 4;
}

construct_runtime! {
	pub enum TestRuntime
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Grandpa1: pallet_bridge_grandpa::<Instance1>::{Pallet, Event<T>},
		Grandpa2: pallet_bridge_grandpa::<Instance2>::{Pallet, Event<T>},
		Parachains: pallet_bridge_parachains::{Call, Pallet, Event<T>},
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
}

parameter_types! {
	pub const HeadersToKeep: u32 = 5;
}

impl pallet_bridge_grandpa::Config<pallet_bridge_grandpa::Instance1> for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = TestBridgedChain;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<2>;
	type HeadersToKeep = HeadersToKeep;
	type WeightInfo = ();
}

impl pallet_bridge_grandpa::Config<pallet_bridge_grandpa::Instance2> for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = TestBridgedChain;
	type MaxFreeMandatoryHeadersPerBlock = ConstU32<2>;
	type HeadersToKeep = HeadersToKeep;
	type WeightInfo = ();
}

parameter_types! {
	pub const HeadsToKeep: u32 = 4;
	pub const ParasPalletName: &'static str = PARAS_PALLET_NAME;
	pub GetTenFirstParachains: Vec<ParaId> = (0..10).map(ParaId).collect();
}

impl pallet_bridge_parachains::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type BridgesGrandpaPalletInstance = pallet_bridge_grandpa::Instance1;
	type ParasPalletName = ParasPalletName;
	type ParaStoredHeaderDataBuilder = (Parachain1, Parachain2, Parachain3, BigParachain);
	type HeadsToKeep = HeadsToKeep;
	type MaxParaHeadDataSize = ConstU32<MAXIMAL_PARACHAIN_HEAD_DATA_SIZE>;
}

#[cfg(feature = "runtime-benchmarks")]
impl pallet_bridge_parachains::benchmarking::Config<()> for TestRuntime {
	fn parachains() -> Vec<ParaId> {
		vec![
			ParaId(Parachain1::PARACHAIN_ID),
			ParaId(Parachain2::PARACHAIN_ID),
			ParaId(Parachain3::PARACHAIN_ID),
		]
	}

	fn prepare_parachain_heads_proof(
		parachains: &[ParaId],
		_parachain_head_size: u32,
		_proof_size: bp_runtime::StorageProofSize,
	) -> (
		crate::RelayBlockNumber,
		crate::RelayBlockHash,
		bp_polkadot_core::parachains::ParaHeadsProof,
		Vec<(ParaId, bp_polkadot_core::parachains::ParaHash)>,
	) {
		// in mock run we only care about benchmarks correctness, not the benchmark results
		// => ignore size related arguments
		let (state_root, proof, parachains) =
			bp_test_utils::prepare_parachain_heads_proof::<RegularParachainHeader>(
				parachains.iter().map(|p| (p.0, crate::tests::head_data(p.0, 1))).collect(),
			);
		let relay_genesis_hash = crate::tests::initialize(state_root);
		(0, relay_genesis_hash, proof, parachains)
	}
}

#[derive(Debug)]
pub struct TestBridgedChain;

impl Chain for TestBridgedChain {
	const ID: ChainId = *b"tbch";

	type BlockNumber = crate::RelayBlockNumber;
	type Hash = crate::RelayBlockHash;
	type Hasher = crate::RelayBlockHasher;
	type Header = RelayBlockHeader;

	type AccountId = AccountId;
	type Balance = u32;
	type Nonce = u32;
	type Signature = sp_runtime::testing::TestSignature;

	fn max_extrinsic_size() -> u32 {
		unreachable!()
	}

	fn max_extrinsic_weight() -> Weight {
		unreachable!()
	}
}

impl ChainWithGrandpa for TestBridgedChain {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = "";
	const MAX_AUTHORITIES_COUNT: u32 = 16;
	const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 = 8;
	const MAX_MANDATORY_HEADER_SIZE: u32 = 256;
	const AVERAGE_HEADER_SIZE: u32 = 64;
}

#[derive(Debug)]
pub struct OtherBridgedChain;

impl Chain for OtherBridgedChain {
	const ID: ChainId = *b"obch";

	type BlockNumber = u64;
	type Hash = crate::RelayBlockHash;
	type Hasher = crate::RelayBlockHasher;
	type Header = sp_runtime::generic::Header<u64, crate::RelayBlockHasher>;

	type AccountId = AccountId;
	type Balance = u32;
	type Nonce = u32;
	type Signature = sp_runtime::testing::TestSignature;

	fn max_extrinsic_size() -> u32 {
		unreachable!()
	}

	fn max_extrinsic_weight() -> Weight {
		unreachable!()
	}
}

impl ChainWithGrandpa for OtherBridgedChain {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = "";
	const MAX_AUTHORITIES_COUNT: u32 = 16;
	const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 = 8;
	const MAX_MANDATORY_HEADER_SIZE: u32 = 256;
	const AVERAGE_HEADER_SIZE: u32 = 64;
}

/// Return test externalities to use in tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_io::TestExternalities::new(Default::default())
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		System::reset_events();
		test()
	})
}

/// Return test relay chain header with given number.
pub fn test_relay_header(
	num: crate::RelayBlockNumber,
	state_root: crate::RelayBlockHash,
) -> RelayBlockHeader {
	RelayBlockHeader::new(
		num,
		Default::default(),
		state_root,
		Default::default(),
		Default::default(),
	)
}
