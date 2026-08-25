// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
	bridge_common_config::BridgeReward,
	xcm_config,
	xcm_config::{AccumulateAccount, RelayNetwork, RootLocation, UniversalLocation, XcmConfig},
	Balances, BridgeRelayers, EthereumBeaconClient, EthereumInboundQueue, EthereumInboundQueueV2,
	EthereumOutboundQueue, EthereumOutboundQueueV2, EthereumSystem, EthereumSystemV2, MessageQueue,
	Runtime, RuntimeEvent, TransactionByteFee,
};
use bp_asset_hub_westend::CreateForeignAssetDeposit;
use bridge_hub_common::AggregateMessageOrigin;
use frame_support::{
	parameter_types,
	traits::{Contains, Equals, EverythingBut},
	weights::ConstantMultiplier,
};
use frame_system::EnsureRootWithSuccess;
use hex_literal::hex;
use pallet_xcm::EnsureXcm;
use parachains_common::{AccountId, Balance};
use snowbridge_beacon_primitives::{Fork, ForkVersions};
use snowbridge_core::{gwei, meth, AllowSiblingsOnly, PricingParameters, Rewards};
use snowbridge_inbound_queue_primitives::v2::{
	CreateAssetCallInfo, MessageToXcm, XcmMessageProcessor as InboundXcmMessageProcessor,
};
use snowbridge_outbound_queue_primitives::{
	v1::{ConstantGasMeter, EthereumBlobExporter},
	v2::{ConstantGasMeter as ConstantGasMeterV2, EthereumBlobExporter as EthereumBlobExporterV2},
};
use sp_core::H160;
use sp_runtime::{
	traits::{ConstU32, ConstU8, Keccak256},
	FixedU128,
};
use testnet_parachains_constants::westend::{
	currency::*,
	fee::WeightToFee,
	locations::AssetHubLocation,
	snowbridge::{
		AssetHubParaId, EthereumLocation, EthereumNetwork, FRONTEND_PALLET_INDEX,
		INBOUND_QUEUE_PALLET_INDEX_V1, INBOUND_QUEUE_PALLET_INDEX_V2,
	},
};
use westend_runtime_constants::system_parachain::ASSET_HUB_ID;
use xcm::prelude::{GlobalConsensus, InteriorLocation, Location, PalletInstance, Parachain};
use xcm_executor::XcmExecutor;

pub const SLOTS_PER_EPOCH: u32 = snowbridge_pallet_ethereum_client::config::SLOTS_PER_EPOCH as u32;

/// Exports message to the Ethereum Gateway contract.
pub type SnowbridgeExporter = EthereumBlobExporter<
	UniversalLocation,
	EthereumNetwork,
	snowbridge_pallet_outbound_queue::Pallet<Runtime>,
	snowbridge_core::AgentIdOf,
	EthereumSystem,
>;

pub type SnowbridgeExporterV2 = EthereumBlobExporterV2<
	UniversalLocation,
	EthereumNetwork,
	EthereumOutboundQueueV2,
	EthereumSystemV2,
	AssetHubParaId,
	EverythingBut<Equals<AssetHubLocation>>,
>;

// Ethereum Bridge
parameter_types! {
	pub storage EthereumGatewayAddress: H160 = H160(hex!("b1185ede04202fe62d38f5db72f71e38ff3e8305"));
}

parameter_types! {
	pub const CreateAssetCallIndex: [u8;2] = [53, 0];
	pub const SetReservesCallIndex: [u8;2] = [53, 33];
	pub Parameters: PricingParameters<u128> = PricingParameters {
		exchange_rate: FixedU128::from_rational(1, 400),
		fee_per_gas: gwei(20),
		rewards: Rewards { local: 1 * UNITS, remote: meth(1) },
		multiplier: FixedU128::from_rational(1, 1),
	};
	pub AssetHubFromEthereum: Location = Location::new(1, [GlobalConsensus(RelayNetwork::get()), Parachain(ASSET_HUB_ID)]);
	pub EthereumUniversalLocation: InteriorLocation = [GlobalConsensus(EthereumNetwork::get())].into();
	pub AssetHubUniversalLocation: InteriorLocation = [GlobalConsensus(RelayNetwork::get()), Parachain(ASSET_HUB_ID)].into();
	pub InboundQueueV2Location: InteriorLocation = [PalletInstance(INBOUND_QUEUE_PALLET_INDEX_V2)].into();
	pub const SnowbridgeReward: BridgeReward = BridgeReward::Snowbridge;
	pub CreateAssetCall: CreateAssetCallInfo = CreateAssetCallInfo {
		create_call: CreateAssetCallIndex::get(),
		deposit: CreateForeignAssetDeposit::get(),
		min_balance:1,
		set_reserves_call: SetReservesCallIndex::get(),
	};
	pub SnowbridgeFrontendLocation: Location = Location::new(1, [Parachain(ASSET_HUB_ID), PalletInstance(FRONTEND_PALLET_INDEX)]);
	pub TargetLocation: Location = Location::new(1, [Parachain(AssetHubParaId::get().into())]);
}

impl snowbridge_pallet_inbound_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Verifier = snowbridge_pallet_ethereum_client::Pallet<Runtime>;
	type Token = Balances;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type XcmSender = crate::XcmRouter;
	#[cfg(feature = "runtime-benchmarks")]
	type XcmSender = benchmark_helpers::DoNothingRouter;
	type ChannelLookup = EthereumSystem;
	type GatewayAddress = EthereumGatewayAddress;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = Runtime;
	type MessageConverter = snowbridge_inbound_queue_primitives::v1::MessageToXcm<
		CreateAssetCallIndex,
		CreateForeignAssetDeposit,
		ConstU8<INBOUND_QUEUE_PALLET_INDEX_V1>,
		AccountId,
		Balance,
		EthereumSystem,
		EthereumUniversalLocation,
		AssetHubFromEthereum,
	>;
	type WeightToFee = WeightToFee;
	type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
	type MaxMessageSize = ConstU32<2048>;
	type WeightInfo = crate::weights::snowbridge_pallet_inbound_queue::WeightInfo<Runtime>;
	type PricingParameters = EthereumSystem;
	type AssetTransactor = <xcm_config::XcmConfig as xcm_executor::Config>::AssetTransactor;
	type MaxProofNodes = ConstU32<16>;
	type MaxReceiptBytes = ConstU32<8192>;
}

pub type XcmMessageProcessor = InboundXcmMessageProcessor<
	Runtime,
	crate::XcmRouter,
	XcmExecutor<XcmConfig>,
	MessageToXcm<
		CreateAssetCall,
		EthereumNetwork,
		RelayNetwork,
		EthereumGatewayAddress,
		InboundQueueV2Location,
		AssetHubParaId,
		EthereumSystem,
		AccountId,
	>,
	xcm_builder::AliasesIntoAccountId32<
		xcm_config::RelayNetwork,
		<Runtime as frame_system::Config>::AccountId,
	>,
	TargetLocation,
>;

impl snowbridge_pallet_inbound_queue_v2::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Verifier = EthereumBeaconClient;
	type GatewayAddress = EthereumGatewayAddress;
	type WeightInfo = crate::weights::snowbridge_pallet_inbound_queue_v2::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type MessageProcessor = benchmark_helpers::DummyXcmProcessor;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type MessageProcessor = XcmMessageProcessor;
	type RewardKind = BridgeReward;
	type DefaultRewardKind = SnowbridgeReward;
	type RewardPayment = BridgeRelayers;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = Runtime;
	type MaxProofNodes = ConstU32<16>;
	type MaxReceiptBytes = ConstU32<8192>;
}

impl snowbridge_pallet_outbound_queue::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Hashing = Keccak256;
	type MessageQueue = MessageQueue;
	type Decimals = ConstU8<12>;
	type MaxMessagePayloadSize = ConstU32<2048>;
	type MaxMessagesPerBlock = ConstU32<32>;
	type GasMeter = ConstantGasMeter;
	type Balance = Balance;
	type WeightToFee = WeightToFee;
	type WeightInfo = crate::weights::snowbridge_pallet_outbound_queue::WeightInfo<Runtime>;
	type PricingParameters = EthereumSystem;
	type Channels = EthereumSystem;
}

impl snowbridge_pallet_outbound_queue_v2::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Hashing = Keccak256;
	type MessageQueue = MessageQueue;
	// Maximum payload size for outbound messages.
	type MaxMessagePayloadSize = ConstU32<2048>;
	// Maximum number of outbound messages that can be committed per block.
	// It's benchmarked, including the entire process flow(initialize,submit,commit) in the
	// worst-case, Benchmark results in `../weights/snowbridge_pallet_outbound_queue_v2.
	// rs` show that the `process` function consumes less than 1% of the block capacity, which is
	// safe enough.
	type MaxMessagesPerBlock = ConstU32<32>;
	type GasMeter = ConstantGasMeterV2;
	type Balance = Balance;
	type WeightToFee = WeightToFee;
	type Verifier = EthereumBeaconClient;
	type GatewayAddress = EthereumGatewayAddress;
	type WeightInfo = crate::weights::snowbridge_pallet_outbound_queue_v2::WeightInfo<Runtime>;
	type EthereumNetwork = EthereumNetwork;
	type RewardKind = BridgeReward;
	type DefaultRewardKind = SnowbridgeReward;
	type RewardPayment = BridgeRelayers;
	type AggregateMessageOrigin = AggregateMessageOrigin;
	type OnNewCommitment = ();
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = Runtime;
	type MaxProofNodes = ConstU32<16>;
	type MaxReceiptBytes = ConstU32<8192>;
}

#[cfg(not(any(feature = "std", feature = "fast-runtime", feature = "runtime-benchmarks", test)))]
parameter_types! {
	pub const ChainForkVersions: ForkVersions = ForkVersions {
		genesis: Fork {
			version: hex!("90000069"),
			epoch: 0,
		},
		altair: Fork {
			version: hex!("90000070"),
			epoch: 50,
		},
		bellatrix: Fork {
			version: hex!("90000071"),
			epoch: 100,
		},
		capella: Fork {
			version: hex!("90000072"),
			epoch: 56832,
		},
		deneb: Fork {
			version: hex!("90000073"),
			epoch: 132608,
		},
		electra: Fork {
			version: hex!("90000074"),
			epoch: 222464,
		},
		fulu: Fork {
			version: hex!("90000075"),
			epoch: 272640, // https://notes.ethereum.org/@bbusa/fusaka-bpo-timeline
		},
	};
}

#[cfg(any(feature = "std", feature = "fast-runtime", feature = "runtime-benchmarks", test))]
parameter_types! {
	pub const ChainForkVersions: ForkVersions = ForkVersions {
		genesis: Fork {
			version: hex!("00000000"),
			epoch: 0,
		},
		altair: Fork {
			version: hex!("01000000"),
			epoch: 0,
		},
		bellatrix: Fork {
			version: hex!("02000000"),
			epoch: 0,
		},
		capella: Fork {
			version: hex!("03000000"),
			epoch: 0,
		},
		deneb: Fork {
			version: hex!("04000000"),
			epoch: 0,
		},
		electra: Fork {
			version: hex!("05000000"),
			epoch: 0,
		},
		fulu: Fork {
			version: hex!("06000000"),
			epoch: 5000000,
		}
	};
}

impl snowbridge_pallet_ethereum_client::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type ForkVersions = ChainForkVersions;
	type FreeHeadersInterval = ConstU32<SLOTS_PER_EPOCH>;
	type WeightInfo = crate::weights::snowbridge_pallet_ethereum_client::WeightInfo<Runtime>;
}

impl snowbridge_pallet_system::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OutboundQueue = EthereumOutboundQueue;
	type SiblingOrigin = EnsureXcm<AllowSiblingsOnly>;
	type AgentIdOf = snowbridge_core::AgentIdOf;
	type TreasuryAccount = AccumulateAccount;
	type Token = Balances;
	type WeightInfo = crate::weights::snowbridge_pallet_system::WeightInfo<Runtime>;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
	type DefaultPricingParameters = Parameters;
	type InboundDeliveryCost = EthereumInboundQueue;
	type UniversalLocation = UniversalLocation;
	type EthereumLocation = EthereumLocation;
}

pub struct AllowFromEthereumFrontend;
impl Contains<Location> for AllowFromEthereumFrontend {
	fn contains(location: &Location) -> bool {
		match location.unpack() {
			(1, [Parachain(para_id), PalletInstance(index)]) => {
				return *para_id == ASSET_HUB_ID && *index == FRONTEND_PALLET_INDEX
			},
			_ => false,
		}
	}
}

impl snowbridge_pallet_system_v2::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OutboundQueue = EthereumOutboundQueueV2;
	type InboundQueue = EthereumInboundQueueV2;
	type FrontendOrigin = EnsureXcm<AllowFromEthereumFrontend>;
	type WeightInfo = crate::weights::snowbridge_pallet_system_v2::WeightInfo<Runtime>;
	type GovernanceOrigin = EnsureRootWithSuccess<crate::AccountId, RootLocation>;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
}

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmark_helpers {
	use crate::{
		bridge_to_ethereum_config::{
			CreateAssetCall, EthereumGatewayAddress, InboundQueueV2Location, TargetLocation,
		},
		vec,
		xcm_config::{RelayNetwork, XcmConfig},
		EthereumSystem, Runtime, RuntimeOrigin, System,
	};
	use codec::Encode;
	use hex_literal::hex;
	use snowbridge_beacon_primitives::CompactBeaconState;
	use snowbridge_inbound_queue_primitives::{
		v2::{MessageToXcm, XcmMessageProcessor as InboundXcmMessageProcessor},
		EventFixture,
	};
	use snowbridge_pallet_ethereum_client::{FinalizedBeaconState, LatestFinalizedBlockRoot};
	use snowbridge_pallet_inbound_queue::BenchmarkHelper;
	use snowbridge_pallet_inbound_queue_fixtures::dynamic::{
		build_dynamic_fixture, DynamicFixture,
	};
	use snowbridge_pallet_inbound_queue_v2::BenchmarkHelper as InboundQueueBenchmarkHelperV2;
	use snowbridge_pallet_outbound_queue_v2::BenchmarkHelper as OutboundQueueBenchmarkHelperV2;
	use testnet_parachains_constants::westend::snowbridge::{AssetHubParaId, EthereumNetwork};
	use xcm::latest::{Assets, Location, SendError, SendResult, SendXcm, Xcm, XcmHash};
	use xcm_executor::XcmExecutor;

	impl<T: snowbridge_pallet_ethereum_client::Config> BenchmarkHelper<T> for Runtime {
		fn initialize_storage(n: u32, s: u32) -> EventFixture {
			let DynamicFixture { event_fixture, finalized_block_root } =
				build_dynamic_fixture(n, s);
			// Bypass `store_finalized_header` (which would re-hash the header and key the
			// CompactBeaconState by that hash). The dynamic fixture computes its own
			// `block_roots_root` against a deterministic `finalized_block_root`, so we
			// inject the matching state directly.
			FinalizedBeaconState::<Runtime>::insert(
				finalized_block_root,
				CompactBeaconState {
					slot: event_fixture.event.proof.execution_proof.header.slot + 1,
					block_roots_root: event_fixture.block_roots_root,
				},
			);
			LatestFinalizedBlockRoot::<Runtime>::set(finalized_block_root);
			System::set_storage(
				RuntimeOrigin::root(),
				vec![(
					EthereumGatewayAddress::key().to_vec(),
					hex!("EDa338E4dC46038493b885327842fD3E301CaB39").to_vec(),
				)],
			)
			.unwrap();
			// Register the synthetic channel id used by the dynamic fixture so that
			// `EthereumSystem::ChannelLookup` resolves to AssetHub during `submit`.
			snowbridge_pallet_system::Channels::<Runtime>::insert(
				snowbridge_pallet_inbound_queue_fixtures::dynamic::CHANNEL_ID_AS_CHANNEL_ID,
				snowbridge_core::Channel {
					agent_id: sp_core::H256::zero(),
					para_id: AssetHubParaId::get(),
				},
			);
			event_fixture
		}
	}

	impl<T: snowbridge_pallet_inbound_queue_v2::Config> InboundQueueBenchmarkHelperV2<T> for Runtime {
		fn initialize_storage(n: u32, s: u32) -> EventFixture {
			let DynamicFixture { event_fixture, finalized_block_root } =
				snowbridge_pallet_inbound_queue_v2_fixtures::dynamic::build_dynamic_fixture(n, s);
			// Inject CompactBeaconState directly so the dynamic fixture's deterministic
			// `finalized_block_root` matches what the verifier looks up.
			FinalizedBeaconState::<Runtime>::insert(
				finalized_block_root,
				CompactBeaconState {
					slot: event_fixture.event.proof.execution_proof.header.slot + 1,
					block_roots_root: event_fixture.block_roots_root,
				},
			);
			LatestFinalizedBlockRoot::<Runtime>::set(finalized_block_root);
			event_fixture
		}
	}

	impl<T: snowbridge_pallet_outbound_queue_v2::Config> OutboundQueueBenchmarkHelperV2<T> for Runtime {
		fn initialize_storage(n: u32, s: u32) -> EventFixture {
			let DynamicFixture { event_fixture, finalized_block_root } =
				snowbridge_pallet_outbound_queue_v2::dynamic_fixture::build_dynamic_fixture(n, s);
			// Mirror the inbound v2 helper: inject the FinalizedBeaconState directly so the
			// dynamic fixture's deterministic `finalized_block_root` matches what the
			// verifier looks up.
			FinalizedBeaconState::<Runtime>::insert(
				finalized_block_root,
				CompactBeaconState {
					slot: event_fixture.event.proof.execution_proof.header.slot + 1,
					block_roots_root: event_fixture.block_roots_root,
				},
			);
			LatestFinalizedBlockRoot::<Runtime>::set(finalized_block_root);
			event_fixture
		}
	}

	pub struct DoNothingRouter;
	impl SendXcm for DoNothingRouter {
		type Ticket = Xcm<()>;

		fn validate(
			_dest: &mut Option<Location>,
			xcm: &mut Option<Xcm<()>>,
		) -> SendResult<Self::Ticket> {
			Ok((xcm.clone().unwrap(), Assets::new()))
		}
		fn deliver(xcm: Xcm<()>) -> Result<XcmHash, SendError> {
			let hash = xcm.using_encoded(sp_io::hashing::blake2_256);
			Ok(hash)
		}
	}

	pub type DummyXcmProcessor = InboundXcmMessageProcessor<
		Runtime,
		DoNothingRouter,
		XcmExecutor<XcmConfig>,
		MessageToXcm<
			CreateAssetCall,
			EthereumNetwork,
			RelayNetwork,
			EthereumGatewayAddress,
			InboundQueueV2Location,
			AssetHubParaId,
			EthereumSystem,
			<Runtime as frame_system::Config>::AccountId,
		>,
		xcm_builder::AliasesIntoAccountId32<
			RelayNetwork,
			<Runtime as frame_system::Config>::AccountId,
		>,
		TargetLocation,
	>;

	impl snowbridge_pallet_system::BenchmarkHelper<RuntimeOrigin> for () {
		fn make_xcm_origin(location: Location) -> RuntimeOrigin {
			RuntimeOrigin::from(pallet_xcm::Origin::Xcm(location))
		}
	}

	impl snowbridge_pallet_system_v2::BenchmarkHelper<RuntimeOrigin> for () {
		fn make_xcm_origin(location: Location) -> RuntimeOrigin {
			RuntimeOrigin::from(pallet_xcm::Origin::Xcm(location))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bridge_hub_inbound_queue_pallet_index_is_correct() {
		assert_eq!(
			INBOUND_QUEUE_PALLET_INDEX_V1,
			<EthereumInboundQueue as frame_support::traits::PalletInfoAccess>::index() as u8
		);
	}

	#[test]
	fn bridge_hub_inbound_v2_queue_pallet_index_is_correct() {
		assert_eq!(
			INBOUND_QUEUE_PALLET_INDEX_V2,
			<EthereumInboundQueueV2 as frame_support::traits::PalletInfoAccess>::index() as u8
		);
	}
}
