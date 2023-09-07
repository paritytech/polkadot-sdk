// Copyright 2021 Parity Technologies (UK) Ltd.
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

use frame_support::{
	dispatch::DispatchError,
	storage::{with_transaction, TransactionOutcome},
};
use xcm::latest::prelude::*;
use xcm_executor::traits::ProcessTransaction;

/// Transactional processor implementation using frame transactional layers.
pub struct FrameTransactionalProcessor;
impl ProcessTransaction for FrameTransactionalProcessor {
	const IS_TRANSACTIONAL: bool = true;

	fn process<F>(f: F) -> Result<(), XcmError>
	where
		F: FnOnce() -> Result<(), XcmError>,
	{
		let transaction_outcome =
			with_transaction(|| -> TransactionOutcome<Result<_, DispatchError>> {
				let output = f();
				match &output {
					// If `f` was successful, we commit the transactional layer.
					Ok(()) => TransactionOutcome::Commit(Ok(output)),
					// Else we roll back the changes.
					_ => TransactionOutcome::Rollback(Ok(output)),
				}
			});

		let output = match transaction_outcome {
			// `with_transactional` executed successfully, and we have the expected output.
			Ok(output) => output,
			// `with_transactional` returned an error because the `TRANSACTIONAL_LIMIT` was
			// reached. We describe that error with `XcmError::ExceedsStackLimit`.
			Err(_) => Err(XcmError::ExceedsStackLimit),
		};

		output
	}
}
