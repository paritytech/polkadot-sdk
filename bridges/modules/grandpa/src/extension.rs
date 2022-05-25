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

/// Declares a runtime-specific `CheckBridgedBlockNumber` signed extension.
///
/// ## Example
///
/// ```nocompile
/// pallet_bridge_grandpa::declare_check_bridged_block_number_ext!{
///     Runtime,
///     Call::BridgeRialtoGrandpa => RialtoGrandpaInstance,
///     Call::BridgeWestendGrandpa => WestendGrandpaInstance,
/// }
/// ```
#[macro_export]
macro_rules! declare_check_bridged_block_number_ext {
	($runtime:ident, $($call:path => $instance:ty),*) => {
		/// Transaction-with-obsolete-bridged-header check that will reject transaction if
		/// it submits obsolete bridged header.
		#[derive(Clone, codec::Decode, codec::Encode, Eq, PartialEq, frame_support::RuntimeDebug, scale_info::TypeInfo)]
		pub struct CheckBridgedBlockNumber;

		impl sp_runtime::traits::SignedExtension for CheckBridgedBlockNumber {
			const IDENTIFIER: &'static str = "CheckBridgedBlockNumber";
			type AccountId = <$runtime as frame_system::Config>::AccountId;
			type Call = <$runtime as frame_system::Config>::Call;
			type AdditionalSigned = ();
			type Pre = ();

			fn additional_signed(&self) -> sp_std::result::Result<
				(),
				sp_runtime::transaction_validity::TransactionValidityError,
			> {
				Ok(())
			}

			fn validate(
				&self,
				_who: &Self::AccountId,
				call: &Self::Call,
				_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				_len: usize,
			) -> sp_runtime::transaction_validity::TransactionValidity {
				match *call {
					$(
						$call($crate::Call::<$runtime, $instance>::submit_finality_proof { ref finality_target, ..}) => {
							use sp_runtime::traits::Header as HeaderT;

							let bundled_block_number = *finality_target.number();

							let best_finalized_hash = $crate::BestFinalized::<$runtime, $instance>::get();
							let best_finalized_number = match $crate::ImportedHeaders::<
								$runtime,
								$instance,
							>::get(best_finalized_hash) {
								Some(best_finalized_header) => *best_finalized_header.number(),
								None => return sp_runtime::transaction_validity::InvalidTransaction::Call.into(),
							};

							if best_finalized_number < bundled_block_number {
								Ok(sp_runtime::transaction_validity::ValidTransaction::default())
							} else {
								sp_runtime::transaction_validity::InvalidTransaction::Stale.into()
							}
						},
					)*
					_ => Ok(sp_runtime::transaction_validity::ValidTransaction::default()),
				}
			}

			fn pre_dispatch(
				self,
				who: &Self::AccountId,
				call: &Self::Call,
				info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				len: usize,
			) -> Result<Self::Pre, sp_runtime::transaction_validity::TransactionValidityError> {
				self.validate(who, call, info, len).map(drop)
			}

			fn post_dispatch(
				_maybe_pre: Option<Self::Pre>,
				_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
				_post_info: &sp_runtime::traits::PostDispatchInfoOf<Self::Call>,
				_len: usize,
				_result: &sp_runtime::DispatchResult,
			) -> Result<(), sp_runtime::transaction_validity::TransactionValidityError> {
				Ok(())
			}
		}
	};
}

#[cfg(test)]
mod tests {
	use crate::{
		mock::{run_test, test_header, Call, TestNumber, TestRuntime},
		BestFinalized, ImportedHeaders,
	};
	use bp_test_utils::make_default_justification;
	use frame_support::weights::{DispatchClass, DispatchInfo, Pays};
	use sp_runtime::traits::SignedExtension;

	declare_check_bridged_block_number_ext! {
		TestRuntime,
		Call::Grandpa => ()
	}

	fn validate_block_submit(num: TestNumber) -> bool {
		CheckBridgedBlockNumber
			.validate(
				&42,
				&Call::Grandpa(crate::Call::<TestRuntime, ()>::submit_finality_proof {
					finality_target: Box::new(test_header(num)),
					justification: make_default_justification(&test_header(num)),
				}),
				&DispatchInfo { weight: 0, class: DispatchClass::Operational, pays_fee: Pays::Yes },
				0,
			)
			.is_ok()
	}

	fn sync_to_header_10() {
		let header10_hash = sp_core::H256::default();
		BestFinalized::<TestRuntime, ()>::put(header10_hash);
		ImportedHeaders::<TestRuntime, ()>::insert(header10_hash, test_header(10));
	}

	#[test]
	fn check_bridged_block_number_rejects_obsolete_header() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#5 => tx is
			// rejected
			sync_to_header_10();
			assert!(!validate_block_submit(5));
		});
	}

	#[test]
	fn check_bridged_block_number_rejects_same_header() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#10 => tx is
			// rejected
			sync_to_header_10();
			assert!(!validate_block_submit(10));
		});
	}

	#[test]
	fn check_bridged_block_number_accepts_new_header() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#15 => tx is
			// accepted
			sync_to_header_10();
			assert!(validate_block_submit(15));
		});
	}
}
