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

use crate::{Config, Pallet};
use bp_runtime::FilterCall;
use frame_support::{dispatch::CallableCallFor, traits::IsSubType};
use sp_runtime::{
	traits::Header,
	transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
};

/// Validate Grandpa headers in order to avoid "mining" transactions that provide outdated
/// bridged chain headers. Without this validation, even honest relayers may lose their funds
/// if there are multiple relays running and submitting the same information.
impl<
		Call: IsSubType<CallableCallFor<Pallet<T, I>, T>>,
		T: frame_system::Config<RuntimeCall = Call> + Config<I>,
		I: 'static,
	> FilterCall<Call> for Pallet<T, I>
{
	fn validate(call: &<T as frame_system::Config>::RuntimeCall) -> TransactionValidity {
		let bundled_block_number = match call.is_sub_type() {
			Some(crate::Call::<T, I>::submit_finality_proof { ref finality_target, .. }) =>
				*finality_target.number(),
			_ => return Ok(ValidTransaction::default()),
		};

		let best_finalized = crate::BestFinalized::<T, I>::get();
		let best_finalized_number = match best_finalized {
			Some((best_finalized_number, _)) => best_finalized_number,
			None => return InvalidTransaction::Call.into(),
		};

		if best_finalized_number >= bundled_block_number {
			log::trace!(
				target: crate::LOG_TARGET,
				"Rejecting obsolete bridged header: bundled {:?}, best {:?}",
				bundled_block_number,
				best_finalized_number,
			);

			return InvalidTransaction::Stale.into()
		}

		Ok(ValidTransaction::default())
	}
}

#[cfg(test)]
mod tests {
	use super::FilterCall;
	use crate::{
		mock::{run_test, test_header, RuntimeCall, TestNumber, TestRuntime},
		BestFinalized,
	};
	use bp_test_utils::make_default_justification;

	fn validate_block_submit(num: TestNumber) -> bool {
		crate::Pallet::<TestRuntime>::validate(&RuntimeCall::Grandpa(crate::Call::<
			TestRuntime,
			(),
		>::submit_finality_proof {
			finality_target: Box::new(test_header(num)),
			justification: make_default_justification(&test_header(num)),
		}))
		.is_ok()
	}

	fn sync_to_header_10() {
		let header10_hash = sp_core::H256::default();
		BestFinalized::<TestRuntime, ()>::put((10, header10_hash));
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
