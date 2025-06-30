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
//! bridge with remote parachain.

use crate::{
	extension::{
		grandpa_adapter::verify_submit_finality_proof_succeeded, verify_messages_call_succeeded,
	},
	Config as BridgeRelayersConfig, LOG_TARGET,
};

use bp_relayers::{BatchCallUnpacker, ExtensionCallData, ExtensionCallInfo, ExtensionConfig};
use bp_runtime::{Parachain, StaticStrProvider};
use core::marker::PhantomData;
use frame_support::dispatch::{DispatchInfo, PostDispatchInfo};
use frame_system::Config as SystemConfig;
use pallet_bridge_grandpa::{
	CallSubType as BridgeGrandpaCallSubtype, Config as BridgeGrandpaConfig,
};
use pallet_bridge_messages::{
	CallSubType as BridgeMessagesCallSubType, Config as BridgeMessagesConfig, LaneIdOf,
};
use pallet_bridge_parachains::{
	CallSubType as BridgeParachainsCallSubtype, Config as BridgeParachainsConfig,
	SubmitParachainHeadsHelper,
};
use sp_runtime::{
	traits::{Dispatchable, Get},
	transaction_validity::{TransactionPriority, TransactionValidityError},
};

/// Adapter to be used in signed extension configuration, when bridging with remote parachains.
pub struct WithParachainExtensionConfig<
	// signed extension identifier
	IdProvider,
	// runtime that implements `BridgeMessagesConfig<BridgeMessagesPalletInstance>`, which
	// uses `BridgeParachainsConfig<BridgeParachainsPalletInstance>` to receive messages and
	// confirmations from the remote chain.
	Runtime,
	// batch call unpacker
	BatchCallUnpacker,
	// instance of the `pallet-bridge-parachains`, tracked by this extension
	BridgeParachainsPalletInstance,
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
		BridgeParachainsPalletInstance,
		BridgeMessagesPalletInstance,
		BridgeRelayersPalletInstance,
		PriorityBoostPerMessage,
	)>,
);

impl<ID, R, BCU, PI, MI, RI, P> ExtensionConfig
	for WithParachainExtensionConfig<ID, R, BCU, PI, MI, RI, P>
where
	ID: StaticStrProvider,
	R: BridgeRelayersConfig<RI>
		+ BridgeMessagesConfig<MI>
		+ BridgeParachainsConfig<PI>
		+ BridgeGrandpaConfig<R::BridgesGrandpaPalletInstance>,
	BCU: BatchCallUnpacker<R>,
	PI: 'static,
	MI: 'static,
	RI: 'static,
	P: Get<TransactionPriority>,
	R::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ BridgeGrandpaCallSubtype<R, R::BridgesGrandpaPalletInstance>
		+ BridgeParachainsCallSubtype<R, PI>
		+ BridgeMessagesCallSubType<R, MI>,
	<R as BridgeMessagesConfig<MI>>::BridgedChain: Parachain,
{
	type IdProvider = ID;
	type Runtime = R;
	type BridgeMessagesPalletInstance = MI;
	type BridgeRelayersPalletInstance = RI;
	type PriorityBoostPerMessage = P;
	type RemoteGrandpaChainBlockNumber =
		pallet_bridge_grandpa::BridgedBlockNumber<R, R::BridgesGrandpaPalletInstance>;
	type LaneId = LaneIdOf<R, Self::BridgeMessagesPalletInstance>;

	fn parse_and_check_for_obsolete_call(
		call: &R::RuntimeCall,
	) -> Result<
		Option<ExtensionCallInfo<Self::RemoteGrandpaChainBlockNumber, Self::LaneId>>,
		TransactionValidityError,
	> {
		let calls = BCU::unpack(call, 3);
		let total_calls = calls.len();
		let mut calls = calls.into_iter().map(Self::check_obsolete_parsed_call).rev();

		let msgs_call = calls.next().transpose()?.and_then(|c| c.call_info());
		let para_finality_call = calls.next().transpose()?.and_then(|c| {
			let r = c.submit_parachain_heads_info_for(
				<R as BridgeMessagesConfig<Self::BridgeMessagesPalletInstance>>::BridgedChain::PARACHAIN_ID,
			);
			r
		});
		let relay_finality_call =
			calls.next().transpose()?.and_then(|c| c.submit_finality_proof_info());
		Ok(match (total_calls, relay_finality_call, para_finality_call, msgs_call) {
			(3, Some(relay_finality_call), Some(para_finality_call), Some(msgs_call)) =>
				Some(ExtensionCallInfo::AllFinalityAndMsgs(
					relay_finality_call,
					para_finality_call,
					msgs_call,
				)),
			(2, None, Some(para_finality_call), Some(msgs_call)) =>
				Some(ExtensionCallInfo::ParachainFinalityAndMsgs(para_finality_call, msgs_call)),
			(1, None, None, Some(msgs_call)) => Some(ExtensionCallInfo::Msgs(msgs_call)),
			_ => None,
		})
	}

	fn check_obsolete_parsed_call(
		call: &R::RuntimeCall,
	) -> Result<&R::RuntimeCall, TransactionValidityError> {
		call.check_obsolete_submit_finality_proof()?;
		call.check_obsolete_submit_parachain_heads()?;
		call.check_obsolete_call()?;
		Ok(call)
	}

	fn check_call_result(
		call_info: &ExtensionCallInfo<Self::RemoteGrandpaChainBlockNumber, Self::LaneId>,
		call_data: &mut ExtensionCallData,
		relayer: &R::AccountId,
	) -> bool {
		verify_submit_finality_proof_succeeded::<Self, R::BridgesGrandpaPalletInstance>(
			call_info, call_data, relayer,
		) && verify_submit_parachain_head_succeeded::<Self, PI>(call_info, call_data, relayer) &&
			verify_messages_call_succeeded::<Self>(call_info, call_data, relayer)
	}
}

/// If the batch call contains the parachain state update call, verify that it
/// has been successful.
///
/// Only returns false when parachain state update call has failed.
pub(crate) fn verify_submit_parachain_head_succeeded<C, PI>(
	call_info: &ExtensionCallInfo<C::RemoteGrandpaChainBlockNumber, C::LaneId>,
	_call_data: &mut ExtensionCallData,
	relayer: &<C::Runtime as SystemConfig>::AccountId,
) -> bool
where
	C: ExtensionConfig,
	PI: 'static,
	C::Runtime: BridgeParachainsConfig<PI>,
{
	let Some(para_proof_info) = call_info.submit_parachain_heads_info() else { return true };

	if !SubmitParachainHeadsHelper::<C::Runtime, PI>::was_successful(para_proof_info) {
		// we only refund relayer if all calls have updated chain state
		log::trace!(
			target: LOG_TARGET,
			"{}.{:?}: relayer {:?} has submitted invalid parachain finality proof",
			C::IdProvider::STR,
			call_info.messages_call_info().lane_id(),
			relayer,
		);
		return false
	}

	true
}
