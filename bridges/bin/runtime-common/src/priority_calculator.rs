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

//! Bridge transaction priority calculator.
//!
//! We aim to prioritize message delivery transactions with more messages over
//! those with fewer messages. That's because we reject delivery transactions
//! containing already delivered messages. If a transaction delivers
//! a single message with nonce `N`, then the transaction with nonces `N..=N+100` will
//! be rejected. This can reduce bridge throughput to one message per block.

use bp_messages::MessageNonce;
use frame_support::traits::Get;
use sp_runtime::transaction_validity::TransactionPriority;

// Reexport everything from the `integrity_tests` module.
#[allow(unused_imports)]
pub use integrity_tests::*;

/// Compute the priority boost for a message delivery transaction that delivers
/// a given number of messages.
pub fn compute_priority_boost<PriorityBoostPerMessage>(
	messages: MessageNonce,
) -> TransactionPriority
where
	PriorityBoostPerMessage: Get<TransactionPriority>,
{
	// We don't want any boost for a transaction with a single message => minus one.
	PriorityBoostPerMessage::get().saturating_mul(messages.saturating_sub(1))
}

#[cfg(not(feature = "integrity-test"))]
mod integrity_tests {}

#[cfg(feature = "integrity-test")]
mod integrity_tests {
	use super::compute_priority_boost;

	use bp_messages::MessageNonce;
	use bp_runtime::PreComputedSize;
	use frame_support::{
		dispatch::{DispatchClass, DispatchInfo, Pays, PostDispatchInfo},
		traits::Get,
	};
	use pallet_bridge_messages::WeightInfoExt;
	use pallet_transaction_payment::OnChargeTransaction;
	use sp_runtime::{
		traits::{Dispatchable, UniqueSaturatedInto, Zero},
		transaction_validity::TransactionPriority,
		FixedPointOperand, SaturatedConversion, Saturating,
	};

	type BalanceOf<T> =
		<<T as pallet_transaction_payment::Config>::OnChargeTransaction as OnChargeTransaction<
			T,
		>>::Balance;

	/// Ensures that the value of `PriorityBoostPerMessage` matches the value of
	/// `tip_boost_per_message`.
	///
	/// We want two transactions, `TX1` with `N` messages and `TX2` with `N+1` messages, to have almost
	/// the same priority if we add `tip_boost_per_message` tip to `TX1`. We aim to ensure
	/// that if we add plain `PriorityBoostPerMessage` priority to `TX1`, the priority will be close
	/// to `TX2` as well.
	pub fn ensure_priority_boost_is_sane<Runtime, MessagesInstance, PriorityBoostPerMessage>(
		tip_boost_per_message: BalanceOf<Runtime>,
	) where
		Runtime:
			pallet_transaction_payment::Config + pallet_bridge_messages::Config<MessagesInstance>,
		MessagesInstance: 'static,
		PriorityBoostPerMessage: Get<TransactionPriority>,
		Runtime::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
		BalanceOf<Runtime>: Send + Sync + FixedPointOperand,
	{
		let priority_boost_per_message = PriorityBoostPerMessage::get();
		let maximal_messages_in_delivery_transaction =
			Runtime::MaxUnconfirmedMessagesAtInboundLane::get();
		for messages in 1..=maximal_messages_in_delivery_transaction {
			let base_priority = estimate_message_delivery_transaction_priority::<
				Runtime,
				MessagesInstance,
			>(messages, Zero::zero());
			let priority_boost = compute_priority_boost::<PriorityBoostPerMessage>(messages);
			let priority_with_boost = base_priority + priority_boost;

			let tip = tip_boost_per_message.saturating_mul((messages - 1).unique_saturated_into());
			let priority_with_tip =
				estimate_message_delivery_transaction_priority::<Runtime, MessagesInstance>(1, tip);

			const ERROR_MARGIN: TransactionPriority = 5; // 5%.
			if priority_with_boost.abs_diff(priority_with_tip).saturating_mul(100) /
				priority_with_tip >
				ERROR_MARGIN
			{
				panic!(
					"The PriorityBoostPerMessage value ({}) must be fixed to: {}",
					priority_boost_per_message,
					compute_priority_boost_per_message::<Runtime, MessagesInstance>(
						tip_boost_per_message
					),
				);
			}
		}
	}

	/// Compute the priority boost that we give to a message delivery transaction for an additional message.
	#[cfg(feature = "integrity-test")]
	fn compute_priority_boost_per_message<Runtime, MessagesInstance>(
		tip_boost_per_message: BalanceOf<Runtime>,
	) -> TransactionPriority
	where
		Runtime:
			pallet_transaction_payment::Config + pallet_bridge_messages::Config<MessagesInstance>,
		MessagesInstance: 'static,
		Runtime::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
		BalanceOf<Runtime>: Send + Sync + FixedPointOperand,
	{
		// Estimate the priority of a transaction that delivers one message and has a large tip.
		let maximal_messages_in_delivery_transaction =
			Runtime::MaxUnconfirmedMessagesAtInboundLane::get();
		let small_with_tip_priority =
			estimate_message_delivery_transaction_priority::<Runtime, MessagesInstance>(
				1,
				tip_boost_per_message
					.saturating_mul(maximal_messages_in_delivery_transaction.saturated_into()),
			);
		// Estimate the priority of a transaction that delivers the maximum number of messages, but has no tip.
		let large_without_tip_priority = estimate_message_delivery_transaction_priority::<
			Runtime,
			MessagesInstance,
		>(maximal_messages_in_delivery_transaction, Zero::zero());

		small_with_tip_priority
			.saturating_sub(large_without_tip_priority)
			.saturating_div(maximal_messages_in_delivery_transaction - 1)
	}

	/// Estimate the priority of a message delivery transaction.
	#[cfg(feature = "integrity-test")]
	fn estimate_message_delivery_transaction_priority<Runtime, MessagesInstance>(
		messages: MessageNonce,
		tip: BalanceOf<Runtime>,
	) -> TransactionPriority
	where
		Runtime:
			pallet_transaction_payment::Config + pallet_bridge_messages::Config<MessagesInstance>,
		MessagesInstance: 'static,
		Runtime::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
		BalanceOf<Runtime>: Send + Sync + FixedPointOperand,
	{
		// An estimation of extra transaction bytes added to every transaction
		// (including signature, signed extensions, and extras + in our case, it includes
		// all call arguments except the proof itself).
		let base_tx_size = 512;
		// Let's say we are relaying similar small messages and for every message, we add more trie
		// nodes to the proof (x0.5 because we expect some nodes to be reused).
		let estimated_message_size = 512;
		// Let's say all our messages have the same dispatch weight.
		let estimated_message_dispatch_weight =
			Runtime::WeightInfo::message_dispatch_weight(estimated_message_size);
		// Message proof argument size is, for every message, message size + some additional
		// trie nodes. Some of them are reused by different messages, so let's take 2/3 of the default
		// "overhead" constant.
		let messages_proof_size = Runtime::WeightInfo::expected_extra_storage_proof_size()
			.saturating_mul(2)
			.saturating_div(3)
			.saturating_add(estimated_message_size)
			.saturating_mul(messages as _);

		// Finally, we can estimate transaction size and weight.
		let transaction_size = base_tx_size.saturating_add(messages_proof_size);
		let transaction_weight = Runtime::WeightInfo::receive_messages_proof_weight(
			&PreComputedSize(transaction_size as _),
			messages as _,
			estimated_message_dispatch_weight.saturating_mul(messages),
		);

		pallet_transaction_payment::ChargeTransactionPayment::<Runtime>::get_priority(
			&DispatchInfo {
				weight: transaction_weight,
				class: DispatchClass::Normal,
				pays_fee: Pays::Yes,
			},
			transaction_size as _,
			tip,
			Zero::zero(),
		)
	}
}
