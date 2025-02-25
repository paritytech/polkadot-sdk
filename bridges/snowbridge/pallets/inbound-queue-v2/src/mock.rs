// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use crate::{self as inbound_queue_v2};
use codec::Encode;
use frame_support::{
	derive_impl, parameter_types,
	traits::ConstU32,
	weights::{constants::RocksDbWeight, IdentityFee},
};
use hex_literal::hex;
use snowbridge_beacon_primitives::{
	types::deneb, BeaconHeader, ExecutionProof, Fork, ForkVersions, VersionedExecutionPayloadHeader,
};
use snowbridge_core::TokenId;
use snowbridge_inbound_queue_primitives::{v2::MessageToXcm, Log, Proof, VerificationError};
use sp_core::H160;
use sp_runtime::{
	traits::{IdentifyAccount, IdentityLookup, MaybeEquivalence, Verify},
	BuildStorage, MultiSignature,
};
use sp_std::{convert::From, default::Default, marker::PhantomData};
use xcm::{latest::SendXcm, opaque::latest::WESTEND_GENESIS_HASH, prelude::*};
type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system::{Pallet, Call, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		EthereumBeaconClient: snowbridge_pallet_ethereum_client::{Pallet, Call, Storage, Event<T>},
		InboundQueue: inbound_queue_v2::{Pallet, Call, Storage, Event<T>},
	}
);

pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

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

parameter_types! {
	pub const ChainForkVersions: ForkVersions = ForkVersions {
		genesis: Fork {
			version: [0, 0, 0, 1], // 0x00000001
			epoch: 0,
		},
		altair: Fork {
			version: [1, 0, 0, 1], // 0x01000001
			epoch: 0,
		},
		bellatrix: Fork {
			version: [2, 0, 0, 1], // 0x02000001
			epoch: 0,
		},
		capella: Fork {
			version: [3, 0, 0, 1], // 0x03000001
			epoch: 0,
		},
		deneb: Fork {
			version: [4, 0, 0, 1], // 0x04000001
			epoch: 0,
		},
		electra: Fork {
			version: [5, 0, 0, 0], // 0x05000000
			epoch: 80000000000,
		}
	};
}

impl snowbridge_pallet_ethereum_client::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ForkVersions = ChainForkVersions;
	type FreeHeadersInterval = ConstU32<32>;
	type WeightInfo = ();
}

// Mock verifier
pub struct MockVerifier;

impl Verifier for MockVerifier {
	fn verify(log: &Log, _: &Proof) -> Result<(), VerificationError> {
		if log.address == hex!("0000000000000000000000000000000000000911").into() {
			return Err(VerificationError::InvalidProof)
		}
		Ok(())
	}
}

const GATEWAY_ADDRESS: [u8; 20] = hex!["b8ea8cb425d85536b158d661da1ef0895bb92f1d"];

#[cfg(feature = "runtime-benchmarks")]
impl<T: snowbridge_pallet_ethereum_client::Config> BenchmarkHelper<T> for Test {
	// not implemented since the MockVerifier is used for tests
	fn initialize_storage(_: BeaconHeader, _: H256) {}
}

// Mock XCM sender that always succeeds
pub struct MockXcmSender;
impl SendXcm for MockXcmSender {
	type Ticket = Xcm<()>;

	fn validate(
		dest: &mut Option<Location>,
		xcm: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		if let Some(location) = dest {
			match location.unpack() {
				(_, [Parachain(1001)]) => return Err(SendError::NotApplicable),
				_ => Ok((xcm.clone().unwrap(), Assets::default())),
			}
		} else {
			Ok((xcm.clone().unwrap(), Assets::default()))
		}
	}

	fn deliver(xcm: Self::Ticket) -> core::result::Result<XcmHash, SendError> {
		let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
		Ok(hash)
	}
}

pub enum Weightless {}
impl PreparedMessage for Weightless {
	fn weight_of(&self) -> Weight {
		unreachable!();
	}
}

pub struct MockXcmExecutor;
impl<C> ExecuteXcm<C> for MockXcmExecutor {
	type Prepared = Weightless;
	fn prepare(message: Xcm<C>) -> Result<Self::Prepared, Xcm<C>> {
		Err(message)
	}
	fn execute(_: impl Into<Location>, _: Self::Prepared, _: &mut XcmHash, _: Weight) -> Outcome {
		unreachable!()
	}
	fn charge_fees(_: impl Into<Location>, _: Assets) -> xcm::latest::Result {
		Ok(())
	}
}

pub struct MockTokenIdConvert;
impl MaybeEquivalence<TokenId, Location> for MockTokenIdConvert {
	fn convert(_id: &TokenId) -> Option<Location> {
		Some(Location::parent())
	}
	fn convert_back(_loc: &Location) -> Option<TokenId> {
		None
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
	pub const EthereumNetwork: xcm::v5::NetworkId = xcm::v5::NetworkId::Ethereum { chain_id: 11155111 };
	pub const GatewayAddress: H160 = H160(GATEWAY_ADDRESS);
	pub InboundQueueLocation: InteriorLocation = [PalletInstance(84)].into();
	pub AssetHubLocation: InteriorLocation = Parachain(1000).into();
	pub UniversalLocation: InteriorLocation =
		[GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH)), Parachain(1002)].into();
	pub AssetHubFromEthereum: Location = Location::new(1,[GlobalConsensus(ByGenesis(WESTEND_GENESIS_HASH)),Parachain(1000)]);
	pub const InitialFund: u128 = 1_000_000_000_000;
}

impl inbound_queue_v2::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Verifier = MockVerifier;
	type XcmSender = MockXcmSender;
	type XcmExecutor = MockXcmExecutor;
	type RewardPayment = ();
	type EthereumNetwork = EthereumNetwork;
	type GatewayAddress = GatewayAddress;
	type AssetHubParaId = ConstU32<1000>;
	type MessageConverter = MessageToXcm<
		EthereumNetwork,
		InboundQueueLocation,
		MockTokenIdConvert,
		GatewayAddress,
		UniversalLocation,
		AssetHubFromEthereum,
	>;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = Test;
	type Balance = u128;
	type WeightInfo = ();
	type WeightToFee = IdentityFee<u128>;
	type Token = Balances;
	type AccountToLocation = MockAccountLocationConverter<AccountId>;
}

pub fn setup() {
	System::set_block_number(1);
}

pub fn new_tester() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext: sp_io::TestExternalities = storage.into();
	ext.execute_with(setup);
	ext
}

// Generated from smoketests:
//   cd smoketests
//   ./make-bindings
//   cargo test --test register_token -- --nocapture
pub fn mock_event_log() -> Log {
	Log {
        // gateway address
        address: hex!("b8ea8cb425d85536b158d661da1ef0895bb92f1d").into(),
        topics: vec![
            hex!("b61699d45635baed7500944331ea827538a50dbfef79180f2079e9185da627aa").into(),
        ],
        // Nonce + Payload
        data: hex!("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000b8ea8cb425d85536b158d661da1ef0895bb92f1d00000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000160000000000000000000000000000000000000000000000000000000001dcd6500000000000000000000000000000000000000000000000000000000003b9aca000000000000000000000000000000000000000000000000000000000059682f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002cdeadbeef774667629726ec1fabebcec0d9139bd1c8f72a23deadbeef0000000000000000000000001dcd650000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").into(),
    }
}

pub fn mock_event_log_invalid_gateway() -> Log {
	Log {
        // gateway address
        address: H160::zero(),
        topics: vec![
            hex!("b61699d45635baed7500944331ea827538a50dbfef79180f2079e9185da627aa").into(),
        ],
        // Nonce + Payload
        data: hex!("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000b8ea8cb425d85536b158d661da1ef0895bb92f1d00000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000160000000000000000000000000000000000000000000000000000000001dcd6500000000000000000000000000000000000000000000000000000000003b9aca000000000000000000000000000000000000000000000000000000000059682f000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002cdeadbeef774667629726ec1fabebcec0d9139bd1c8f72a23deadbeef0000000000000000000000001dcd650000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").into(),
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

// For backwards compatibility and tests
impl WeightInfo for () {
	fn submit() -> Weight {
		Weight::from_parts(70_000_000, 0)
			.saturating_add(Weight::from_parts(0, 3601))
			.saturating_add(RocksDbWeight::get().reads(2))
			.saturating_add(RocksDbWeight::get().writes(2))
	}
}

pub mod mock_xcm_send_failure {
	use super::*;

	frame_support::construct_runtime!(
		pub enum TestXcmSendFailure
		{
			System: frame_system::{Pallet, Call, Storage, Event<T>},
			Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
			EthereumBeaconClient: snowbridge_pallet_ethereum_client::{Pallet, Call, Storage, Event<T>},
			InboundQueue: inbound_queue_v2::{Pallet, Call, Storage, Event<T>},
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for TestXcmSendFailure {
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type AccountData = pallet_balances::AccountData<u128>;
		type Block = Block;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for TestXcmSendFailure {
		type Balance = Balance;
		type ExistentialDeposit = ExistentialDeposit;
		type AccountStore = System;
	}

	impl inbound_queue_v2::Config for TestXcmSendFailure {
		type RuntimeEvent = RuntimeEvent;
		type Verifier = MockVerifier;
		type XcmSender = MockXcmFailureSender;
		type XcmExecutor = MockXcmExecutor;
		type RewardPayment = ();
		type EthereumNetwork = EthereumNetwork;
		type GatewayAddress = GatewayAddress;
		type AssetHubParaId = ConstU32<1000>;
		type MessageConverter = MessageToXcm<
			EthereumNetwork,
			InboundQueueLocation,
			MockTokenIdConvert,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>;
		#[cfg(feature = "runtime-benchmarks")]
		type Helper = Test;
		type Balance = u128;
		type WeightInfo = ();
		type WeightToFee = IdentityFee<u128>;
		type Token = Balances;
		type AccountToLocation = MockAccountLocationConverter<AccountId>;
	}

	impl snowbridge_pallet_ethereum_client::Config for TestXcmSendFailure {
		type RuntimeEvent = RuntimeEvent;
		type ForkVersions = ChainForkVersions;
		type FreeHeadersInterval = ConstU32<32>;
		type WeightInfo = ();
	}

	pub struct MockXcmFailureSender;
	impl SendXcm for MockXcmFailureSender {
		type Ticket = Xcm<()>;

		fn validate(
			dest: &mut Option<Location>,
			xcm: &mut Option<Xcm<()>>,
		) -> SendResult<Self::Ticket> {
			if let Some(location) = dest {
				match location.unpack() {
					(_, [Parachain(1001)]) => return Err(SendError::NotApplicable),
					_ => Ok((xcm.clone().unwrap(), Assets::default())),
				}
			} else {
				Ok((xcm.clone().unwrap(), Assets::default()))
			}
		}

		fn deliver(_xcm: Self::Ticket) -> core::result::Result<XcmHash, SendError> {
			return Err(SendError::DestinationUnsupported)
		}
	}

	pub fn new_tester() -> sp_io::TestExternalities {
		let storage = frame_system::GenesisConfig::<TestXcmSendFailure>::default()
			.build_storage()
			.unwrap();
		let mut ext: sp_io::TestExternalities = storage.into();
		ext.execute_with(setup);
		ext
	}
}

pub mod mock_xcm_validate_failure {
	use super::*;

	#[cfg(feature = "runtime-benchmarks")]
	impl<T: snowbridge_pallet_ethereum_client::Config> BenchmarkHelper<T> for Test {
		// not implemented since the MockVerifier is used for tests
		fn initialize_storage(_: BeaconHeader, _: H256) {}
	}

	frame_support::construct_runtime!(
		pub enum Test
		{
			System: frame_system::{Pallet, Call, Storage, Event<T>},
			Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
			EthereumBeaconClient: snowbridge_pallet_ethereum_client::{Pallet, Call, Storage, Event<T>},
			InboundQueue: inbound_queue_v2::{Pallet, Call, Storage, Event<T>},
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type AccountData = pallet_balances::AccountData<u128>;
		type Block = Block;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for Test {
		type Balance = Balance;
		type ExistentialDeposit = ExistentialDeposit;
		type AccountStore = System;
	}

	impl inbound_queue_v2::Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type Verifier = MockVerifier;
		type XcmSender = MockXcmFailureValidate;
		type XcmExecutor = MockXcmExecutor;
		type RewardPayment = ();
		type EthereumNetwork = EthereumNetwork;
		type GatewayAddress = GatewayAddress;
		type AssetHubParaId = ConstU32<1000>;
		type MessageConverter = MessageToXcm<
			EthereumNetwork,
			InboundQueueLocation,
			MockTokenIdConvert,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>;
		#[cfg(feature = "runtime-benchmarks")]
		type Helper = Test;
		type Balance = u128;
		type WeightInfo = ();
		type WeightToFee = IdentityFee<u128>;
		type Token = Balances;
		type AccountToLocation = MockAccountLocationConverter<AccountId>;
	}

	impl snowbridge_pallet_ethereum_client::Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type ForkVersions = ChainForkVersions;
		type FreeHeadersInterval = ConstU32<32>;
		type WeightInfo = ();
	}

	pub struct MockXcmFailureValidate;
	impl SendXcm for MockXcmFailureValidate {
		type Ticket = Xcm<()>;

		fn validate(
			_dest: &mut Option<Location>,
			_xcm: &mut Option<Xcm<()>>,
		) -> SendResult<Self::Ticket> {
			return Err(SendError::NotApplicable)
		}

		fn deliver(xcm: Self::Ticket) -> core::result::Result<XcmHash, SendError> {
			let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
			Ok(hash)
		}
	}

	pub fn new_tester() -> sp_io::TestExternalities {
		let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let mut ext: sp_io::TestExternalities = storage.into();
		ext.execute_with(setup);
		ext
	}
}

pub mod mock_charge_fees_failure {
	use super::*;

	#[cfg(feature = "runtime-benchmarks")]
	impl<T: snowbridge_pallet_ethereum_client::Config> BenchmarkHelper<T> for Test {
		// not implemented since the MockVerifier is used for tests
		fn initialize_storage(_: BeaconHeader, _: H256) {}
	}

	frame_support::construct_runtime!(
		pub enum Test
		{
			System: frame_system::{Pallet, Call, Storage, Event<T>},
			Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
			EthereumBeaconClient: snowbridge_pallet_ethereum_client::{Pallet, Call, Storage, Event<T>},
			InboundQueue: inbound_queue_v2::{Pallet, Call, Storage, Event<T>},
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type AccountData = pallet_balances::AccountData<u128>;
		type Block = Block;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for Test {
		type Balance = Balance;
		type ExistentialDeposit = ExistentialDeposit;
		type AccountStore = System;
	}

	impl inbound_queue_v2::Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type Verifier = MockVerifier;
		type XcmSender = MockXcmSender;
		type XcmExecutor = MockXcmChargeFeesFailure;
		type RewardPayment = ();
		type EthereumNetwork = EthereumNetwork;
		type GatewayAddress = GatewayAddress;
		type AssetHubParaId = ConstU32<1000>;
		type MessageConverter = MessageToXcm<
			EthereumNetwork,
			InboundQueueLocation,
			MockTokenIdConvert,
			GatewayAddress,
			UniversalLocation,
			AssetHubFromEthereum,
		>;
		#[cfg(feature = "runtime-benchmarks")]
		type Helper = Test;
		type Balance = u128;
		type WeightInfo = ();
		type WeightToFee = IdentityFee<u128>;
		type Token = Balances;
		type AccountToLocation = MockAccountLocationConverter<AccountId>;
	}

	impl snowbridge_pallet_ethereum_client::Config for Test {
		type RuntimeEvent = RuntimeEvent;
		type ForkVersions = ChainForkVersions;
		type FreeHeadersInterval = ConstU32<32>;
		type WeightInfo = ();
	}

	pub struct MockXcmChargeFeesFailure;
	impl<C> ExecuteXcm<C> for MockXcmChargeFeesFailure {
		type Prepared = Weightless;
		fn prepare(message: Xcm<C>) -> Result<Self::Prepared, Xcm<C>> {
			Err(message)
		}
		fn execute(
			_: impl Into<Location>,
			_: Self::Prepared,
			_: &mut XcmHash,
			_: Weight,
		) -> Outcome {
			unreachable!()
		}
		fn charge_fees(_: impl Into<Location>, _: Assets) -> xcm::latest::Result {
			Err(XcmError::Barrier)
		}
	}

	pub fn new_tester() -> sp_io::TestExternalities {
		let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		let mut ext: sp_io::TestExternalities = storage.into();
		ext.execute_with(setup);
		ext
	}
}
