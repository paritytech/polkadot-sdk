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

//! Integrity tests for chain constants and pallets configuration.
//!
//! Most of the tests in this module assume that the bridge is using standard (see `crate::messages`
//! module for details) configuration.

use crate::{messages, messages::MessageBridge};

use bp_messages::{InboundLaneData, MessageNonce};
use bp_runtime::{Chain, ChainId};
use codec::Encode;
use frame_support::{storage::generator::StorageValue, traits::Get, weights::Weight};
use frame_system::limits;
use pallet_bridge_messages::WeightInfoExt as _;

/// Macro that ensures that the runtime configuration and chain primitives crate are sharing
/// the same types (nonce, block number, hash, hasher, account id and header).
#[macro_export]
macro_rules! assert_chain_types(
	( runtime: $r:path, this_chain: $this:path ) => {
		{
			// if one of asserts fail, then either bridge isn't configured properly (or alternatively - non-standard
			// configuration is used), or something has broke existing configuration (meaning that all bridged chains
			// and relays will stop functioning)
			use frame_system::{Config as SystemConfig, pallet_prelude::{BlockNumberFor, HeaderFor}};
			use static_assertions::assert_type_eq_all;

			assert_type_eq_all!(<$r as SystemConfig>::Nonce, bp_runtime::NonceOf<$this>);
			assert_type_eq_all!(BlockNumberFor<$r>, bp_runtime::BlockNumberOf<$this>);
			assert_type_eq_all!(<$r as SystemConfig>::Hash, bp_runtime::HashOf<$this>);
			assert_type_eq_all!(<$r as SystemConfig>::Hashing, bp_runtime::HasherOf<$this>);
			assert_type_eq_all!(<$r as SystemConfig>::AccountId, bp_runtime::AccountIdOf<$this>);
			assert_type_eq_all!(HeaderFor<$r>, bp_runtime::HeaderOf<$this>);
		}
	}
);

/// Macro that ensures that the bridge GRANDPA pallet is configured properly to bridge with given
/// chain.
#[macro_export]
macro_rules! assert_bridge_grandpa_pallet_types(
	( runtime: $r:path, with_bridged_chain_grandpa_instance: $i:path, bridged_chain: $bridged:path ) => {
		{
			// if one of asserts fail, then either bridge isn't configured properly (or alternatively - non-standard
			// configuration is used), or something has broke existing configuration (meaning that all bridged chains
			// and relays will stop functioning)
			use pallet_bridge_grandpa::Config as GrandpaConfig;
			use static_assertions::assert_type_eq_all;

			assert_type_eq_all!(<$r as GrandpaConfig<$i>>::BridgedChain, $bridged);
		}
	}
);

/// Macro that ensures that the bridge messages pallet is configured properly to bridge using given
/// configuration.
#[macro_export]
macro_rules! assert_bridge_messages_pallet_types(
	(
		runtime: $r:path,
		with_bridged_chain_messages_instance: $i:path,
		bridge: $bridge:path
	) => {
		{
			// if one of asserts fail, then either bridge isn't configured properly (or alternatively - non-standard
			// configuration is used), or something has broke existing configuration (meaning that all bridged chains
			// and relays will stop functioning)
			use $crate::messages::{
				source::{FromThisChainMessagePayload, TargetHeaderChainAdapter},
				target::{FromBridgedChainMessagePayload, SourceHeaderChainAdapter},
				AccountIdOf, BalanceOf, BridgedChain, ThisChain,
			};
			use pallet_bridge_messages::Config as MessagesConfig;
			use static_assertions::assert_type_eq_all;

			assert_type_eq_all!(<$r as MessagesConfig<$i>>::OutboundPayload, FromThisChainMessagePayload);

			assert_type_eq_all!(<$r as MessagesConfig<$i>>::InboundRelayer, AccountIdOf<BridgedChain<$bridge>>);

			assert_type_eq_all!(<$r as MessagesConfig<$i>>::TargetHeaderChain, TargetHeaderChainAdapter<$bridge>);
			assert_type_eq_all!(<$r as MessagesConfig<$i>>::SourceHeaderChain, SourceHeaderChainAdapter<$bridge>);
		}
	}
);

/// Macro that combines four other macro calls - `assert_chain_types`, `assert_bridge_types`,
/// `assert_bridge_grandpa_pallet_types` and `assert_bridge_messages_pallet_types`. It may be used
/// at the chain that is implementing complete standard messages bridge (i.e. with bridge GRANDPA
/// and messages pallets deployed).
#[macro_export]
macro_rules! assert_complete_bridge_types(
	(
		runtime: $r:path,
		with_bridged_chain_grandpa_instance: $gi:path,
		with_bridged_chain_messages_instance: $mi:path,
		bridge: $bridge:path,
		this_chain: $this:path,
		bridged_chain: $bridged:path,
	) => {
		$crate::assert_chain_types!(runtime: $r, this_chain: $this);
		$crate::assert_bridge_grandpa_pallet_types!(
			runtime: $r,
			with_bridged_chain_grandpa_instance: $gi,
			bridged_chain: $bridged
		);
		$crate::assert_bridge_messages_pallet_types!(
			runtime: $r,
			with_bridged_chain_messages_instance: $mi,
			bridge: $bridge
		);
	}
);

/// Parameters for asserting chain-related constants.
#[derive(Debug)]
pub struct AssertChainConstants {
	/// Block length limits of the chain.
	pub block_length: limits::BlockLength,
	/// Block weight limits of the chain.
	pub block_weights: limits::BlockWeights,
}

/// Test that our hardcoded, chain-related constants, are matching chain runtime configuration.
///
/// In particular, this test ensures that:
///
/// 1) block weight limits are matching;
/// 2) block size limits are matching.
pub fn assert_chain_constants<R>(params: AssertChainConstants)
where
	R: frame_system::Config,
{
	// we don't check runtime version here, because in our case we'll be building relay from one
	// repo and runtime will live in another repo, along with outdated relay version. To avoid
	// unneeded commits, let's not raise an error in case of version mismatch.

	// if one of following assert fails, it means that we may need to upgrade bridged chain and
	// relay to use updated constants. If constants are now smaller than before, it may lead to
	// undeliverable messages.

	// `BlockLength` struct is not implementing `PartialEq`, so we compare encoded values here.
	assert_eq!(
		R::BlockLength::get().encode(),
		params.block_length.encode(),
		"BlockLength from runtime ({:?}) differ from hardcoded: {:?}",
		R::BlockLength::get(),
		params.block_length,
	);
	// `BlockWeights` struct is not implementing `PartialEq`, so we compare encoded values here
	assert_eq!(
		R::BlockWeights::get().encode(),
		params.block_weights.encode(),
		"BlockWeights from runtime ({:?}) differ from hardcoded: {:?}",
		R::BlockWeights::get(),
		params.block_weights,
	);
}

/// Test that the constants, used in GRANDPA pallet configuration are valid.
pub fn assert_bridge_grandpa_pallet_constants<R, GI>()
where
	R: pallet_bridge_grandpa::Config<GI>,
	GI: 'static,
{
	assert!(
		R::HeadersToKeep::get() > 0,
		"HeadersToKeep ({}) must be larger than zero",
		R::HeadersToKeep::get(),
	);
}

/// Parameters for asserting messages pallet constants.
#[derive(Debug)]
pub struct AssertBridgeMessagesPalletConstants {
	/// Maximal number of unrewarded relayer entries in a confirmation transaction at the bridged
	/// chain.
	pub max_unrewarded_relayers_in_bridged_confirmation_tx: MessageNonce,
	/// Maximal number of unconfirmed messages in a confirmation transaction at the bridged chain.
	pub max_unconfirmed_messages_in_bridged_confirmation_tx: MessageNonce,
	/// Identifier of the bridged chain.
	pub bridged_chain_id: ChainId,
}

/// Test that the constants, used in messages pallet configuration are valid.
pub fn assert_bridge_messages_pallet_constants<R, MI>(params: AssertBridgeMessagesPalletConstants)
where
	R: pallet_bridge_messages::Config<MI>,
	MI: 'static,
{
	assert!(
		!R::ActiveOutboundLanes::get().is_empty(),
		"ActiveOutboundLanes ({:?}) must not be empty",
		R::ActiveOutboundLanes::get(),
	);
	assert!(
		R::MaxUnrewardedRelayerEntriesAtInboundLane::get() <= params.max_unrewarded_relayers_in_bridged_confirmation_tx,
		"MaxUnrewardedRelayerEntriesAtInboundLane ({}) must be <= than the hardcoded value for bridged chain: {}",
		R::MaxUnrewardedRelayerEntriesAtInboundLane::get(),
		params.max_unrewarded_relayers_in_bridged_confirmation_tx,
	);
	assert!(
		R::MaxUnconfirmedMessagesAtInboundLane::get() <= params.max_unconfirmed_messages_in_bridged_confirmation_tx,
		"MaxUnrewardedRelayerEntriesAtInboundLane ({}) must be <= than the hardcoded value for bridged chain: {}",
		R::MaxUnconfirmedMessagesAtInboundLane::get(),
		params.max_unconfirmed_messages_in_bridged_confirmation_tx,
	);
	assert_eq!(R::BridgedChainId::get(), params.bridged_chain_id);
}

/// Parameters for asserting bridge pallet names.
#[derive(Debug)]
pub struct AssertBridgePalletNames<'a> {
	/// Name of the messages pallet, deployed at the bridged chain and used to bridge with this
	/// chain.
	pub with_this_chain_messages_pallet_name: &'a str,
	/// Name of the GRANDPA pallet, deployed at this chain and used to bridge with the bridged
	/// chain.
	pub with_bridged_chain_grandpa_pallet_name: &'a str,
	/// Name of the messages pallet, deployed at this chain and used to bridge with the bridged
	/// chain.
	pub with_bridged_chain_messages_pallet_name: &'a str,
}

/// Tests that bridge pallet names used in `construct_runtime!()` macro call are matching constants
/// from chain primitives crates.
pub fn assert_bridge_pallet_names<B, R, GI, MI>(params: AssertBridgePalletNames)
where
	B: MessageBridge,
	R: pallet_bridge_grandpa::Config<GI> + pallet_bridge_messages::Config<MI>,
	GI: 'static,
	MI: 'static,
{
	assert_eq!(B::BRIDGED_MESSAGES_PALLET_NAME, params.with_this_chain_messages_pallet_name);
	assert_eq!(
		pallet_bridge_grandpa::PalletOwner::<R, GI>::storage_value_final_key().to_vec(),
		bp_runtime::storage_value_key(params.with_bridged_chain_grandpa_pallet_name, "PalletOwner",).0,
	);
	assert_eq!(
		pallet_bridge_messages::PalletOwner::<R, MI>::storage_value_final_key().to_vec(),
		bp_runtime::storage_value_key(
			params.with_bridged_chain_messages_pallet_name,
			"PalletOwner",
		)
		.0,
	);
}

/// Parameters for asserting complete standard messages bridge.
#[derive(Debug)]
pub struct AssertCompleteBridgeConstants<'a> {
	/// Parameters to assert this chain constants.
	pub this_chain_constants: AssertChainConstants,
	/// Parameters to assert messages pallet constants.
	pub messages_pallet_constants: AssertBridgeMessagesPalletConstants,
	/// Parameters to assert pallet names constants.
	pub pallet_names: AssertBridgePalletNames<'a>,
}

/// All bridge-related constants tests for the complete standard messages bridge (i.e. with bridge
/// GRANDPA and messages pallets deployed).
pub fn assert_complete_bridge_constants<R, GI, MI, B>(params: AssertCompleteBridgeConstants)
where
	R: frame_system::Config
		+ pallet_bridge_grandpa::Config<GI>
		+ pallet_bridge_messages::Config<MI>,
	GI: 'static,
	MI: 'static,
	B: MessageBridge,
{
	assert_chain_constants::<R>(params.this_chain_constants);
	assert_bridge_grandpa_pallet_constants::<R, GI>();
	assert_bridge_messages_pallet_constants::<R, MI>(params.messages_pallet_constants);
	assert_bridge_pallet_names::<B, R, GI, MI>(params.pallet_names);
}

/// Check that the message lane weights are correct.
pub fn check_message_lane_weights<
	C: Chain,
	T: frame_system::Config + pallet_bridge_messages::Config<MessagesPalletInstance>,
	MessagesPalletInstance: 'static,
>(
	bridged_chain_extra_storage_proof_size: u32,
	this_chain_max_unrewarded_relayers: MessageNonce,
	this_chain_max_unconfirmed_messages: MessageNonce,
	// whether `RefundBridgedParachainMessages` extension is deployed at runtime and is used for
	// refunding this bridge transactions?
	//
	// in other words: pass true for all known production chains
	runtime_includes_refund_extension: bool,
) {
	type Weights<T, MI> = <T as pallet_bridge_messages::Config<MI>>::WeightInfo;

	// check basic weight assumptions
	pallet_bridge_messages::ensure_weights_are_correct::<Weights<T, MessagesPalletInstance>>();

	// check that weights allow us to receive messages
	let max_incoming_message_proof_size = bridged_chain_extra_storage_proof_size
		.saturating_add(messages::target::maximal_incoming_message_size(C::max_extrinsic_size()));
	pallet_bridge_messages::ensure_able_to_receive_message::<Weights<T, MessagesPalletInstance>>(
		C::max_extrinsic_size(),
		C::max_extrinsic_weight(),
		max_incoming_message_proof_size,
		messages::target::maximal_incoming_message_dispatch_weight(C::max_extrinsic_weight()),
	);

	// check that weights allow us to receive delivery confirmations
	let max_incoming_inbound_lane_data_proof_size =
		InboundLaneData::<()>::encoded_size_hint_u32(this_chain_max_unrewarded_relayers as _);
	pallet_bridge_messages::ensure_able_to_receive_confirmation::<Weights<T, MessagesPalletInstance>>(
		C::max_extrinsic_size(),
		C::max_extrinsic_weight(),
		max_incoming_inbound_lane_data_proof_size,
		this_chain_max_unrewarded_relayers,
		this_chain_max_unconfirmed_messages,
	);

	// check that extra weights of delivery/confirmation transactions include the weight
	// of `RefundBridgedParachainMessages` operations. This signed extension assumes the worst case
	// (i.e. slashing if delivery transaction was invalid) and refunds some weight if
	// assumption was wrong (i.e. if we did refund instead of slashing). This check
	// ensures the extension will not refund weight when it doesn't need to (i.e. if pallet
	// weights do not account weights of refund extension).
	if runtime_includes_refund_extension {
		assert_ne!(
			Weights::<T, MessagesPalletInstance>::receive_messages_proof_overhead_from_runtime(),
			Weight::zero()
		);
		assert_ne!(
			Weights::<T, MessagesPalletInstance>::receive_messages_delivery_proof_overhead_from_runtime(),
			Weight::zero()
		);
	}
}
