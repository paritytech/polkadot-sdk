// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use frame_support::{
	parameter_types,
	traits::{ConstU128, ConstU32, Everything},
	weights::IdentityFee,
};
use hex_literal::hex;
use snowbridge_beacon_primitives::{Fork, ForkVersions};
use snowbridge_core::{
	gwei,
	inbound::{Log, Proof, VerificationError},
	meth, Channel, ChannelId, PricingParameters, Rewards, StaticLookup,
};
use snowbridge_router_primitives::inbound::MessageToXcm;
use sp_core::{H160, H256};
use sp_runtime::{
	traits::{BlakeTwo256, IdentifyAccount, IdentityLookup, Verify},
	BuildStorage, FixedU128, MultiSignature,
};
use sp_std::convert::From;
use xcm::{latest::SendXcm, prelude::*};
use xcm_executor::AssetsInHolding;

use crate::{self as inbound_queue};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system::{Pallet, Call, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		EthereumBeaconClient: snowbridge_pallet_ethereum_client::{Pallet, Call, Storage, Event<T>},
		InboundQueue: inbound_queue::{Pallet, Call, Storage, Event<T>},
	}
);

pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

type Balance = u128;

impl frame_system::Config for Test {
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeTask = RuntimeTask;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u128>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type Nonce = u64;
	type Block = Block;
}

impl pallet_balances::Config for Test {
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeHoldReason = ();
	type RuntimeFreezeReason = ();
}

parameter_types! {
	pub const ExecutionHeadersPruneThreshold: u32 = 10;
	pub const ChainForkVersions: ForkVersions = ForkVersions{
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
			epoch: 4294967295,
		}
	};
}

impl snowbridge_pallet_ethereum_client::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type ForkVersions = ChainForkVersions;
	type MaxExecutionHeadersToKeep = ExecutionHeadersPruneThreshold;
	type WeightInfo = ();
}

// Mock verifier
pub struct MockVerifier;

impl Verifier for MockVerifier {
	fn verify(_: &Log, _: &Proof) -> Result<(), VerificationError> {
		Ok(())
	}
}

const GATEWAY_ADDRESS: [u8; 20] = hex!["eda338e4dc46038493b885327842fd3e301cab39"];

parameter_types! {
	pub const EthereumNetwork: xcm::v3::NetworkId = xcm::v3::NetworkId::Ethereum { chain_id: 11155111 };
	pub const GatewayAddress: H160 = H160(GATEWAY_ADDRESS);
	pub const CreateAssetCall: [u8;2] = [53, 0];
	pub const CreateAssetExecutionFee: u128 = 2_000_000_000;
	pub const CreateAssetDeposit: u128 = 100_000_000_000;
	pub const SendTokenExecutionFee: u128 = 1_000_000_000;
	pub const InitialFund: u128 = 1_000_000_000_000;
	pub const InboundQueuePalletInstance: u8 = 80;
}

#[cfg(feature = "runtime-benchmarks")]
impl<T: snowbridge_pallet_ethereum_client::Config> BenchmarkHelper<T> for Test {
	// not implemented since the MockVerifier is used for tests
	fn initialize_storage(_: H256, _: CompactExecutionHeader) {}
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
				(_, [Parachain(1001)]) => return Err(XcmpSendError::NotApplicable),
				_ => Ok((xcm.clone().unwrap(), Assets::default())),
			}
		} else {
			Ok((xcm.clone().unwrap(), Assets::default()))
		}
	}

	fn deliver(xcm: Self::Ticket) -> core::result::Result<XcmHash, XcmpSendError> {
		let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
		Ok(hash)
	}
}

parameter_types! {
	pub const OwnParaId: ParaId = ParaId::new(1013);
	pub Parameters: PricingParameters<u128> = PricingParameters {
		exchange_rate: FixedU128::from_rational(1, 400),
		fee_per_gas: gwei(20),
		rewards: Rewards { local: DOT, remote: meth(1) }
	};
}

pub const DOT: u128 = 10_000_000_000;

pub struct MockChannelLookup;
impl StaticLookup for MockChannelLookup {
	type Source = ChannelId;
	type Target = Channel;

	fn lookup(channel_id: Self::Source) -> Option<Self::Target> {
		if channel_id !=
			hex!("c173fac324158e77fb5840738a1a541f633cbec8884c6a601c567d2b376a0539").into()
		{
			return None
		}
		Some(Channel { agent_id: H256::zero(), para_id: ASSET_HUB_PARAID.into() })
	}
}

pub struct SuccessfulTransactor;
impl TransactAsset for SuccessfulTransactor {
	fn can_check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		Ok(())
	}

	fn can_check_out(_dest: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		Ok(())
	}

	fn deposit_asset(_what: &Asset, _who: &Location, _context: Option<&XcmContext>) -> XcmResult {
		Ok(())
	}

	fn withdraw_asset(
		_what: &Asset,
		_who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<AssetsInHolding, XcmError> {
		Ok(AssetsInHolding::default())
	}

	fn internal_transfer_asset(
		_what: &Asset,
		_from: &Location,
		_to: &Location,
		_context: &XcmContext,
	) -> Result<AssetsInHolding, XcmError> {
		Ok(AssetsInHolding::default())
	}
}

impl inbound_queue::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Verifier = MockVerifier;
	type Token = Balances;
	type XcmSender = MockXcmSender;
	type WeightInfo = ();
	type GatewayAddress = GatewayAddress;
	type MessageConverter = MessageToXcm<
		CreateAssetCall,
		CreateAssetDeposit,
		InboundQueuePalletInstance,
		AccountId,
		Balance,
	>;
	type PricingParameters = Parameters;
	type ChannelLookup = MockChannelLookup;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = Test;
	type WeightToFee = IdentityFee<u128>;
	type LengthToFee = IdentityFee<u128>;
	type MaxMessageSize = ConstU32<1024>;
	type AssetTransactor = SuccessfulTransactor;
}

pub fn last_events(n: usize) -> Vec<RuntimeEvent> {
	frame_system::Pallet::<Test>::events()
		.into_iter()
		.rev()
		.take(n)
		.rev()
		.map(|e| e.event)
		.collect()
}

pub fn expect_events(e: Vec<RuntimeEvent>) {
	assert_eq!(last_events(e.len()), e);
}

pub fn setup() {
	System::set_block_number(1);
	Balances::mint_into(
		&sibling_sovereign_account::<Test>(ASSET_HUB_PARAID.into()),
		InitialFund::get(),
	)
	.unwrap();
	Balances::mint_into(
		&sibling_sovereign_account::<Test>(TEMPLATE_PARAID.into()),
		InitialFund::get(),
	)
	.unwrap();
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
        address: hex!("eda338e4dc46038493b885327842fd3e301cab39").into(),
        topics: vec![
            hex!("7153f9357c8ea496bba60bf82e67143e27b64462b49041f8e689e1b05728f84f").into(),
            // channel id
            hex!("c173fac324158e77fb5840738a1a541f633cbec8884c6a601c567d2b376a0539").into(),
            // message id
            hex!("5f7060e971b0dc81e63f0aa41831091847d97c1a4693ac450cc128c7214e65e0").into(),
        ],
        // Nonce + Payload
        data: hex!("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000002e000f000000000000000087d1f7fdfee7f651fabc8bfcb6e086c278b77a7d00e40b54020000000000000000000000000000000000000000000000000000000000").into(),
    }
}

pub fn mock_event_log_invalid_channel() -> Log {
	Log {
        address: hex!("eda338e4dc46038493b885327842fd3e301cab39").into(),
        topics: vec![
            hex!("7153f9357c8ea496bba60bf82e67143e27b64462b49041f8e689e1b05728f84f").into(),
            // invalid channel id
            hex!("0000000000000000000000000000000000000000000000000000000000000000").into(),
            hex!("5f7060e971b0dc81e63f0aa41831091847d97c1a4693ac450cc128c7214e65e0").into(),
        ],
        data: hex!("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000001e000f000000000000000087d1f7fdfee7f651fabc8bfcb6e086c278b77a7d0000").into(),
    }
}

pub fn mock_event_log_invalid_gateway() -> Log {
	Log {
        // gateway address
        address: H160::zero(),
        topics: vec![
            hex!("7153f9357c8ea496bba60bf82e67143e27b64462b49041f8e689e1b05728f84f").into(),
            // channel id
            hex!("c173fac324158e77fb5840738a1a541f633cbec8884c6a601c567d2b376a0539").into(),
            // message id
            hex!("5f7060e971b0dc81e63f0aa41831091847d97c1a4693ac450cc128c7214e65e0").into(),
        ],
        // Nonce + Payload
        data: hex!("00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000001e000f000000000000000087d1f7fdfee7f651fabc8bfcb6e086c278b77a7d0000").into(),
    }
}

pub const ASSET_HUB_PARAID: u32 = 1000u32;
pub const TEMPLATE_PARAID: u32 = 1001u32;
