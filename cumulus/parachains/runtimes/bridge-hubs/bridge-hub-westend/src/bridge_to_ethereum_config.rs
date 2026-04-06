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
	xcm_config::{
		MaxAssetsIntoHolding, MaxInstructions, RelayNetwork, RootLocation, TreasuryAccount,
		UniversalLocation, XcmConfig, XcmOriginToTransactDispatchOrigin,
	},
	Balances, BridgeRelayers, EthereumBeaconClient, EthereumInboundQueue, EthereumInboundQueueV2,
	EthereumOutboundQueue, EthereumOutboundQueueV2, EthereumSystem, EthereumSystemV2, MessageQueue,
	PolkadotXcm, Runtime, RuntimeCall, RuntimeEvent, TransactionByteFee,
};
use alloc::boxed::Box;
use bp_asset_hub_westend::CreateForeignAssetDeposit;
use bridge_hub_common::AggregateMessageOrigin;
use frame_support::{
	ensure, parameter_types,
	traits::{
		tokens::imbalance::{
			ImbalanceAccounting, UnsafeConstructorDestructor, UnsafeManualAccounting,
		},
		Contains, Equals, Everything, EverythingBut, Nothing, ProcessMessageError,
	},
	weights::{ConstantMultiplier, Weight},
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
	v2::{
		snowbridge_v2_instructions_contain_alias_origin, snowbridge_v2_outbound_xcm_shape,
		ConstantGasMeter as ConstantGasMeterV2, EthereumBlobExporter as EthereumBlobExporterV2,
		ExecuteBeforeSnowbridgeV2BlobExport,
	},
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
use xcm::{
	latest::{Error as XcmError, Fungibility},
	prelude::{
		GlobalConsensus, Instruction, InteriorLocation, Location, PalletInstance, Parachain,
	},
};
use xcm_builder::{AliasOriginRootUsingFilter, FrameTransactionalProcessor, WeightInfoBounds};
use xcm_executor::{
	traits::{Properties, ShouldExecute, TransactAsset, WaiveDeliveryFees},
	XcmExecutor,
};

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
>;

/// Minimal [`ImbalanceAccounting`] for [`AssetsInHolding`](xcm_executor::AssetsInHolding) credits
/// used only by [`EthereumSimulationAssetTransactor`] (no real balances).
struct SimulationFungibleCredit(u128);

impl UnsafeConstructorDestructor<u128> for SimulationFungibleCredit {
	fn unsafe_clone(&self) -> Box<dyn ImbalanceAccounting<u128>> {
		Box::new(Self(self.0))
	}
	fn forget_imbalance(&mut self) -> u128 {
		let amt = self.0;
		self.0 = 0;
		amt
	}
}

impl UnsafeManualAccounting<u128> for SimulationFungibleCredit {
	fn saturating_subsume(&mut self, mut other: Box<dyn ImbalanceAccounting<u128>>) {
		self.0 = self.0.saturating_add(other.forget_imbalance());
	}
}

impl ImbalanceAccounting<u128> for SimulationFungibleCredit {
	fn amount(&self) -> u128 {
		self.0
	}
	fn saturating_take(&mut self, amount: u128) -> Box<dyn ImbalanceAccounting<u128>> {
		let taken = self.0.min(amount);
		self.0 -= taken;
		Box::new(Self(taken))
	}
}

fn simulation_asset_to_holding(asset: xcm::latest::Asset) -> xcm_executor::AssetsInHolding {
	let mut holding = xcm_executor::AssetsInHolding::new();
	match asset.fun {
		Fungibility::Fungible(amount) => {
			holding.fungible.insert(asset.id, Box::new(SimulationFungibleCredit(amount)));
		},
		Fungibility::NonFungible(instance) => {
			holding.non_fungible.insert((asset.id, instance));
		},
	}
	holding
}

/// Snowbridge v2 Ethereum destinations use an `AccountKey20` junction for the beneficiary address.
/// Match that here so invalid shapes fail [`TransactAsset::deposit_asset`] and simulated XCM ends
/// with assets still in holding (then [`EthereumXcmConfig::AssetTrap`] records them).
fn ethereum_simulation_deposit_beneficiary_valid(who: &Location) -> bool {
	matches!(who.last(), Some(xcm::latest::Junction::AccountKey20 { .. }))
}

/// Asset transactor for Snowbridge Ethereum **export simulation** on Bridge Hub.
///
/// Withdraw / mint / internal transfer are bookkeeping-only (no real balances). [`DepositAsset`]
/// only succeeds when the beneficiary ends with
/// [`AccountKey20`](xcm::latest::Junction::AccountKey20), matching Snowbridge’s Ethereum-facing
/// shape; otherwise assets stay in holding for trapping.
pub struct EthereumSimulationAssetTransactor;
impl TransactAsset for EthereumSimulationAssetTransactor {
	fn withdraw_asset(
		what: &xcm::latest::Asset,
		_who: &Location,
		_context: Option<&xcm::latest::XcmContext>,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		Ok(simulation_asset_to_holding(what.clone()))
	}

	fn deposit_asset(
		what: xcm_executor::AssetsInHolding,
		who: &Location,
		_context: Option<&xcm::latest::XcmContext>,
	) -> Result<(), (xcm_executor::AssetsInHolding, XcmError)> {
		if !ethereum_simulation_deposit_beneficiary_valid(who) {
			return Err((
				what,
				XcmError::FailedToTransactAsset(
					"Ethereum DepositAsset beneficiary must end with AccountKey20",
				),
			));
		}
		drop(what);
		Ok(())
	}

	fn mint_asset(
		what: &xcm::latest::Asset,
		_context: &xcm::latest::XcmContext,
	) -> Result<xcm_executor::AssetsInHolding, XcmError> {
		Ok(simulation_asset_to_holding(what.clone()))
	}

	fn internal_transfer_asset(
		what: &xcm::latest::Asset,
		_from: &Location,
		_to: &Location,
		_context: &xcm::latest::XcmContext,
	) -> Result<xcm::latest::Asset, XcmError> {
		Ok(what.clone())
	}
}

/// [`ShouldExecute`] barrier for [`EthereumXcmExecutor`] when
/// [`ExecuteBeforeSnowbridgeV2BlobExport`] simulates the **inner** Ethereum `ExportMessage` blob.
/// That blob does not use `UnpaidExecution`; fee-free simulation is provided separately by
/// [`EthereumExecutionFreeTrader`] and [`WaiveDeliveryFees`] on [`EthereumXcmConfig`].
///
/// Snowbridge v2 outbound blobs are recognized by an [`AliasOrigin`] instruction (same as
/// [`snowbridge_outbound_queue_primitives::v2::EthereumBlobExporter`]). Those programs must match
/// [`snowbridge_v2_outbound_xcm_shape`]. Legacy Snowbridge v1 exports (e.g. `ReserveAssetDeposited`
/// + `BuyExecution` + `DepositAsset` from `transfer_assets`) contain no `AliasOrigin`; they skip
///   the
/// strict shape here while the v1
/// [`snowbridge_outbound_queue_primitives::v1::EthereumBlobExporter`] still validates on enqueue.
pub struct EthereumExportSimulationBarrier;
impl ShouldExecute for EthereumExportSimulationBarrier {
	fn should_execute<RuntimeCall>(
		origin: &Location,
		instructions: &mut [Instruction<RuntimeCall>],
		_max_weight: Weight,
		_properties: &mut Properties,
	) -> Result<(), ProcessMessageError> {
		ensure!(Everything::contains(origin), ProcessMessageError::Unsupported);
		if snowbridge_v2_instructions_contain_alias_origin(instructions) {
			snowbridge_v2_outbound_xcm_shape(instructions, EthereumNetwork::get())
		} else {
			Err(ProcessMessageError::Unsupported)
		}
	}
}

/// Buys execution weight without touching real fee balances (paired with [`WaiveDeliveryFees`]).
#[derive(Clone)]
pub struct EthereumExecutionFreeTrader;
impl xcm_executor::traits::WeightTrader for EthereumExecutionFreeTrader {
	fn new() -> Self {
		Self
	}

	fn buy_weight(
		&mut self,
		_weight: Weight,
		_payment: xcm_executor::AssetsInHolding,
		_context: &xcm::latest::XcmContext,
	) -> Result<xcm_executor::AssetsInHolding, (xcm_executor::AssetsInHolding, XcmError)> {
		Ok(xcm_executor::AssetsInHolding::new())
	}
}

/// XCM executor config used only to **simulate** Snowbridge Ethereum outbound messages on Bridge
/// Hub (v2 and legacy v1 blob shapes).
///
/// [`EthereumSimulationAssetTransactor`] does not move real assets; [`DepositAsset`] succeeds
/// only when the beneficiary [`Location`] ends with
/// [`AccountKey20`](xcm::latest::Junction::AccountKey20), matching Ethereum destinations.
/// [`MessageExporter`](EthereumSimulationSuccessExporter) stubs nested `ExportMessage` so execution
/// can reach `Outcome::Complete` without a second real enqueue.
///
/// `Aliasers` allows `AliasOrigin` only when the current origin is the Asset Hub root
/// ([`AssetHubLocation`]), matching Snowbridge v2 `preserve_origin` traffic. The alias target must
/// be any [`Location`] except that same Asset Hub root (see [`EverythingBut`] + [`Equals`]).
pub struct EthereumXcmConfig;
impl xcm_executor::Config for EthereumXcmConfig {
	type RuntimeCall = RuntimeCall;
	type XcmSender = ();
	type XcmEventEmitter = PolkadotXcm;
	type AssetTransactor = EthereumSimulationAssetTransactor;
	type OriginConverter = XcmOriginToTransactDispatchOrigin;
	type IsReserve = Everything;
	type IsTeleporter = ();
	type Aliasers = AliasOriginRootUsingFilter<
		AssetHubLocation,
		EverythingBut<Equals<AssetHubLocation>>,
	>;
	type UniversalLocation = UniversalLocation;
	type Barrier = EthereumExportSimulationBarrier;
	type Weigher = WeightInfoBounds<
		crate::weights::xcm::BridgeHubWestendXcmWeight<RuntimeCall>,
		RuntimeCall,
		MaxInstructions,
	>;
	type Trader = EthereumExecutionFreeTrader;
	type ResponseHandler = ();
	type AssetTrap = PolkadotXcm;
	type AssetLocker = ();
	type AssetExchanger = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = WaiveDeliveryFees;
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = RuntimeCall;
	type SafeCallFilter = Nothing;
	type TransactionalProcessor = FrameTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = PolkadotXcm;
}

/// [`XcmExecutor`](xcm_executor::XcmExecutor) for [`EthereumXcmConfig`].
pub type EthereumXcmExecutor = XcmExecutor<EthereumXcmConfig>;

pub type SnowbridgeExporterV2WithXcmExecution = ExecuteBeforeSnowbridgeV2BlobExport<
	SnowbridgeExporterV2,
	EthereumXcmExecutor,
	UniversalLocation,
	EthereumNetwork,
	AssetHubParaId,
	RuntimeCall,
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
			(1, [Parachain(para_id), PalletInstance(index)]) => {
				return *para_id == ASSET_HUB_ID && *index == FRONTEND_PALLET_INDEX;
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
		EthereumBeaconClient, EthereumSystem, Runtime, RuntimeOrigin, System,
	};
	use codec::Encode;
	use frame_support::assert_ok;
	use hex_literal::hex;
	use snowbridge_beacon_primitives::BeaconHeader;
	use snowbridge_inbound_queue_primitives::{
		v2::{MessageToXcm, XcmMessageProcessor as InboundXcmMessageProcessor},
		EventFixture,
	};
	use snowbridge_pallet_inbound_queue::BenchmarkHelper;
	use snowbridge_pallet_inbound_queue_fixtures::register_token::make_register_token_message;
	use snowbridge_pallet_inbound_queue_v2::BenchmarkHelper as InboundQueueBenchmarkHelperV2;
	use snowbridge_pallet_inbound_queue_v2_fixtures::register_token::make_register_token_message as make_register_token_message_v2;
	use snowbridge_pallet_outbound_queue_v2::BenchmarkHelper as OutboundQueueBenchmarkHelperV2;
	use sp_core::H256;
	use testnet_parachains_constants::westend::snowbridge::{AssetHubParaId, EthereumNetwork};
	use xcm::latest::{Assets, Location, SendError, SendResult, SendXcm, Xcm, XcmHash};
	use xcm_executor::XcmExecutor;

	impl<T: snowbridge_pallet_ethereum_client::Config> BenchmarkHelper<T> for Runtime {
		fn initialize_storage() -> EventFixture {
			let message = make_register_token_message();
			EthereumBeaconClient::store_finalized_header(
				message.finalized_header,
				message.block_roots_root,
			)
			.unwrap();
			System::set_storage(
				RuntimeOrigin::root(),
				vec![(
					EthereumGatewayAddress::key().to_vec(),
					hex!("EDa338E4dC46038493b885327842fD3E301CaB39").to_vec(),
				)],
			)
			.unwrap();
			message
		}
	}

	impl<T: snowbridge_pallet_inbound_queue_v2::Config> InboundQueueBenchmarkHelperV2<T> for Runtime {
		fn initialize_storage() -> EventFixture {
			let message = make_register_token_message_v2();

			assert_ok!(EthereumBeaconClient::store_finalized_header(
				message.finalized_header,
				message.block_roots_root,
			));

			message
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
	use xcm::{
		latest::ExecuteXcm,
		prelude::{AccountKey20, Asset, AssetId, Fungible, WithdrawAsset, Xcm},
	};
	use xcm_config::FungibleTransactor;
	use xcm_executor::traits::TransactAsset;

	use super::{EthereumSimulationAssetTransactor, EthereumXcmExecutor, RuntimeCall, Weight};

	#[test]
	fn ethereum_simulation_transactor_accepts_account_key20_withdraw() {
		let eth = Asset {
			id: AssetId(Location::new(0, [AccountKey20 { network: None, key: [1u8; 20] }])),
			fun: Fungible(1u128),
		};
		let origin = Location::here();
		let ctx = xcm::latest::XcmContext::with_message_id([0u8; 32]);
		assert!(
			EthereumSimulationAssetTransactor::withdraw_asset(&eth, &origin, Some(&ctx)).is_ok()
		);
		assert!(FungibleTransactor::withdraw_asset(&eth, &origin, Some(&ctx)).is_err());
	}

	#[test]
	fn ethereum_xcm_executor_prepare_succeeds_for_ethereum_shaped_withdraw() {
		let eth = Asset {
			id: AssetId(Location::new(0, [AccountKey20 { network: None, key: [2u8; 20] }])),
			fun: Fungible(1u128),
		};
		let msg: Xcm<RuntimeCall> = Xcm(vec![WithdrawAsset(eth.into())]);
		assert!(EthereumXcmExecutor::prepare(msg, Weight::MAX).is_ok());
	}

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
