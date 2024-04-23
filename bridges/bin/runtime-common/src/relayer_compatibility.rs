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

//! An attempt to implement bridge versioning and make relayer (an offchain actor)
//! compatible with different runtime versions, implemeting the same bridge version.
//!
//! Before this versioning, we had to prepare + deploy a new relay version for every
//! runtime upgrade. And since relay is connected to 4 chains, we need to have a new
//! version every time one of chains is upgraded. Recently we have "eased" our
//! requirements and now only watch for bridge hub versions (2 chains).
//!
//! What we are trying to solve with that:
//!
//! - transaction encoding compatibility - when `spec_version`, `transaction_version`, our pallet
//!   calls encoding, pallet ordering or a set of signed extension changes, relay starts building
//!   invalid transactions. For first two things we have a CLI options to read versions right from
//!   runtime. The rest should happen rarely;
//!
//! - bridge configuration compatibility. E.g. relayer compensation/reward scheme may change over
//!   time. And relayer may everntually start losing its tokens by submitting valid and previously
//!   compensated transactions;
//!
//! - unexpected/unknown issues. E.g. : change of `paras` pallet at the relay chain, storage trie
//!   version changes, ... That is something that we expect needs to be detected by
//!   zombienet/chopsticks tests in the fellowhsip repo. But so far it isn't automated and normally
//!   when we are building new relay version for upgraded chains, we need to run Add P<>K bridge
//!   manual zombienet test for asset transfer polkadot-fellows/runtimes#198 and at least detect it
//!   manually.
//!
//! TLDR: by releasing a new relayer version on every runtime upgrade we were trying to ensure
//! that everything works properly. If we ever have an automated testing for that, we will be
//! able to solve that easier. Yet we have an issue, to make relayer fully generic, but it is
//! unlikely to be finished soon.
//!
//! What we can do to make our lives easier now and not to rebuild relayer on every upgrade.
//! Inspired by how pallet storage versioning works: we can add some constant to every bridge
//! pallet (GRANPA, parachains, messages) configuration and also add the same constant to relayer.
//! Relayer should exit if it sees that the constant it has is different from the constant in the
//! runtime. This should solve issues (1) and (2) from above. (3) is still actual and can't be
//! detected without having automated integration tests.
//!
//! This constant should 'seal' everything related to bridge transaction encoding and bridge
//! configuration compatibility:
//!
//! - a set of bridge calls encoding;
//!
//! - a set of bridge-related signed extensions IDs that are related to the bridge;
//!
//! - a set of all pallet settings that may affect relayer.

use crate::{
	messages::{
		source::FromBridgedChainMessagesDeliveryProof, target::FromBridgedChainMessagesProof,
	},
	messages_xcm_extension::XcmAsPlainPayload,
};
use bp_messages::{
	source_chain::TargetHeaderChain, target_chain::SourceHeaderChain, LaneId,
	UnrewardedRelayersState,
};
use bp_polkadot_core::parachains::ParaHeadsProof;
use bp_runtime::RelayerVersion;
use bp_test_utils::make_default_justification;
use codec::{Decode, Encode};
use frame_support::{
	traits::{GetStorageVersion, PalletInfoAccess},
	weights::Weight,
};
use pallet_bridge_grandpa::{
	BridgedHeader as BridgedGrandpaHeader, Call as GrandpaCall, Config as GrandpaConfig,
	Pallet as GrandpaPallet,
};
use pallet_bridge_messages::{
	Call as MessagesCall, Config as MessagesConfig, Pallet as MessagesPallet,
};
use pallet_bridge_parachains::{
	Call as ParachainsCall, Config as ParachainsConfig, Pallet as ParachainsPallet,
};
use sp_core::{blake2_256, Get, H256};
use sp_runtime::traits::{Header, SignedExtension, TrailingZeroInput};

const LOG_TARGET: &str = "bridge";

/// A set of signed extensions that are:
///
/// - not related to bridge operations;
///
/// - have the `AdditionalSigned` set to `()`, so it doesn't break the bridge relayer.
const IGNORED_EXTENSIONS: [&'static str; 0] = [];

/// Ensure that the running relayer is compatible with the `pallet-bridge-grandpa`, deployed
/// at `Runtime`.
pub fn ensure_grandpa_relayer_compatibility<Runtime, I, SignedExtra>()
where
	Runtime: GrandpaConfig<I>,
	I: 'static,
	GrandpaPallet<Runtime, I>: PalletInfoAccess,
	SignedExtra: SignedExtension,
	Runtime::RuntimeCall: From<GrandpaCall<Runtime, I>>,
{
	let expected_version = Runtime::CompatibleWithRelayer::get();
	let actual_version = RelayerVersion {
		manual: expected_version.manual,
		auto: blake2_256(
			&[
				pallet_storage_digest::<GrandpaPallet<Runtime, I>>(),
				grandpa_calls_digest::<Runtime, I>(),
				runtime_version_digest::<Runtime>(),
				siged_extensions_digest::<SignedExtra>(),
			]
			.encode(),
		)
		.into(),
	};
	assert_eq!(
		expected_version, actual_version,
		"Expected GRANDPA relayer version: {expected_version:?}. Actual: {actual_version:?}",
	);
}

/// Ensure that the running relayer is compatible with the `pallet-bridge-parachains`, deployed
/// at `Runtime`.
pub fn ensure_parachains_relayer_compatibility<Runtime, I, SignedExtra>()
where
	Runtime: ParachainsConfig<I>,
	I: 'static,
	ParachainsPallet<Runtime, I>: PalletInfoAccess,
	SignedExtra: SignedExtension,
	Runtime::RuntimeCall: From<ParachainsCall<Runtime, I>>,
{
	let expected_version = <Runtime as ParachainsConfig<I>>::CompatibleWithRelayer::get();
	let actual_version = RelayerVersion {
		manual: expected_version.manual,
		auto: blake2_256(
			&[
				pallet_storage_digest::<ParachainsPallet<Runtime, I>>(),
				parachains_calls_digest::<Runtime, I>(),
				runtime_version_digest::<Runtime>(),
				siged_extensions_digest::<SignedExtra>(),
			]
			.encode(),
		)
		.into(),
	};
	assert_eq!(
		expected_version, actual_version,
		"Expected parachains relayer version: {expected_version:?}. Actual: {actual_version:?}",
	);
}

/// Ensure that the running relayer is compatible with the `pallet-bridge-messages`, deployed
/// at `Runtime`.
pub fn ensure_messages_relayer_compatibility<
	Runtime,
	I,
	SignedExtra,
	BridgedHash,
	BridgedAccountId,
>()
where
	Runtime:
		MessagesConfig<I, InboundRelayer = BridgedAccountId, OutboundPayload = XcmAsPlainPayload>,
	I: 'static,
	MessagesPallet<Runtime, I>: PalletInfoAccess,
	SignedExtra: SignedExtension,
	Runtime::RuntimeCall: From<MessagesCall<Runtime, I>>,
	Runtime::SourceHeaderChain:
		SourceHeaderChain<MessagesProof = FromBridgedChainMessagesProof<BridgedHash>>,
	Runtime::TargetHeaderChain: TargetHeaderChain<
		XcmAsPlainPayload,
		Runtime::AccountId,
		MessagesDeliveryProof = FromBridgedChainMessagesDeliveryProof<BridgedHash>,
	>,
	BridgedHash: Default,
	BridgedAccountId: Decode,
{
	let expected_version = <Runtime as MessagesConfig<I>>::CompatibleWithRelayer::get();
	let actual_version = RelayerVersion {
		manual: expected_version.manual,
		auto: blake2_256(
			&[
				pallet_storage_digest::<MessagesPallet<Runtime, I>>(),
				messages_calls_digest::<Runtime, I, BridgedHash, BridgedAccountId>(),
				messages_config_digest::<Runtime, I>(),
				runtime_version_digest::<Runtime>(),
				siged_extensions_digest::<SignedExtra>(),
			]
			.encode(),
		)
		.into(),
	};
	assert_eq!(
		expected_version, actual_version,
		"Expected messages relayer version: {expected_version:?}. Actual: {actual_version:?}",
	);
}

/// Seal bridge pallet storage version.
fn pallet_storage_digest<P>() -> H256
where
	P: PalletInfoAccess + GetStorageVersion,
{
	// keys of storage entries, used by pallet are computed using:
	// 1) name of the pallet, used in `construct_runtime!` macro call;
	// 2) name of the storage value/map/doublemap itself.
	//
	// When the `1` from above is changed, `PalletInfoAccess::name()` shall change;
	// When the `2` from above is changed, `GetStorageVersion::in_code_storage_version()` shall
	// change.
	let pallet_name = P::name();
	let storage_version = P::on_chain_storage_version();
	log::info!(target: LOG_TARGET, "Sealing pallet storage: {:?}", (pallet_name, storage_version));
	blake2_256(&(pallet_name, storage_version).encode()).into()
}

/// Seal bridge GRANDPA call encoding.
fn grandpa_calls_digest<Runtime, I>() -> H256
where
	Runtime: GrandpaConfig<I>,
	I: 'static,
	Runtime::RuntimeCall: From<GrandpaCall<Runtime, I>>,
{
	// the relayer normally only uses the `submit_finality_proof` call. Let's ensure that
	// the encoding stays the same. Obviously, we can not detect all encoding changes here,
	// but such breaking changes are not assumed to be detected using this test.
	let bridged_header = BridgedGrandpaHeader::<Runtime, I>::new(
		42u32.into(),
		Default::default(),
		Default::default(),
		Default::default(),
		Default::default(),
	);
	let bridged_justification = make_default_justification(&bridged_header);
	let call: Runtime::RuntimeCall = GrandpaCall::<Runtime, I>::submit_finality_proof {
		finality_target: Box::new(bridged_header),
		justification: bridged_justification,
	}
	.into();
	log::info!(target: LOG_TARGET, "Sealing GRANDPA call encoding: {:?}", call);
	blake2_256(&call.encode()).into()
}

/// Seal bridge parachains call encoding.
fn parachains_calls_digest<Runtime, I>() -> H256
where
	Runtime: ParachainsConfig<I>,
	I: 'static,
	Runtime::RuntimeCall: From<ParachainsCall<Runtime, I>>,
{
	// the relayer normally only uses the `submit_parachain_heads` call. Let's ensure that
	// the encoding stays the same. Obviously, we can not detect all encoding changes here,
	// but such breaking changes are not assumed to be detected using this test.
	let call: Runtime::RuntimeCall = ParachainsCall::<Runtime, I>::submit_parachain_heads {
		at_relay_block: (84, Default::default()),
		parachains: vec![(42.into(), Default::default())],
		parachain_heads_proof: ParaHeadsProof { storage_proof: vec![vec![42u8; 42]] },
	}
	.into();
	log::info!(target: LOG_TARGET, "Sealing parachains call encoding: {:?}", call);
	blake2_256(&call.encode()).into()
}

/// Seal bridge messages call encoding.
fn messages_calls_digest<Runtime, I, BridgedHash, BridgedAccountId>() -> H256
where
	Runtime:
		MessagesConfig<I, InboundRelayer = BridgedAccountId, OutboundPayload = XcmAsPlainPayload>,
	I: 'static,
	Runtime::RuntimeCall: From<MessagesCall<Runtime, I>>,
	Runtime::SourceHeaderChain:
		SourceHeaderChain<MessagesProof = FromBridgedChainMessagesProof<BridgedHash>>,
	Runtime::TargetHeaderChain: TargetHeaderChain<
		XcmAsPlainPayload,
		Runtime::AccountId,
		MessagesDeliveryProof = FromBridgedChainMessagesDeliveryProof<BridgedHash>,
	>,
	BridgedHash: Default,
	BridgedAccountId: Decode,
{
	// the relayer normally only uses the `receive_messages_proof` and
	// `receive_messages_delivery_proof` calls. Let's ensure that the encoding stays the same.
	// Obviously, we can not detect all encoding changes here, but such breaking changes are
	// not assumed to be detected using this test.
	let call1: Runtime::RuntimeCall = MessagesCall::<Runtime, I>::receive_messages_proof {
		relayer_id_at_bridged_chain: BridgedAccountId::decode(&mut TrailingZeroInput::zeroes())
			.expect("is decoded successfully in tests; qed"),
		proof: FromBridgedChainMessagesProof {
			bridged_header_hash: BridgedHash::default(),
			storage_proof: vec![vec![42u8; 42]],
			lane: LaneId([0, 0, 0, 0]),
			nonces_start: 42,
			nonces_end: 42,
		},
		messages_count: 1,
		dispatch_weight: Weight::zero(),
	}
	.into();
	let call2: Runtime::RuntimeCall = MessagesCall::<Runtime, I>::receive_messages_delivery_proof {
		proof: FromBridgedChainMessagesDeliveryProof {
			bridged_header_hash: BridgedHash::default(),
			storage_proof: vec![vec![42u8; 42]],
			lane: LaneId([0, 0, 0, 0]),
		},
		relayers_state: UnrewardedRelayersState::default(),
	}
	.into();
	log::info!(target: LOG_TARGET, "Sealing message calls encoding: {:?} {:?}", call1, call2);
	blake2_256(&(call1, call2).encode()).into()
}

/// Seal bridge messages pallet configuration settings that may affect running relayer.
fn messages_config_digest<Runtime, I>() -> H256
where
	Runtime: MessagesConfig<I>,
	I: 'static,
{
	let settings = (
		Runtime::MaxUnrewardedRelayerEntriesAtInboundLane::get(),
		Runtime::MaxUnconfirmedMessagesAtInboundLane::get(),
	);
	log::info!(target: LOG_TARGET, "Sealing messages pallet configuration: {:?}", settings);
	blake2_256(&settings.encode()).into()
}

/// Seal runtime version. We want to make relayer tolerant towards `spec_version` and
/// `transcation_version` changes. But any changes to storage trie format are breaking,
/// not only for the relay, but for all involved pallets on other chains as well.
fn runtime_version_digest<Runtime: frame_system::Config>() -> H256 {
	let state_version = Runtime::Version::get().state_version;
	log::info!(target: LOG_TARGET, "Sealing runtime version: {:?}", state_version);
	blake2_256(&state_version.encode()).into()
}

/// Seal all signed extensions that may break bridge.
fn siged_extensions_digest<SignedExtra: SignedExtension>() -> H256 {
	let extensions: Vec<_> = SignedExtra::metadata()
		.into_iter()
		.map(|m| m.identifier)
		.filter(|id| !IGNORED_EXTENSIONS.contains(id))
		.collect();
	log::info!(target: LOG_TARGET, "Sealing runtime extensions: {:?}", extensions);
	blake2_256(&extensions.encode()).into()
}
