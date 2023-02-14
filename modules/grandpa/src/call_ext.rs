// Copyright 2021 Parity Technologies (UK) Ltd.
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

use crate::{Config, Error, Pallet};
use bp_runtime::BlockNumberOf;
use frame_support::{dispatch::CallableCallFor, traits::IsSubType};
use sp_runtime::{
	traits::Header,
	transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
};

/// Helper struct that provides methods for working with the `SubmitFinalityProof` call.
pub struct SubmitFinalityProofHelper<T: Config<I>, I: 'static> {
	pub _phantom_data: sp_std::marker::PhantomData<(T, I)>,
}

impl<T: Config<I>, I: 'static> SubmitFinalityProofHelper<T, I> {
	/// Check that the GRANDPA head provided by the `SubmitFinalityProof` is better than the best
	/// one we know.
	pub fn check_obsolete(
		finality_target: BlockNumberOf<T::BridgedChain>,
	) -> Result<(), Error<T, I>> {
		let best_finalized = crate::BestFinalized::<T, I>::get().ok_or_else(|| {
			log::trace!(
				target: crate::LOG_TARGET,
				"Cannot finalize header {:?} because pallet is not yet initialized",
				finality_target,
			);
			<Error<T, I>>::NotInitialized
		})?;

		if best_finalized.number() >= finality_target {
			log::trace!(
				target: crate::LOG_TARGET,
				"Cannot finalize obsolete header: bundled {:?}, best {:?}",
				finality_target,
				best_finalized,
			);

			return Err(Error::<T, I>::OldHeader)
		}

		Ok(())
	}

	/// Check if the `SubmitFinalityProof` was successfully executed.
	pub fn was_successful(finality_target: BlockNumberOf<T::BridgedChain>) -> bool {
		match crate::BestFinalized::<T, I>::get() {
			Some(best_finalized) => best_finalized.number() == finality_target,
			None => false,
		}
	}
}

/// Trait representing a call that is a sub type of this pallet's call.
pub trait CallSubType<T: Config<I, RuntimeCall = Self>, I: 'static>:
	IsSubType<CallableCallFor<Pallet<T, I>, T>>
{
	/// Extract the finality target from a `SubmitParachainHeads` call.
	fn submit_finality_proof_info(&self) -> Option<BlockNumberOf<T::BridgedChain>> {
		if let Some(crate::Call::<T, I>::submit_finality_proof { finality_target, .. }) =
			self.is_sub_type()
		{
			return Some(*finality_target.number())
		}

		None
	}

	/// Validate Grandpa headers in order to avoid "mining" transactions that provide outdated
	/// bridged chain headers. Without this validation, even honest relayers may lose their funds
	/// if there are multiple relays running and submitting the same information.
	fn check_obsolete_submit_finality_proof(&self) -> TransactionValidity
	where
		Self: Sized,
	{
		let finality_target = match self.submit_finality_proof_info() {
			Some(finality_proof) => finality_proof,
			_ => return Ok(ValidTransaction::default()),
		};

		match SubmitFinalityProofHelper::<T, I>::check_obsolete(finality_target) {
			Ok(_) => Ok(ValidTransaction::default()),
			Err(Error::<T, I>::OldHeader) => InvalidTransaction::Stale.into(),
			Err(_) => InvalidTransaction::Call.into(),
		}
	}
}

impl<T: Config<I>, I: 'static> CallSubType<T, I> for T::RuntimeCall where
	T::RuntimeCall: IsSubType<CallableCallFor<Pallet<T, I>, T>>
{
}

#[cfg(test)]
mod tests {
	use crate::{
		call_ext::CallSubType,
		mock::{run_test, test_header, RuntimeCall, TestNumber, TestRuntime},
		BestFinalized,
	};
	use bp_runtime::HeaderId;
	use bp_test_utils::make_default_justification;

	fn validate_block_submit(num: TestNumber) -> bool {
		let bridge_grandpa_call = crate::Call::<TestRuntime, ()>::submit_finality_proof {
			finality_target: Box::new(test_header(num)),
			justification: make_default_justification(&test_header(num)),
		};
		RuntimeCall::check_obsolete_submit_finality_proof(&RuntimeCall::Grandpa(
			bridge_grandpa_call,
		))
		.is_ok()
	}

	fn sync_to_header_10() {
		let header10_hash = sp_core::H256::default();
		BestFinalized::<TestRuntime, ()>::put(HeaderId(10, header10_hash));
	}

	#[test]
	fn extension_rejects_obsolete_header() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#5 => tx is
			// rejected
			sync_to_header_10();
			assert!(!validate_block_submit(5));
		});
	}

	#[test]
	fn extension_rejects_same_header() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#10 => tx is
			// rejected
			sync_to_header_10();
			assert!(!validate_block_submit(10));
		});
	}

	#[test]
	fn extension_accepts_new_header() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#15 => tx is
			// accepted
			sync_to_header_10();
			assert!(validate_block_submit(15));
		});
	}
}
