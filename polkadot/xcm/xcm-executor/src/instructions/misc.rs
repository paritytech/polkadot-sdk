// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::ExecuteInstruction;
use crate::traits::{
	validate_export, CallDispatcher, ConvertOrigin, ExportXcm, FeeReason, ProcessTransaction,
};
use crate::{config, MaybeErrorCode, XcmExecutor};
use codec::{Decode, Encode};
use frame_support::{
	dispatch::GetDispatchInfo,
	traits::{Contains, Defensive, Get},
};
use sp_io::hashing::blake2_128;
use xcm::latest::{instructions::*, Error as XcmError};

impl<Config: config::Config> ExecuteInstruction<Config> for Transact<Config::RuntimeCall> {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let Transact { origin_kind, mut call, .. } = self;
		// We assume that the Relay-chain is allowed to use transact on this parachain.
		let origin = executor.cloned_origin().ok_or_else(|| {
			tracing::trace!(
				target: "xcm::process_instruction::transact",
				"No origin provided",
			);

			XcmError::BadOrigin
		})?;

		// TODO: #2841 #TRANSACTFILTER allow the trait to issue filters for the relay-chain
		let message_call = call.take_decoded().map_err(|_| {
			tracing::trace!(
				target: "xcm::process_instruction::transact",
				"Failed to decode call",
			);

			XcmError::FailedToDecode
		})?;

		tracing::trace!(
			target: "xcm::process_instruction::transact",
			?call,
			"Processing call",
		);

		if !Config::SafeCallFilter::contains(&message_call) {
			tracing::trace!(
				target: "xcm::process_instruction::transact",
				"Call filtered by `SafeCallFilter`",
			);

			return Err(XcmError::NoPermission);
		}

		let dispatch_origin = Config::OriginConverter::convert_origin(origin.clone(), origin_kind)
			.map_err(|_| {
				tracing::trace!(
					target: "xcm::process_instruction::transact",
					?origin,
					?origin_kind,
					"Failed to convert origin to a local origin."
				);

				XcmError::BadOrigin
			})?;

		tracing::trace!(
			target: "xcm::process_instruction::transact",
			origin = ?dispatch_origin,
			"Dispatching with origin",
		);

		let weight = message_call.get_dispatch_info().call_weight;
		let maybe_actual_weight =
			match Config::CallDispatcher::dispatch(message_call, dispatch_origin) {
				Ok(post_info) => {
					tracing::trace!(
						target: "xcm::process_instruction::transact",
						?post_info,
						"Dispatch successful"
					);
					executor.transact_status = MaybeErrorCode::Success;
					post_info.actual_weight
				},
				Err(error_and_info) => {
					tracing::trace!(
						target: "xcm::process_instruction::transact",
						?error_and_info,
						"Dispatch failed"
					);

					executor.transact_status = error_and_info.error.encode().into();
					error_and_info.post_info.actual_weight
				},
			};
		let actual_weight = maybe_actual_weight.unwrap_or(weight);
		let surplus = weight.saturating_sub(actual_weight);
		// If the actual weight of the call was less than the specified weight, we credit it.
		//
		// We make the adjustment for the total surplus, which is used eventually
		// reported back to the caller and this ensures that they account for the total
		// weight consumed correctly (potentially allowing them to do more operations in a
		// block than they otherwise would).
		executor.total_surplus.saturating_accrue(surplus);
		Ok(())
	}
}

impl<Config: config::Config> ExecuteInstruction<Config> for ExportMessage {
	fn execute(self, executor: &mut XcmExecutor<Config>) -> Result<(), XcmError> {
		let ExportMessage { network, destination, xcm } = self;
		// The actual message sent to the bridge for forwarding is prepended with
		// `UniversalOrigin` and `DescendOrigin` in order to ensure that the message is
		// executed with this Origin.
		//
		// Prepend the desired message with instructions which effectively rewrite the
		// origin.
		//
		// This only works because the remote chain empowers the bridge
		// to speak for the local network.
		let origin = executor.context.origin.as_ref().ok_or(XcmError::BadOrigin)?.clone();
		let universal_source = Config::UniversalLocation::get()
			.within_global(origin)
			.map_err(|()| XcmError::Unanchored)?;
		let hash = (executor.origin_ref(), &destination).using_encoded(blake2_128);
		let channel = u32::decode(&mut hash.as_ref()).unwrap_or(0);
		// Hash identifies the lane on the exporter which we use. We use the pairwise
		// combination of the origin and destination to ensure origin/destination pairs
		// will generally have their own lanes.
		let (ticket, fee) = validate_export::<Config::MessageExporter>(
			network,
			channel,
			universal_source,
			destination.clone(),
			xcm,
		)?;
		let old_holding = executor.holding.clone();
		let result = Config::TransactionalProcessor::process(|| {
			executor.take_fee(fee, FeeReason::Export { network, destination })?;
			let _ = Config::MessageExporter::deliver(ticket).defensive_proof(
				"`deliver` called immediately after `validate_export`; \
						`take_fee` does not affect the validity of the ticket; qed",
			);
			Ok(())
		});
		if Config::TransactionalProcessor::IS_TRANSACTIONAL && result.is_err() {
			executor.holding = old_holding;
		}
		result
	}
}
