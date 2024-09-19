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
//! bridge with any remote chain. This adapter does not refund any finality transactions.

use crate::{extension::verify_messages_call_succeeded, Config as BridgeRelayersConfig};

use bp_relayers::{ExtensionCallData, ExtensionCallInfo, ExtensionConfig};
use bp_runtime::StaticStrProvider;
use frame_support::dispatch::{DispatchInfo, PostDispatchInfo};
use pallet_bridge_messages::{
	CallSubType as BridgeMessagesCallSubType, Config as BridgeMessagesConfig,
};
use sp_runtime::{
	traits::{Dispatchable, Get},
	transaction_validity::{TransactionPriority, TransactionValidityError},
};
use sp_std::marker::PhantomData;

/// Transaction extension that refunds a relayer for standalone messages delivery and confirmation
/// transactions. Finality transactions are not refunded.
pub struct WithMessagesExtensionConfig<
	IdProvider,
	Runtime,
	BridgeMessagesPalletInstance,
	PriorityBoostPerMessage,
>(
	PhantomData<(
		// signed extension identifier
		IdProvider,
		// runtime with `pallet-bridge-messages` pallet deployed
		Runtime,
		// instance of BridgedChain `pallet-bridge-messages`, tracked by this extension
		BridgeMessagesPalletInstance,
		// message delivery transaction priority boost for every additional message
		PriorityBoostPerMessage,
	)>,
);

impl<ID, R, MI, P> ExtensionConfig for WithMessagesExtensionConfig<ID, R, MI, P>
where
	ID: StaticStrProvider,
	R: BridgeRelayersConfig + BridgeMessagesConfig<MI>,
	MI: 'static,
	P: Get<TransactionPriority>,
	R::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>
		+ BridgeMessagesCallSubType<R, MI>,
{
	type IdProvider = ID;
	type Runtime = R;
	type BridgeMessagesPalletInstance = MI;
	type PriorityBoostPerMessage = P;
	type Reward = R::Reward;
	type RemoteGrandpaChainBlockNumber = ();

	fn parse_and_check_for_obsolete_call(
		call: &R::RuntimeCall,
	) -> Result<
		Option<ExtensionCallInfo<Self::RemoteGrandpaChainBlockNumber>>,
		TransactionValidityError,
	> {
		let call = Self::check_obsolete_parsed_call(call)?;
		Ok(call.call_info().map(ExtensionCallInfo::Msgs))
	}

	fn check_obsolete_parsed_call(
		call: &R::RuntimeCall,
	) -> Result<&R::RuntimeCall, TransactionValidityError> {
		call.check_obsolete_call()?;
		Ok(call)
	}

	fn check_call_result(
		call_info: &ExtensionCallInfo<Self::RemoteGrandpaChainBlockNumber>,
		call_data: &mut ExtensionCallData,
		relayer: &R::AccountId,
	) -> bool {
		verify_messages_call_succeeded::<Self, MI>(call_info, call_data, relayer)
	}
}
