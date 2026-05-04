// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use crate::{self as inbound_queue_v2};
use frame_support::{derive_impl, parameter_types};
use hex_literal::hex;
use snowbridge_beacon_primitives::{
	types::deneb, BeaconHeader, ExecutionProof, Fork, ForkVersions, VersionedExecutionPayloadHeader,
};
use snowbridge_core::{ParaId, TokenId};
use snowbridge_inbound_queue_primitives::v2::{
	CreateAssetCallInfo, MessageProcessorError, MessageToXcm, XcmMessageProcessor,
};
use snowbridge_verification_primitives::{
	DefaultMaxDepth, DefaultMaxNodeSize, Log, Proof, Verifier,
};
use sp_core::{ConstU32, H160};
use sp_runtime::{
	traits::{IdentityLookup, MaybeConvert, TryConvert},
	BuildStorage,
};
use sp_std::{convert::From, default::Default, marker::PhantomData};
use xcm::{opaque::latest::WESTEND_GENESIS_HASH, prelude::*};
type Block = frame_system::mocking::MockBlock<Test>;
use snowbridge_test_utils::mock_rewards::{BridgeReward, MockRewardLedger};
pub use snowbridge_test_utils::mock_xcm::{MockXcmExecutor, MockXcmSender};

#[cfg(any(test, feature = "runtime-benchmarks"))]
use snowbridge_inbound_queue_primitives::EventFixture;
#[cfg(any(test, feature = "runtime-benchmarks"))]
use snowbridge_pallet_inbound_queue_v2_fixtures::register_token::{
	make_register_token_message, make_register_token_message_worst_case,
};

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system::{Pallet, Call, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		EthereumBeaconClient: snowbridge_pallet_ethereum_client::{Pallet, Call, Storage, Event<T>},
		InboundQueue: inbound_queue_v2::{Pallet, Call, Storage, Event<T>},
	}
);

pub(crate) const ERROR_ADDRESS: [u8; 20] = hex!("0000000000000000000000000000000000000911");

pub type AccountId = sp_runtime::AccountId32;
type Balance = u128;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountData = pallet_balances::AccountData<u128>;
	type Block = Block;
}

parameter_types! {
	pub const ExistentialDeposit: u128 = 1;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

// Mock verifier
pub struct MockVerifier;

impl Verifier for MockVerifier {
	type Proof = Proof;

	fn verify(log: &Log, _: &Proof) -> Result<(), VerificationError> {
		if log.address == ERROR_ADDRESS.into() {
			return Err(VerificationError::InvalidProof);
		}
		Ok(())
	}
}

parameter_types! {
	pub const ChainForkVersions: ForkVersions = ForkVersions {
		genesis: Fork {
			version: hex!("00000001"),
			epoch: 0,
		},
		altair: Fork {
			version: hex!("01000001"),
			epoch: 0,
		},
		bellatrix: Fork {
			version: hex!("02000001"),
			epoch: 0,
		},
		capella: Fork {
			version: hex!("03000001"),
			epoch: 0,
		},
		deneb: Fork {
			version: hex!("04000001"),
			epoch: 0,
		},
		electra: Fork {
			version: hex!("05000000"),
			epoch: 80000000000,
		},
		fulu: Fork {
			version: hex!("06000000"),
			epoch: 80000000001,
		}
	};
}

impl snowbridge_pallet_ethereum_client::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ForkVersions = ChainForkVersions;
	type FreeHeadersInterval = ConstU32<32>;
	type MaxReceiptProofDepth = DefaultMaxDepth;
	type MaxMptNodeSize = DefaultMaxNodeSize;
	type WeightInfo = ();
}

const GATEWAY_ADDRESS: [u8; 20] = hex!["b1185ede04202fe62d38f5db72f71e38ff3e8305"];

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper<Test> for Test {
	fn initialize_storage() -> EventFixture<Proof> {
		let fixture = make_register_token_message::<DefaultMaxNodeSize, DefaultMaxDepth>();
		EventFixture {
			event: snowbridge_inbound_queue_primitives::EventProof {
				event_log: fixture.event.event_log,
				proof: fixture.event.proof,
			},
			finalized_header: fixture.finalized_header,
			block_roots_root: fixture.block_roots_root,
		}
	}

	fn initialize_storage_worst_case_invalid_proof() -> EventFixture<Proof> {
		let fixture =
			make_register_token_message_worst_case::<DefaultMaxNodeSize, DefaultMaxDepth>();
		EventFixture {
			event: snowbridge_inbound_queue_primitives::EventProof {
				event_log: fixture.event.event_log,
				proof: fixture.event.proof,
			},
			finalized_header: fixture.finalized_header,
			block_roots_root: fixture.block_roots_root,
		}
	}
}

pub struct MockTokenIdConvert;
impl MaybeConvert<TokenId, Location> for MockTokenIdConvert {
	fn maybe_convert(_id: TokenId) -> Option<Location> {
		Some(Location::parent())
	}
}

pub struct MockAccountLocationConverter<AccountId>(PhantomData<AccountId>);
impl<'a, AccountId: Clone + Clone> TryConvert<&'a AccountId, Location>
	for MockAccountLocationConverter<AccountId>
{
	fn try_convert(_who: &AccountId) -> Result<Location, &AccountId> {
		Ok(Location::here())
	}
}

parameter_types! {
	pub const EthereumNetwork: NetworkId = Ethereum { chain_id: 11155111 };
	pub const GatewayAddress: H160 = H160(GATEWAY_ADDRESS);
	pub InboundQueueLocation: InteriorLocation = [PalletInstance(84)].into();
	pub SnowbridgeReward: BridgeReward = BridgeReward::Snowbridge;
	pub const CreateAssetCallIndex: [u8;2] = [53, 0];
	pub const SetReservesCallIndex: [u8;2] = [53, 33];
	pub const CreateAssetDeposit: u128 = 10_000_000_000u128;
	pub const LocalNetwork: NetworkId = ByGenesis(WESTEND_GENESIS_HASH);
	pub CreateAssetCall: CreateAssetCallInfo = CreateAssetCallInfo {
		create_call: CreateAssetCallIndex::get(),
		deposit: CreateAssetDeposit::get(),
		min_balance: 1,
		set_reserves_call: SetReservesCallIndex::get(),
	};
	pub AssetHubParaId: ParaId = ParaId::from(1000);
	pub TargetLocation: Location = Location::new(1, [Parachain(AssetHubParaId::get().into())]);
}

pub struct DummyPrefix;

impl MessageProcessor<AccountId> for DummyPrefix {
	fn can_process_message(_relayer: &AccountId, _message: &Message) -> bool {
		false
	}

	fn process_message(
		_relayer: AccountId,
		_message: Message,
	) -> Result<[u8; 32], MessageProcessorError> {
		panic!("DummyPrefix::process_message shouldn't be called");
	}
}

pub struct DummySuffix;

impl MessageProcessor<AccountId> for DummySuffix {
	fn can_process_message(_relayer: &AccountId, _message: &Message) -> bool {
		true
	}

	fn process_message(
		_relayer: AccountId,
		_message: Message,
	) -> Result<[u8; 32], MessageProcessorError> {
		panic!("DummySuffix::process_message shouldn't be called");
	}
}

impl inbound_queue_v2::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	#[cfg(feature = "runtime-benchmarks")]
	type Verifier = EthereumBeaconClient;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type Verifier = MockVerifier;
	type GatewayAddress = GatewayAddress;
	// Passively test that the implementation of MessageProcessor trait works correctly for tuple
	type MessageProcessor = (
		DummyPrefix,
		XcmMessageProcessor<
			Test,
			MockXcmSender,
			MockXcmExecutor,
			MessageToXcm<
				CreateAssetCall,
				EthereumNetwork,
				LocalNetwork,
				GatewayAddress,
				InboundQueueLocation,
				AssetHubParaId,
				MockTokenIdConvert,
				AccountId,
			>,
			MockAccountLocationConverter<AccountId>,
			TargetLocation,
		>,
		DummySuffix,
	);
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = Test;
	type WeightInfo = ();
	type RewardKind = BridgeReward;
	type DefaultRewardKind = SnowbridgeReward;
	type RewardPayment = MockRewardLedger;
}

pub fn setup() {
	System::set_block_number(1);
	#[cfg(feature = "runtime-benchmarks")]
	{
		let message = make_register_token_message::<
			<Test as snowbridge_pallet_ethereum_client::Config>::MaxMptNodeSize,
			<Test as snowbridge_pallet_ethereum_client::Config>::MaxReceiptProofDepth,
		>();
		EthereumBeaconClient::store_finalized_header(
			message.finalized_header,
			message.block_roots_root,
		)
		.unwrap();
	}
}

pub fn new_tester() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext: sp_io::TestExternalities = storage.into();
	ext.execute_with(setup);
	ext
}

/// Full EventProof for register_token (matches finalized header from setup).
#[cfg(any(test, feature = "runtime-benchmarks"))]
pub fn register_token_event_proof() -> snowbridge_inbound_queue_primitives::EventProof<Proof> {
	let fixture = make_register_token_message::<
		<Test as snowbridge_pallet_ethereum_client::Config>::MaxMptNodeSize,
		<Test as snowbridge_pallet_ethereum_client::Config>::MaxReceiptProofDepth,
	>();
	fixture.event
}

// Generated from smoketests:
//   cd smoketests
//   ./make-bindings
//   cargo test --test register_token -- --nocapture
pub fn mock_event_log() -> Log {
	Log {
        // gateway address
        address: hex!("b1185ede04202fe62d38f5db72f71e38ff3e8305").into(),
        topics: vec![
            hex!("550e2067494b1736ea5573f2d19cdc0ac95b410fff161bf16f11c6229655ec9c").into(),
        ],
        // Nonce + Payload
        data: hex!("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000b1185ede04202fe62d38f5db72f71e38ff3e830500000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000009184e72a0000000000000000000000000000000000000000000000000000000015d3ef798000000000000000000000000000000000000000000000000000000015d3ef798000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000040000000000000000000000000b8ea8cb425d85536b158d661da1ef0895bb92f1d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").into(),
        tx_index: 0,
    }
}

pub fn mock_event_log_invalid_gateway() -> Log {
	Log {
        // gateway address
        address: H160::zero(),
        topics: vec![
            hex!("550e2067494b1736ea5573f2d19cdc0ac95b410fff161bf16f11c6229655ec9c").into(),
        ],
        // Nonce + Payload
        data: hex!("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000b1185ede04202fe62d38f5db72f71e38ff3e830500000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000009184e72a0000000000000000000000000000000000000000000000000000000015d3ef798000000000000000000000000000000000000000000000000000000015d3ef798000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000040000000000000000000000000b8ea8cb425d85536b158d661da1ef0895bb92f1d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").into(),
        tx_index: 0,
    }
}

pub fn mock_event_log_invalid_message() -> Log {
	Log {
		// gateway address
		address: hex!("b8ea8cb425d85536b158d661da1ef0895bb92f1d").into(),
		topics: vec![
			hex!("b61699d45635baed7500944331ea827538a50dbfef79180f2079e9185da627aa").into(),
		],
		// Nonce + Payload
		data: hex!("000000000000000000000000000000000000000000000000000000b8ea8cb425d85536b158d661da1ef0895bb92f1d000000000000000000000000000000000000000000000000001dcd6500000000000000000000000000000000000000000000000000000000003b9aca000000000000000000000000000000000000000000000000000000000059682f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002cdeadbeef774667629726ec1fabebcec0d9139bd1c8f72a23deadbeef0000000000000000000000001dcd650000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").into(),
		tx_index: 0,
	}
}

pub fn mock_execution_proof() -> ExecutionProof {
	ExecutionProof {
		header: BeaconHeader::default(),
		ancestry_proof: None,
		execution_header: VersionedExecutionPayloadHeader::Deneb(deneb::ExecutionPayloadHeader {
			parent_hash: Default::default(),
			fee_recipient: Default::default(),
			state_root: Default::default(),
			receipts_root: Default::default(),
			logs_bloom: vec![],
			prev_randao: Default::default(),
			block_number: 0,
			gas_limit: 0,
			gas_used: 0,
			timestamp: 0,
			extra_data: vec![],
			base_fee_per_gas: Default::default(),
			block_hash: Default::default(),
			transactions_root: Default::default(),
			withdrawals_root: Default::default(),
			blob_gas_used: 0,
			excess_blob_gas: 0,
		}),
		execution_branch: vec![],
	}
}

// Generated from smoketests:
//   cd smoketests
//   ./make-bindings
//   cargo test --test register_token_v2 -- --nocapture
pub fn mock_event_log_v2() -> Log {
	Log {
        // gateway address
        address: hex!("b1185ede04202fe62d38f5db72f71e38ff3e8305").into(),
        topics: vec![
            hex!("550e2067494b1736ea5573f2d19cdc0ac95b410fff161bf16f11c6229655ec9c").into(),
        ],
        // Nonce + Payload
        data: hex!("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000b1185ede04202fe62d38f5db72f71e38ff3e830500000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000009184e72a0000000000000000000000000000000000000000000000000000000015d3ef798000000000000000000000000000000000000000000000000000000015d3ef798000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000040000000000000000000000000b8ea8cb425d85536b158d661da1ef0895bb92f1d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").into(),
        tx_index: 0,
    }
}

pub mod exploit {
	use super::*;

	use frame_support::traits::ConstU32;
	use hex_literal::hex;
	use snowbridge_beacon_primitives::{Fork, ForkVersions};

	type Block = frame_system::mocking::MockBlock<ExploitTest>;

	frame_support::construct_runtime!(
		pub enum ExploitTest
		{
			System: frame_system::{Pallet, Call, Storage, Event<T>},
			Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
			EthereumBeaconClient: snowbridge_pallet_ethereum_client::{Pallet, Call, Storage, Event<T>},
			InboundQueue: inbound_queue_v2::{Pallet, Call, Storage, Event<T>},
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for ExploitTest {
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type AccountData = pallet_balances::AccountData<u128>;
		type Block = Block;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for ExploitTest {
		type Balance = Balance;
		type ExistentialDeposit = ExistentialDeposit;
		type AccountStore = System;
	}

	parameter_types! {
		pub const ChainForkVersions: ForkVersions = ForkVersions {
			genesis: Fork { version: hex!("00000000"), epoch: 0 },
			altair: Fork { version: hex!("01000000"), epoch: 0 },
			bellatrix: Fork { version: hex!("02000000"), epoch: 0 },
			capella: Fork { version: hex!("03000000"), epoch: 0 },
			deneb: Fork { version: hex!("04000000"), epoch: 0 },
			electra: Fork { version: hex!("05000000"), epoch: 0 },
			fulu: Fork { version: hex!("06000000"), epoch: 100_000_000 },
		};
	}

	impl snowbridge_pallet_ethereum_client::Config for ExploitTest {
		type RuntimeEvent = RuntimeEvent;
		type ForkVersions = ChainForkVersions;
		type FreeHeadersInterval = ConstU32<32>;
		type MaxReceiptProofDepth = DefaultMaxDepth;
		type MaxMptNodeSize = DefaultMaxNodeSize;
		type WeightInfo = ();
	}

	impl inbound_queue_v2::Config for ExploitTest {
		type RuntimeEvent = RuntimeEvent;
		type Verifier = EthereumBeaconClient;
		type GatewayAddress = GatewayAddress;
		type MessageProcessor = (
			DummyPrefix,
			XcmMessageProcessor<
				ExploitTest,
				MockXcmSender,
				MockXcmExecutor,
				MessageToXcm<
					CreateAssetCall,
					EthereumNetwork,
					LocalNetwork,
					GatewayAddress,
					InboundQueueLocation,
					AssetHubParaId,
					MockTokenIdConvert,
					AccountId,
				>,
				MockAccountLocationConverter<AccountId>,
				TargetLocation,
			>,
			DummySuffix,
		);
		#[cfg(feature = "runtime-benchmarks")]
		type Helper = Test;
		type WeightInfo = ();
		type RewardKind = BridgeReward;
		type DefaultRewardKind = SnowbridgeReward;
		type RewardPayment = MockRewardLedger;
	}

	pub fn setup() {
		System::set_block_number(1);
	}

	pub fn new_tester() -> sp_io::TestExternalities {
		let storage =
			frame_system::GenesisConfig::<ExploitTest>::default().build_storage().unwrap();
		let mut ext: sp_io::TestExternalities = storage.into();
		ext.execute_with(setup);
		ext
	}
}
