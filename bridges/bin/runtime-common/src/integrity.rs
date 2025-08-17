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

use bp_header_chain::ChainWithGrandpa;
use bp_messages::{ChainWithMessages, InboundLaneData, MessageNonce};
use bp_runtime::{AccountIdOf, Chain};
use codec::Encode;
use frame_support::{storage::generator::StorageValue, traits::Get, weights::Weight};
use frame_system::limits;
use pallet_bridge_messages::{ThisChainOf, WeightInfoExt as _};

// Re-export to avoid include all dependencies everywhere.
#[doc(hidden)]
pub mod __private {
	pub use static_assertions;
}

/// Macro that ensures that the runtime configuration and chain primitives crate are sharing
/// the same types (nonce, block number, hash, hasher, account id and header).
#[macro_export]
macro_rules! assert_chain_types(
	( runtime: $r:path, this_chain: $this:path ) => {
		{
			use frame_system::{Config as SystemConfig, pallet_prelude::{BlockNumberFor, HeaderFor}};
			use $crate::integrity::__private::static_assertions::assert_type_eq_all;

			// if one of asserts fail, then either bridge isn't configured properly (or alternatively - non-standard
			// configuration is used), or something has broke existing configuration (meaning that all bridged chains
			// and relays will stop functioning)

			assert_type_eq_all!(<$r as SystemConfig>::Nonce, bp_runtime::NonceOf<$this>);
			assert_type_eq_all!(BlockNumberFor<$r>, bp_runtime::BlockNumberOf<$this>);
			assert_type_eq_all!(<$r as SystemConfig>::Hash, bp_runtime::HashOf<$this>);
			assert_type_eq_all!(<$r as SystemConfig>::Hashing, bp_runtime::HasherOf<$this>);
			assert_type_eq_all!(<$r as SystemConfig>::AccountId, bp_runtime::AccountIdOf<$this>);
			assert_type_eq_all!(HeaderFor<$r>, bp_runtime::HeaderOf<$this>);
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
		this_chain: $this:path,
		bridged_chain: $bridged:path,
		expected_payload_type: $payload:path,
	) => {
		{
			use $crate::integrity::__private::static_assertions::assert_type_eq_all;
			use bp_messages::ChainWithMessages;
			use bp_runtime::Chain;
			use pallet_bridge_messages::Config as BridgeMessagesConfig;

			// if one of asserts fail, then either bridge isn't configured properly (or alternatively - non-standard
			// configuration is used), or something has broke existing configuration (meaning that all bridged chains
			// and relays will stop functioning)

			assert_type_eq_all!(<$r as BridgeMessagesConfig<$i>>::ThisChain, $this);
			assert_type_eq_all!(<$r as BridgeMessagesConfig<$i>>::BridgedChain, $bridged);

			assert_type_eq_all!(<$r as BridgeMessagesConfig<$i>>::OutboundPayload, $payload);
			assert_type_eq_all!(<$r as BridgeMessagesConfig<$i>>::InboundPayload, $payload);
		}
	}
);

/// Macro that combines four other macro calls - `assert_chain_types`, `assert_bridge_types`,
/// and `assert_bridge_messages_pallet_types`. It may be used
/// at the chain that is implementing standard messages bridge with messages pallets deployed.
#[macro_export]
macro_rules! assert_complete_bridge_types(
	(
		runtime: $r:path,
		with_bridged_chain_messages_instance: $mi:path,
		this_chain: $this:path,
		bridged_chain: $bridged:path,
		expected_payload_type: $payload:path,
	) => {
		$crate::assert_chain_types!(runtime: $r, this_chain: $this);
		$crate::assert_bridge_messages_pallet_types!(
			runtime: $r,
			with_bridged_chain_messages_instance: $mi,
			this_chain: $this,
			bridged_chain: $bridged,
			expected_payload_type: $payload,
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

/// Test that the constants, used in messages pallet configuration are valid.
pub fn assert_bridge_messages_pallet_constants<R, MI>()
where
	R: pallet_bridge_messages::Config<MI>,
	MI: 'static,
{
	assert!(
		pallet_bridge_messages::BridgedChainOf::<R, MI>::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX
			<= pallet_bridge_messages::BridgedChainOf::<R, MI>::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
		"MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX ({}) of {:?} is larger than \
			its MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX ({}). This makes \
			no sense",
		pallet_bridge_messages::BridgedChainOf::<R, MI>::MAX_UNREWARDED_RELAYERS_IN_CONFIRMATION_TX,
		pallet_bridge_messages::BridgedChainOf::<R, MI>::ID,
		pallet_bridge_messages::BridgedChainOf::<R, MI>::MAX_UNCONFIRMED_MESSAGES_IN_CONFIRMATION_TX,
	);
}

/// Parameters for asserting bridge GRANDPA pallet names.
#[derive(Debug)]
struct AssertBridgeGrandpaPalletNames<'a> {
	/// Name of the GRANDPA pallet, deployed at this chain and used to bridge with the bridged
	/// chain.
	pub with_bridged_chain_grandpa_pallet_name: &'a str,
}

/// Tests that bridge pallet names used in `construct_runtime!()` macro call are matching constants
/// from chain primitives crates.
fn assert_bridge_grandpa_pallet_names<R, GI>(params: AssertBridgeGrandpaPalletNames)
where
	R: pallet_bridge_grandpa::Config<GI>,
	GI: 'static,
{
	// check that the bridge GRANDPA pallet has required name
	assert_eq!(
			pallet_bridge_grandpa::PalletOwner::<R, GI>::storage_value_final_key().to_vec(),
			bp_runtime::storage_value_key(
				params.with_bridged_chain_grandpa_pallet_name,
				"PalletOwner",
			)
			.0,
		);
	assert_eq!(
		pallet_bridge_grandpa::PalletOperatingMode::<R, GI>::storage_value_final_key().to_vec(),
		bp_runtime::storage_value_key(
			params.with_bridged_chain_grandpa_pallet_name,
			"PalletOperatingMode",
		)
		.0,
	);
}

/// Parameters for asserting bridge messages pallet names.
#[derive(Debug)]
struct AssertBridgeMessagesPalletNames<'a> {
	/// Name of the messages pallet, deployed at this chain and used to bridge with the bridged
	/// chain.
	pub with_bridged_chain_messages_pallet_name: &'a str,
}

/// Tests that bridge pallet names used in `construct_runtime!()` macro call are matching constants
/// from chain primitives crates.
fn assert_bridge_messages_pallet_names<R, MI>(params: AssertBridgeMessagesPalletNames)
where
	R: pallet_bridge_messages::Config<MI>,
	MI: 'static,
{
	// check that the bridge messages pallet has required name
	assert_eq!(
		pallet_bridge_messages::PalletOwner::<R, MI>::storage_value_final_key().to_vec(),
		bp_runtime::storage_value_key(
			params.with_bridged_chain_messages_pallet_name,
			"PalletOwner",
		)
		.0,
	);
	assert_eq!(
		pallet_bridge_messages::PalletOperatingMode::<R, MI>::storage_value_final_key().to_vec(),
		bp_runtime::storage_value_key(
			params.with_bridged_chain_messages_pallet_name,
			"PalletOperatingMode",
		)
		.0,
	);
}

/// Parameters for asserting complete standard messages bridge.
#[derive(Debug)]
pub struct AssertCompleteBridgeConstants {
	/// Parameters to assert this chain constants.
	pub this_chain_constants: AssertChainConstants,
}

/// All bridge-related constants tests for the complete standard relay-chain messages bridge
/// (i.e. with bridge GRANDPA and messages pallets deployed).
pub fn assert_complete_with_relay_chain_bridge_constants<R, GI, MI>(
	params: AssertCompleteBridgeConstants,
) where
	R: frame_system::Config
		+ pallet_bridge_grandpa::Config<GI>
		+ pallet_bridge_messages::Config<MI>,
	GI: 'static,
	MI: 'static,
{
	assert_chain_constants::<R>(params.this_chain_constants);
	assert_bridge_grandpa_pallet_constants::<R, GI>();
	assert_bridge_messages_pallet_constants::<R, MI>();
	assert_bridge_grandpa_pallet_names::<R, GI>(AssertBridgeGrandpaPalletNames {
		with_bridged_chain_grandpa_pallet_name:
			<R as pallet_bridge_grandpa::Config<GI>>::BridgedChain::WITH_CHAIN_GRANDPA_PALLET_NAME,
	});
	assert_bridge_messages_pallet_names::<R, MI>(AssertBridgeMessagesPalletNames {
		with_bridged_chain_messages_pallet_name:
			<R as pallet_bridge_messages::Config<MI>>::BridgedChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
	});
}

/// All bridge-related constants tests for the complete standard parachain messages bridge
/// (i.e. with bridge GRANDPA, parachains and messages pallets deployed).
pub fn assert_complete_with_parachain_bridge_constants<R, PI, MI>(
	params: AssertCompleteBridgeConstants,
) where
	R: frame_system::Config
		+ pallet_bridge_parachains::Config<PI>
		+ pallet_bridge_messages::Config<MI>,
	<R as pallet_bridge_parachains::BoundedBridgeGrandpaConfig<R::BridgesGrandpaPalletInstance>>::BridgedRelayChain: ChainWithGrandpa,
	PI: 'static,
	MI: 'static,
{
	assert_chain_constants::<R>(params.this_chain_constants);
	assert_bridge_grandpa_pallet_constants::<R, R::BridgesGrandpaPalletInstance>();
	assert_bridge_messages_pallet_constants::<R, MI>();
	assert_bridge_grandpa_pallet_names::<R, R::BridgesGrandpaPalletInstance>(
		AssertBridgeGrandpaPalletNames {
			with_bridged_chain_grandpa_pallet_name:
				<<R as pallet_bridge_parachains::BoundedBridgeGrandpaConfig<
					R::BridgesGrandpaPalletInstance,
				>>::BridgedRelayChain>::WITH_CHAIN_GRANDPA_PALLET_NAME,
		},
	);
	assert_bridge_messages_pallet_names::<R, MI>(AssertBridgeMessagesPalletNames {
		with_bridged_chain_messages_pallet_name:
			<R as pallet_bridge_messages::Config<MI>>::BridgedChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
	});
}

/// All bridge-related constants tests for the standalone messages bridge deployment (only with
/// messages pallets deployed).
pub fn assert_standalone_messages_bridge_constants<R, MI>(params: AssertCompleteBridgeConstants)
where
	R: frame_system::Config + pallet_bridge_messages::Config<MI>,
	MI: 'static,
{
	assert_chain_constants::<R>(params.this_chain_constants);
	assert_bridge_messages_pallet_constants::<R, MI>();
	assert_bridge_messages_pallet_names::<R, MI>(AssertBridgeMessagesPalletNames {
		with_bridged_chain_messages_pallet_name:
			<R as pallet_bridge_messages::Config<MI>>::BridgedChain::WITH_CHAIN_MESSAGES_PALLET_NAME,
	});
}

/// Check that the message lane weights are correct.
pub fn check_message_lane_weights<
	C: ChainWithMessages,
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

	// check that the maximal message dispatch weight is below hardcoded limit
	pallet_bridge_messages::ensure_maximal_message_dispatch::<Weights<T, MessagesPalletInstance>>(
		C::maximal_incoming_message_size(),
		C::maximal_incoming_message_dispatch_weight(),
	);

	// check that weights allow us to receive messages
	let max_incoming_message_proof_size =
		bridged_chain_extra_storage_proof_size.saturating_add(C::maximal_incoming_message_size());
	pallet_bridge_messages::ensure_able_to_receive_message::<Weights<T, MessagesPalletInstance>>(
		C::max_extrinsic_size(),
		C::max_extrinsic_weight(),
		max_incoming_message_proof_size,
		C::maximal_incoming_message_dispatch_weight(),
	);

	// check that weights allow us to receive delivery confirmations
	let max_incoming_inbound_lane_data_proof_size = InboundLaneData::<
		AccountIdOf<ThisChainOf<T, MessagesPalletInstance>>,
	>::encoded_size_hint_u32(
		this_chain_max_unrewarded_relayers as _
	);
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
