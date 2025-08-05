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

//! Adapter that allows using `pallet-bridge-relayers` as a signed extension in the
//! bridge with remote GRANDPA chain.

use crate::{
	extension::verify_messages_call_succeeded, Config as BridgeRelayersConfig, LOG_TARGET,
};

use bp_relayers::{BatchCallUnpacker, ExtensionCallData, ExtensionCallInfo, ExtensionConfig};
use bp_runtime::{Chain, StaticStrProvider};
use core::marker::PhantomData;
use frame_support::dispatch::{DispatchInfo, PostDispatchInfo};
use frame_system::Config as SystemConfig;
use pallet_bridge_grandpa::{
	CallSubType as BridgeGrandpaCallSubtype, Config as BridgeGrandpaConfig,
	SubmitFinalityProofHelper,
};
use pallet_bridge_messages::{
	CallSubType as BridgeMessagesCallSubType, Config as BridgeMessagesConfig, LaneIdOf,
};
use sp_runtime::{
	traits::{Dispatchable, Get},
	transaction_validity::{TransactionPriority, TransactionValidityError},
	Saturating,
};

/// Adapter to be used in signed extension configuration, when bridging with remote
/// chains that are using GRANDPA finality.
pub struct WithGrandpaChainExtensionConfig<
	// signed extension identifier
	IdProvider,
	// runtime that implements `BridgeMessagesConfig<BridgeMessagesPalletInstance>`, which
	// uses `BridgeGrandpaConfig<BridgeGrandpaPalletInstance>` to receive messages and
	// confirmations from the remote chain.
	Runtime,
	// batch call unpacker
	BatchCallUnpacker,
	// instance of the `pallet-bridge-grandpa`, tracked by this extension
	BridgeGrandpaPalletInstance,
	// instance of BridgedChain `pallet-bridge-messages`, tracked by this extension
	BridgeMessagesPalletInstance,
	// instance of `pallet-bridge-relayers`, tracked by this extension
	BridgeRelayersPalletInstance,
	// message delivery transaction priority boost for every additional message
	PriorityBoostPerMessage,
>(
	PhantomData<(
		IdProvider,
		Runtime,
		BatchCallUnpacker,
		BridgeGrandpaPalletInstance,
		BridgeMessagesPalletInstance,
		BridgeRelayersPalletInstance,
		PriorityBoostPerMessage,
	)>,
);

impl<ID, R, BCU, GI, MI, RI, P> ExtensionConfig
	for WithGrandpaChainExtensionConfig<ID, R, BCU, GI, MI, RI, P>
where
	ID: StaticStrProvider,
	R: BridgeRelayersConfig<RI>
		+ BridgeMessagesConfig<MI, BridgedChain = pallet_bridge_grandpa::BridgedChain<R, GI>>
		+ BridgeGrandpaConfig<GI>,
	BCU: BatchCallUnpacker<R>,
	GI: 'static,
	MI: 'static,
	RI: 'static,
	P: Get<TransactionPriority>,
	R::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ BridgeGrandpaCallSubtype<R, GI>
		+ BridgeMessagesCallSubType<R, MI>,
{
	type IdProvider = ID;
	type Runtime = R;
	type BridgeMessagesPalletInstance = MI;
	type BridgeRelayersPalletInstance = RI;
	type PriorityBoostPerMessage = P;
	type RemoteGrandpaChainBlockNumber = pallet_bridge_grandpa::BridgedBlockNumber<R, GI>;
	type LaneId = LaneIdOf<R, Self::BridgeMessagesPalletInstance>;

	fn parse_and_check_for_obsolete_call(
		call: &R::RuntimeCall,
	) -> Result<
		Option<ExtensionCallInfo<Self::RemoteGrandpaChainBlockNumber, Self::LaneId>>,
		TransactionValidityError,
	> {
		let calls = BCU::unpack(call, 2);
		let total_calls = calls.len();
		let mut calls = calls.into_iter().map(Self::check_obsolete_parsed_call).rev();

		let msgs_call = calls.next().transpose()?.and_then(|c| c.call_info());
		let relay_finality_call =
			calls.next().transpose()?.and_then(|c| c.submit_finality_proof_info());

		Ok(match (total_calls, relay_finality_call, msgs_call) {
			(2, Some(relay_finality_call), Some(msgs_call)) =>
				Some(ExtensionCallInfo::RelayFinalityAndMsgs(relay_finality_call, msgs_call)),
			(1, None, Some(msgs_call)) => Some(ExtensionCallInfo::Msgs(msgs_call)),
			_ => None,
		})
	}

	fn check_obsolete_parsed_call(
		call: &R::RuntimeCall,
	) -> Result<&R::RuntimeCall, TransactionValidityError> {
		call.check_obsolete_submit_finality_proof()?;
		call.check_obsolete_call()?;
		Ok(call)
	}

	fn check_call_result(
		call_info: &ExtensionCallInfo<Self::RemoteGrandpaChainBlockNumber, Self::LaneId>,
		call_data: &mut ExtensionCallData,
		relayer: &R::AccountId,
	) -> bool {
		verify_submit_finality_proof_succeeded::<Self, GI>(call_info, call_data, relayer) &&
			verify_messages_call_succeeded::<Self>(call_info, call_data, relayer)
	}
}

/// If the batch call contains the GRANDPA chain state update call, verify that it
/// has been successful.
///
/// Only returns false when GRANDPA chain state update call has failed.
pub(crate) fn verify_submit_finality_proof_succeeded<C, GI>(
	call_info: &ExtensionCallInfo<C::RemoteGrandpaChainBlockNumber, C::LaneId>,
	call_data: &mut ExtensionCallData,
	relayer: &<C::Runtime as SystemConfig>::AccountId,
) -> bool
where
	C: ExtensionConfig,
	GI: 'static,
	C::Runtime: BridgeGrandpaConfig<GI>,
	<C::Runtime as BridgeGrandpaConfig<GI>>::BridgedChain:
		Chain<BlockNumber = C::RemoteGrandpaChainBlockNumber>,
{
	let Some(finality_proof_info) = call_info.submit_finality_proof_info() else { return true };

	if !SubmitFinalityProofHelper::<C::Runtime, GI>::was_successful(
		finality_proof_info.block_number,
	) {
		// we only refund relayer if all calls have updated chain state
		log::trace!(
			target: LOG_TARGET,
			"{}.{:?}: relayer {:?} has submitted invalid GRANDPA chain finality proof",
			C::IdProvider::STR,
			call_info.messages_call_info().lane_id(),
			relayer,
		);
		return false
	}

	// there's a conflict between how bridge GRANDPA pallet works and a `utility.batchAll`
	// transaction. If relay chain header is mandatory, the GRANDPA pallet returns
	// `Pays::No`, because such transaction is mandatory for operating the bridge. But
	// `utility.batchAll` transaction always requires payment. But in both cases we'll
	// refund relayer - either explicitly here, or using `Pays::No` if he's choosing
	// to submit dedicated transaction.

	// submitter has means to include extra weight/bytes in the `submit_finality_proof`
	// call, so let's subtract extra weight/size to avoid refunding for this extra stuff
	call_data.extra_weight.saturating_accrue(finality_proof_info.extra_weight);
	call_data.extra_size.saturating_accrue(finality_proof_info.extra_size);

	true
}
