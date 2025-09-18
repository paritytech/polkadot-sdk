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
	xcm_config::{RelayNetwork, RootLocation, TreasuryAccount, UniversalLocation, XcmConfig},
	Balances, BridgeRelayers, EthereumInboundQueue, EthereumInboundQueueV2, EthereumOutboundQueue,
	EthereumOutboundQueueV2, EthereumSystem, EthereumSystemV2, MessageQueue, Runtime, RuntimeEvent,
	TransactionByteFee,
};
#[cfg(feature = "runtime-benchmarks")]
use benchmark_helpers::DoNothingRouter;
use bp_asset_hub_westend::CreateForeignAssetDeposit;
use bridge_hub_common::AggregateMessageOrigin;
use frame_support::{parameter_types, traits::Contains, weights::ConstantMultiplier};
use frame_system::EnsureRootWithSuccess;
use pallet_xcm::EnsureXcm;
use parachains_common::{AccountId, Balance};
use snowbridge_beacon_primitives::{Fork, ForkVersions};
use snowbridge_core::{gwei, meth, AllowSiblingsOnly, ChannelId, PricingParameters, Rewards};
use snowbridge_outbound_queue_primitives::{
	v1::{ConstantGasMeter, EthereumBlobExporter},
	v2::{ConstantGasMeter as ConstantGasMeterV2, EthereumBlobExporter as EthereumBlobExporterV2},
};
use snowbridge_pallet_inbound_queue::RewardThroughSovereign;
use sp_core::H160;
use sp_runtime::{
	traits::{ConstU32, ConstU8, Convert, Keccak256},
	FixedU128,
};
use testnet_parachains_constants::westend::{
	currency::*,
	fee::WeightToFee,
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
use hex_literal::hex;

pub type SnowbridgeExporterV2 = EthereumBlobExporterV2<
	UniversalLocation,
	EthereumNetwork,
	snowbridge_pallet_outbound_queue_v2::Pallet<Runtime>,
	EthereumSystemV2,
	AssetHubParaId,
>;

// Ethereum Bridge
parameter_types! {
	pub storage EthereumGatewayAddress: H160 = H160(hex!("b1185ede04202fe62d38f5db72f71e38ff3e8305"));
}

parameter_types! {
	pub const CreateAssetCall: [u8;2] = [53, 0];
	pub Parameters: PricingParameters<u128> = PricingParameters {
		exchange_rate: FixedU128::from_rational(1, 400),
		fee_per_gas: gwei(20),
		rewards: Rewards { local: 1 * UNITS, remote: meth(1) },
		multiplier: FixedU128::from_rational(1, 1),
	};
	pub AssetHubFromEthereum: Location = Location::new(1, [GlobalConsensus(RelayNetwork::get()), Parachain(ASSET_HUB_ID)]);
	pub AssetHubUniversalLocation: InteriorLocation = [GlobalConsensus(RelayNetwork::get()), Parachain(ASSET_HUB_ID)].into();
	pub AssetHubLocation: Location = Location::new(1, [Parachain(ASSET_HUB_ID)]);
	pub EthereumUniversalLocation: InteriorLocation = [GlobalConsensus(EthereumNetwork::get())].into();
	pub InboundQueueV2Location: InteriorLocation = [PalletInstance(INBOUND_QUEUE_PALLET_INDEX_V2)].into();
	pub SnowbridgeFrontendLocation: Location = Location::new(1, [Parachain(ASSET_HUB_ID), PalletInstance(FRONTEND_PALLET_INDEX)]);
	pub AssetHubXCMFee: u128 = 1_000_000_000_000u128;
	pub const SnowbridgeReward: BridgeReward = BridgeReward::Snowbridge;
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
		CreateAssetCall,
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
	type MessageProcessor =
		snowbridge_pallet_inbound_queue::xcm_message_processor::XcmMessageProcessor<Runtime>;
	type RewardProcessor = RewardThroughSovereign<Self>;
}

impl snowbridge_pallet_inbound_queue_v2::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Verifier = snowbridge_pallet_ethereum_client::Pallet<Runtime>;
	#[cfg(not(feature = "runtime-benchmarks"))]
	type XcmSender = crate::XcmRouter;
	#[cfg(feature = "runtime-benchmarks")]
	type XcmSender = benchmark_helpers::DoNothingRouter;
	type GatewayAddress = EthereumGatewayAddress;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = Runtime;
	type WeightInfo = crate::weights::snowbridge_pallet_inbound_queue_v2::WeightInfo<Runtime>;
	type AssetHubParaId = ConstU32<ASSET_HUB_ID>;
	type XcmExecutor = XcmExecutor<XcmConfig>;
	type MessageConverter = snowbridge_inbound_queue_primitives::v2::MessageToXcm<
		CreateAssetCall,
		CreateForeignAssetDeposit,
		EthereumNetwork,
		InboundQueueV2Location,
		EthereumSystem,
		EthereumGatewayAddress,
		EthereumUniversalLocation,
		AssetHubFromEthereum,
		AssetHubUniversalLocation,
		AccountId,
	>;
	type AccountToLocation = xcm_builder::AliasesIntoAccountId32<
		xcm_config::RelayNetwork,
		<Runtime as frame_system::Config>::AccountId,
	>;
	type RewardKind = BridgeReward;
	type DefaultRewardKind = SnowbridgeReward;
	type RewardPayment = BridgeRelayers;
}
pub struct GetAggregateMessageOrigin;

impl Convert<ChannelId, AggregateMessageOrigin> for GetAggregateMessageOrigin {
	fn convert(channel_id: ChannelId) -> AggregateMessageOrigin {
		AggregateMessageOrigin::Snowbridge(channel_id)
	}
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
	type AggregateMessageOrigin = AggregateMessageOrigin;
	type GetAggregateMessageOrigin = GetAggregateMessageOrigin;
	type OnNewCommitment = ();
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
	type Verifier = snowbridge_pallet_ethereum_client::Pallet<Runtime>;
	type GatewayAddress = EthereumGatewayAddress;
	type WeightInfo = crate::weights::snowbridge_pallet_outbound_queue_v2::WeightInfo<Runtime>;
	type EthereumNetwork = EthereumNetwork;
	type RewardKind = BridgeReward;
	type DefaultRewardKind = SnowbridgeReward;
	type RewardPayment = BridgeRelayers;
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = Runtime;
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
			epoch: 80000000000, // setting to a future epoch for local testing to remain on Deneb.
		},
	};
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
			epoch: 222464, // https://github.com/ethereum/EIPs/pull/9322/files
		},
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
	type TreasuryAccount = TreasuryAccount;
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
			(1, [Parachain(para_id), PalletInstance(index)]) =>
				return *para_id == ASSET_HUB_ID && *index == FRONTEND_PALLET_INDEX,
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
		bridge_to_ethereum_config::EthereumGatewayAddress, vec, EthereumBeaconClient, Runtime,
		RuntimeOrigin, System,
	};
	use codec::Encode;
	use hex_literal::hex;
	use snowbridge_beacon_primitives::BeaconHeader;
	use snowbridge_inbound_queue_primitives::EventFixture;
	use snowbridge_pallet_inbound_queue::BenchmarkHelper;
	use snowbridge_pallet_inbound_queue_v2::BenchmarkHelper as InboundQueueBenchmarkHelperV2;
	use snowbridge_pallet_outbound_queue_v2::BenchmarkHelper as OutboundQueueBenchmarkHelperV2;
	use sp_core::H256;
	use xcm::latest::{Assets, Location, SendError, SendResult, SendXcm, Xcm, XcmHash};

	impl<T: snowbridge_pallet_ethereum_client::Config> BenchmarkHelper<T> for Runtime {
		fn initialize_storage() -> EventFixture {
			/*
			EthereumBeaconClient::store_finalized_header(beacon_header, block_roots_root).unwrap();
			System::set_storage(
				RuntimeOrigin::root(),
				vec![(
					EthereumGatewayAddress::key().to_vec(),
					hex!("EDa338E4dC46038493b885327842fD3E301CaB39").to_vec(),
				)],
			)
			.unwrap();
			*/
			todo!()
		}
	}

	impl<T: snowbridge_pallet_inbound_queue_v2::Config> InboundQueueBenchmarkHelperV2<T> for Runtime {
		fn initialize_storage(beacon_header: BeaconHeader, block_roots_root: H256) {
			EthereumBeaconClient::store_finalized_header(beacon_header, block_roots_root).unwrap();
		}
	}

	impl<T: snowbridge_pallet_outbound_queue_v2::Config> OutboundQueueBenchmarkHelperV2<T> for Runtime {
		fn initialize_storage(beacon_header: BeaconHeader, block_roots_root: H256) {
			EthereumBeaconClient::store_finalized_header(beacon_header, block_roots_root).unwrap();
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

pub(crate) mod migrations {
	use frame_support::pallet_prelude::*;
	use snowbridge_core::TokenId;

	#[frame_support::storage_alias]
	pub type OldNativeToForeignId<T: snowbridge_pallet_system::Config> = StorageMap<
		snowbridge_pallet_system::Pallet<T>,
		Blake2_128Concat,
		xcm::v4::Location,
		TokenId,
		OptionQuery,
	>;

	/// One shot migration for NetworkId::Westend to NetworkId::ByGenesis(WESTEND_GENESIS_HASH)
	pub struct MigrationForXcmV5<T: snowbridge_pallet_system::Config>(core::marker::PhantomData<T>);
	impl<T: snowbridge_pallet_system::Config> frame_support::traits::OnRuntimeUpgrade
		for MigrationForXcmV5<T>
	{
		fn on_runtime_upgrade() -> Weight {
			let mut weight = T::DbWeight::get().reads(1);

			let translate_westend = |pre: xcm::v4::Location| -> Option<xcm::v5::Location> {
				weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));
				Some(xcm::v5::Location::try_from(pre).expect("valid location"))
			};
			snowbridge_pallet_system::ForeignToNativeId::<T>::translate_values(translate_westend);

			weight
		}
	}
}
