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

use crate::{
	weights::WeightInfo, BridgedBlockNumber, BridgedHeader, Config, CurrentAuthoritySet, Error,
	FreeHeadersRemaining, Pallet,
};
use bp_header_chain::{
	justification::GrandpaJustification, max_expected_submit_finality_proof_arguments_size,
	ChainWithGrandpa, GrandpaConsensusLogReader,
};
use bp_runtime::{BlockNumberOf, Chain, OwnedBridgeModule};
use codec::Encode;
use frame_support::{dispatch::CallableCallFor, traits::IsSubType, weights::Weight};
use sp_consensus_grandpa::SetId;
use sp_runtime::{
	traits::{CheckedSub, Header, One, UniqueSaturatedInto, Zero},
	transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionValidity, ValidTransaction,
		ValidTransactionBuilder,
	},
	RuntimeDebug, SaturatedConversion, Saturating,
};

/// Info about a `SubmitParachainHeads` call which tries to update a single parachain.
#[derive(Copy, Clone, PartialEq, RuntimeDebug)]
pub struct SubmitFinalityProofInfo<N> {
	/// Number of the finality target.
	pub block_number: N,
	/// An identifier of the validators set that has signed the submitted justification.
	/// It might be `None` if deprecated version of the `submit_finality_proof` is used.
	pub current_set_id: Option<SetId>,
	/// If `true`, then the call must be free (assuming that everything else is valid) to
	/// be treated as valid.
	pub is_free_execution_expected: bool,
	/// Extra weight that we assume is included in the call.
	///
	/// We have some assumptions about headers and justifications of the bridged chain.
	/// We know that if our assumptions are correct, then the call must not have the
	/// weight above some limit. The fee paid for weight above that limit, is never refunded.
	pub extra_weight: Weight,
	/// Extra size (in bytes) that we assume are included in the call.
	///
	/// We have some assumptions about headers and justifications of the bridged chain.
	/// We know that if our assumptions are correct, then the call must not have the
	/// weight above some limit. The fee paid for bytes above that limit, is never refunded.
	pub extra_size: u32,
}

impl<N> SubmitFinalityProofInfo<N> {
	/// Returns `true` if call size/weight is below our estimations for regular calls.
	pub fn fits_limits(&self) -> bool {
		self.extra_weight.is_zero() && self.extra_size.is_zero()
	}
}

/// Helper struct that provides methods for working with the `SubmitFinalityProof` call.
pub struct SubmitFinalityProofHelper<T: Config<I>, I: 'static> {
	_phantom_data: sp_std::marker::PhantomData<(T, I)>,
}

impl<T: Config<I>, I: 'static> SubmitFinalityProofHelper<T, I> {
	/// Check that the: (1) GRANDPA head provided by the `SubmitFinalityProof` is better than the
	/// best one we know (2) if `current_set_id` matches the current authority set id, if specified
	/// and (3) whether transaction MAY be free for the submitter if `is_free_execution_expected`
	/// is `true`.
	///
	/// Returns number of headers between the current best finalized header, known to the pallet
	/// and the bundled header.
	pub fn check_obsolete_from_extension(
		call_info: &SubmitFinalityProofInfo<BlockNumberOf<T::BridgedChain>>,
	) -> Result<BlockNumberOf<T::BridgedChain>, Error<T, I>> {
		// do basic checks first
		let improved_by = Self::check_obsolete(call_info.block_number, call_info.current_set_id)?;

		// if submitter has NOT specified that it wants free execution, then we are done
		if !call_info.is_free_execution_expected {
			return Ok(improved_by);
		}

		// else - if we can not accept more free headers, "reject" the transaction
		//
		// `unwrap_or(u32::MAX)` means that if `FreeHeadersRemaining` is `None`, we may accept
		// this header for free. That is a small cheat - is is `None` if executed outside of
		// transaction (e.g. during block initialization). Normal relayer would never submit
		// such calls, but if he did, that is not our problem. During normal transactions,
		// the `FreeHeadersRemaining` is always `Some(_)`.
		let free_headers_remaining = FreeHeadersRemaining::<T, I>::get().unwrap_or(u32::MAX);
		if free_headers_remaining == 0 {
			log::trace!(
				target: crate::LOG_TARGET,
				"Cannot accept free {:?} header {:?}. No more free slots remaining",
				T::BridgedChain::ID,
				call_info.block_number,
			);

			return Err(Error::<T, I>::CannotAcceptMoreFreeHeaders);
		}

		// we do not check whether the header matches free submission criteria here - it is the
		// relayer responsibility to check that

		Ok(improved_by)
	}

	/// Check that the GRANDPA head provided by the `SubmitFinalityProof` is better than the best
	/// one we know. Additionally, checks if `current_set_id` matches the current authority set
	/// id, if specified. This method is called by the call code and the transaction extension,
	/// so it does not check the free execution.
	///
	/// Returns number of headers between the current best finalized header, known to the pallet
	/// and the bundled header.
	pub fn check_obsolete(
		finality_target: BlockNumberOf<T::BridgedChain>,
		current_set_id: Option<SetId>,
	) -> Result<BlockNumberOf<T::BridgedChain>, Error<T, I>> {
		let best_finalized = crate::BestFinalized::<T, I>::get().ok_or_else(|| {
			log::trace!(
				target: crate::LOG_TARGET,
				"Cannot finalize header {:?} because pallet is not yet initialized",
				finality_target,
			);
			<Error<T, I>>::NotInitialized
		})?;

		let improved_by = match finality_target.checked_sub(&best_finalized.number()) {
			Some(improved_by) if improved_by > Zero::zero() => improved_by,
			_ => {
				log::trace!(
					target: crate::LOG_TARGET,
					"Cannot finalize obsolete header: bundled {:?}, best {:?}",
					finality_target,
					best_finalized,
				);

				return Err(Error::<T, I>::OldHeader)
			},
		};

		if let Some(current_set_id) = current_set_id {
			let actual_set_id = <CurrentAuthoritySet<T, I>>::get().set_id;
			if current_set_id != actual_set_id {
				log::trace!(
					target: crate::LOG_TARGET,
					"Cannot finalize header signed by unknown authority set: bundled {:?}, best {:?}",
					current_set_id,
					actual_set_id,
				);

				return Err(Error::<T, I>::InvalidAuthoritySetId)
			}
		}

		Ok(improved_by)
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
	/// Extract finality proof info from a runtime call.
	fn submit_finality_proof_info(
		&self,
	) -> Option<SubmitFinalityProofInfo<BridgedBlockNumber<T, I>>> {
		if let Some(crate::Call::<T, I>::submit_finality_proof { finality_target, justification }) =
			self.is_sub_type()
		{
			return Some(submit_finality_proof_info_from_args::<T, I>(
				finality_target,
				justification,
				None,
				false,
			))
		} else if let Some(crate::Call::<T, I>::submit_finality_proof_ex {
			finality_target,
			justification,
			current_set_id,
			is_free_execution_expected,
		}) = self.is_sub_type()
		{
			return Some(submit_finality_proof_info_from_args::<T, I>(
				finality_target,
				justification,
				Some(*current_set_id),
				*is_free_execution_expected,
			))
		}

		None
	}

	/// Validate Grandpa headers in order to avoid "mining" transactions that provide outdated
	/// bridged chain headers. Without this validation, even honest relayers may lose their funds
	/// if there are multiple relays running and submitting the same information.
	///
	/// It also adds `priority_boost` for every missed header between best finalized header, known
	/// to the pallet and bundled header, staring from the second header. So if
	/// `BestFinalized` header is header number `100` and transaction brings header
	/// `101` there's no priority boost. If transaction brings header `102`, then
	/// priority is boosted by `priority_boost` and so on.
	///
	/// If first item in the tuple is true, then the call is the `submit_finality_proof_info`
	/// (or `submit_finality_proof_info_ex` call) of the associated pallet instance.
	fn check_obsolete_submit_finality_proof(
		&self,
		priority_boost: TransactionPriority,
	) -> (Option<BridgedBlockNumber<T, I>>, TransactionValidity)
	where
		Self: Sized,
	{
		let call_info = match self.submit_finality_proof_info() {
			Some(finality_proof) => finality_proof,
			_ => return (None, Ok(ValidTransaction::default())),
		};

		let block_number = Some(call_info.block_number);
		if Pallet::<T, I>::ensure_not_halted().is_err() {
			return (block_number, InvalidTransaction::Call.into())
		}

		let result = SubmitFinalityProofHelper::<T, I>::check_obsolete_from_extension(&call_info);
		(
			block_number,
			match result {
				Ok(improved_by) => {
					let improved_by: TransactionPriority =
						improved_by.saturating_sub(One::one()).unique_saturated_into();
					let total_priority_boost = improved_by.saturating_mul(priority_boost);
					ValidTransactionBuilder::default().priority(total_priority_boost).build()
				},
				Err(Error::<T, I>::OldHeader) => InvalidTransaction::Stale.into(),
				Err(_) => InvalidTransaction::Call.into(),
			},
		)
	}
}

impl<T: Config<I>, I: 'static> CallSubType<T, I> for T::RuntimeCall where
	T::RuntimeCall: IsSubType<CallableCallFor<Pallet<T, I>, T>>
{
}

/// Extract finality proof info from the submitted header and justification.
pub(crate) fn submit_finality_proof_info_from_args<T: Config<I>, I: 'static>(
	finality_target: &BridgedHeader<T, I>,
	justification: &GrandpaJustification<BridgedHeader<T, I>>,
	current_set_id: Option<SetId>,
	is_free_execution_expected: bool,
) -> SubmitFinalityProofInfo<BridgedBlockNumber<T, I>> {
	let block_number = *finality_target.number();

	// the `submit_finality_proof` call will reject justifications with invalid, duplicate,
	// unknown and extra signatures. It'll also reject justifications with less than necessary
	// signatures. So we do not care about extra weight because of additional signatures here.
	let precommits_len = justification.commit.precommits.len().saturated_into();
	let required_precommits = precommits_len;

	// We do care about extra weight because of more-than-expected headers in the votes
	// ancestries. But we have problems computing extra weight for additional headers (weight of
	// additional header is too small, so that our benchmarks aren't detecting that). So if there
	// are more than expected headers in votes ancestries, we will treat the whole call weight
	// as an extra weight.
	let votes_ancestries_len = justification.votes_ancestries.len().saturated_into();
	let extra_weight =
		if votes_ancestries_len > T::BridgedChain::REASONABLE_HEADERS_IN_JUSTIFICATION_ANCESTRY {
			T::WeightInfo::submit_finality_proof(precommits_len, votes_ancestries_len)
		} else {
			Weight::zero()
		};

	// check if the `finality_target` is a mandatory header. If so, we are ready to refund larger
	// size
	let is_mandatory_finality_target =
		GrandpaConsensusLogReader::<BridgedBlockNumber<T, I>>::find_scheduled_change(
			finality_target.digest(),
		)
		.is_some();

	// we can estimate extra call size easily, without any additional significant overhead
	let actual_call_size: u32 = finality_target
		.encoded_size()
		.saturating_add(justification.encoded_size())
		.saturated_into();
	let max_expected_call_size = max_expected_submit_finality_proof_arguments_size::<T::BridgedChain>(
		is_mandatory_finality_target,
		required_precommits,
	);
	let extra_size = actual_call_size.saturating_sub(max_expected_call_size);

	SubmitFinalityProofInfo {
		block_number,
		current_set_id,
		is_free_execution_expected,
		extra_weight,
		extra_size,
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		call_ext::CallSubType,
		mock::{run_test, test_header, RuntimeCall, TestBridgedChain, TestNumber, TestRuntime},
		BestFinalized, Config, CurrentAuthoritySet, FreeHeadersRemaining, PalletOperatingMode,
		StoredAuthoritySet, SubmitFinalityProofInfo, WeightInfo,
	};
	use bp_header_chain::ChainWithGrandpa;
	use bp_runtime::{BasicOperatingMode, HeaderId};
	use bp_test_utils::{
		make_default_justification, make_justification_for_header, JustificationGeneratorParams,
		TEST_GRANDPA_SET_ID,
	};
	use frame_support::weights::Weight;
	use sp_runtime::{testing::DigestItem, traits::Header as _, SaturatedConversion};

	fn validate_block_submit(num: TestNumber) -> bool {
		let bridge_grandpa_call = crate::Call::<TestRuntime, ()>::submit_finality_proof_ex {
			finality_target: Box::new(test_header(num)),
			justification: make_default_justification(&test_header(num)),
			// not initialized => zero
			current_set_id: 0,
			is_free_execution_expected: false,
		};
		RuntimeCall::check_obsolete_submit_finality_proof(
			&RuntimeCall::Grandpa(bridge_grandpa_call),
			0,
		)
		.1
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
	fn extension_rejects_new_header_if_pallet_is_halted() {
		run_test(|| {
			// when pallet is halted => tx is rejected
			sync_to_header_10();
			PalletOperatingMode::<TestRuntime, ()>::put(BasicOperatingMode::Halted);

			assert!(!validate_block_submit(15));
		});
	}

	#[test]
	fn extension_rejects_new_header_if_set_id_is_invalid() {
		run_test(|| {
			// when set id is different from the passed one => tx is rejected
			sync_to_header_10();
			let next_set = StoredAuthoritySet::<TestRuntime, ()>::try_new(vec![], 0x42).unwrap();
			CurrentAuthoritySet::<TestRuntime, ()>::put(next_set);

			assert!(!validate_block_submit(15));
		});
	}

	#[test]
	fn extension_rejects_new_header_if_free_execution_is_requested_and_free_submissions_are_not_accepted(
	) {
		run_test(|| {
			let bridge_grandpa_call = crate::Call::<TestRuntime, ()>::submit_finality_proof_ex {
				finality_target: Box::new(test_header(15)),
				justification: make_default_justification(&test_header(15)),
				current_set_id: 0,
				is_free_execution_expected: true,
			};
			sync_to_header_10();

			// when we can accept free headers => Ok
			FreeHeadersRemaining::<TestRuntime, ()>::put(2);
			assert!(RuntimeCall::check_obsolete_submit_finality_proof(
				&RuntimeCall::Grandpa(bridge_grandpa_call.clone(),),
				0
			)
			.1
			.is_ok());

			// when we can NOT accept free headers => Err
			FreeHeadersRemaining::<TestRuntime, ()>::put(0);
			assert!(RuntimeCall::check_obsolete_submit_finality_proof(
				&RuntimeCall::Grandpa(bridge_grandpa_call.clone(),),
				0
			)
			.1
			.is_err());

			// when called outside of transaction => Ok
			FreeHeadersRemaining::<TestRuntime, ()>::kill();
			assert!(RuntimeCall::check_obsolete_submit_finality_proof(
				&RuntimeCall::Grandpa(bridge_grandpa_call,),
				0
			)
			.1
			.is_ok());
		})
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

	#[test]
	fn submit_finality_proof_info_is_parsed() {
		// when `submit_finality_proof` is used, `current_set_id` is set to `None`
		let deprecated_call =
			RuntimeCall::Grandpa(crate::Call::<TestRuntime, ()>::submit_finality_proof {
				finality_target: Box::new(test_header(42)),
				justification: make_default_justification(&test_header(42)),
			});
		assert_eq!(
			deprecated_call.submit_finality_proof_info(),
			Some(SubmitFinalityProofInfo {
				block_number: 42,
				current_set_id: None,
				extra_weight: Weight::zero(),
				extra_size: 0,
				is_free_execution_expected: false,
			})
		);

		// when `submit_finality_proof_ex` is used, `current_set_id` is set to `Some`
		let deprecated_call =
			RuntimeCall::Grandpa(crate::Call::<TestRuntime, ()>::submit_finality_proof_ex {
				finality_target: Box::new(test_header(42)),
				justification: make_default_justification(&test_header(42)),
				current_set_id: 777,
				is_free_execution_expected: false,
			});
		assert_eq!(
			deprecated_call.submit_finality_proof_info(),
			Some(SubmitFinalityProofInfo {
				block_number: 42,
				current_set_id: Some(777),
				extra_weight: Weight::zero(),
				extra_size: 0,
				is_free_execution_expected: false,
			})
		);
	}

	#[test]
	fn extension_returns_correct_extra_size_if_call_arguments_are_too_large() {
		// when call arguments are below our limit => no refund
		let small_finality_target = test_header(1);
		let justification_params = JustificationGeneratorParams {
			header: small_finality_target.clone(),
			..Default::default()
		};
		let small_justification = make_justification_for_header(justification_params);
		let small_call = RuntimeCall::Grandpa(crate::Call::submit_finality_proof_ex {
			finality_target: Box::new(small_finality_target),
			justification: small_justification,
			current_set_id: TEST_GRANDPA_SET_ID,
			is_free_execution_expected: false,
		});
		assert_eq!(small_call.submit_finality_proof_info().unwrap().extra_size, 0);

		// when call arguments are too large => partial refund
		let mut large_finality_target = test_header(1);
		large_finality_target
			.digest_mut()
			.push(DigestItem::Other(vec![42u8; 1024 * 1024]));
		let justification_params = JustificationGeneratorParams {
			header: large_finality_target.clone(),
			..Default::default()
		};
		let large_justification = make_justification_for_header(justification_params);
		let large_call = RuntimeCall::Grandpa(crate::Call::submit_finality_proof_ex {
			finality_target: Box::new(large_finality_target),
			justification: large_justification,
			current_set_id: TEST_GRANDPA_SET_ID,
			is_free_execution_expected: false,
		});
		assert_ne!(large_call.submit_finality_proof_info().unwrap().extra_size, 0);
	}

	#[test]
	fn extension_returns_correct_extra_weight_if_there_are_too_many_headers_in_votes_ancestry() {
		let finality_target = test_header(1);
		let mut justification_params = JustificationGeneratorParams {
			header: finality_target.clone(),
			ancestors: TestBridgedChain::REASONABLE_HEADERS_IN_JUSTIFICATION_ANCESTRY,
			..Default::default()
		};

		// when there are `REASONABLE_HEADERS_IN_JUSTIFICATION_ANCESTRY` headers => no refund
		let justification = make_justification_for_header(justification_params.clone());
		let call = RuntimeCall::Grandpa(crate::Call::submit_finality_proof_ex {
			finality_target: Box::new(finality_target.clone()),
			justification,
			current_set_id: TEST_GRANDPA_SET_ID,
			is_free_execution_expected: false,
		});
		assert_eq!(call.submit_finality_proof_info().unwrap().extra_weight, Weight::zero());

		// when there are `REASONABLE_HEADERS_IN_JUSTIFICATION_ANCESTRY + 1` headers => full refund
		justification_params.ancestors += 1;
		let justification = make_justification_for_header(justification_params);
		let call_weight = <TestRuntime as Config>::WeightInfo::submit_finality_proof(
			justification.commit.precommits.len().saturated_into(),
			justification.votes_ancestries.len().saturated_into(),
		);
		let call = RuntimeCall::Grandpa(crate::Call::submit_finality_proof_ex {
			finality_target: Box::new(finality_target),
			justification,
			current_set_id: TEST_GRANDPA_SET_ID,
			is_free_execution_expected: false,
		});
		assert_eq!(call.submit_finality_proof_info().unwrap().extra_weight, call_weight);
	}

	#[test]
	fn check_obsolete_submit_finality_proof_boosts_priority() {
		run_test(|| {
			fn make_call(number: u64) -> RuntimeCall {
				RuntimeCall::Grandpa(crate::Call::<TestRuntime, ()>::submit_finality_proof_ex {
					finality_target: Box::new(test_header(number)),
					justification: make_default_justification(&test_header(number)),
					current_set_id: 0,
					is_free_execution_expected: true,
				})
			}

			// when priority boost is zero, total boost is also zero
			sync_to_header_10();
			let result = RuntimeCall::check_obsolete_submit_finality_proof(&make_call(15), 0);
			assert_eq!(result.0, Some(15));
			assert_eq!(result.1.unwrap().priority, 0,);

			// when the difference between headers is 1, no boost
			assert_eq!(
				RuntimeCall::check_obsolete_submit_finality_proof(&make_call(11), 100)
					.1
					.unwrap()
					.priority,
				0,
			);

			// when the difference between headers is 2 => boost
			assert_eq!(
				RuntimeCall::check_obsolete_submit_finality_proof(&make_call(12), 100)
					.1
					.unwrap()
					.priority,
				100,
			);

			// when the difference between headers is 3 => 2 * boost
			assert_eq!(
				RuntimeCall::check_obsolete_submit_finality_proof(&make_call(13), 100)
					.1
					.unwrap()
					.priority,
				200,
			);
		})
	}

	#[test]
	fn check_obsolete_submit_finality_proof_ignores_other_calls() {
		run_test(|| {
			let call =
				RuntimeCall::System(frame_system::Call::<TestRuntime>::remark { remark: vec![42] });

			assert_eq!(RuntimeCall::check_obsolete_submit_finality_proof(&call, 0).0, None);
		})
	}
}
