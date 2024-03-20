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
	Config, GrandpaPalletOf, Pallet, ParachainHeadsUpdateFilter, RelayBlockHash, RelayBlockNumber,
};
use bp_header_chain::HeaderChain;
use bp_parachains::BestParaHeadHash;
use bp_polkadot_core::parachains::{ParaHash, ParaId};
use bp_runtime::OwnedBridgeModule;
use frame_support::{dispatch::CallableCallFor, traits::IsSubType};
use sp_runtime::{
	transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction},
	RuntimeDebug,
};

/// Info about a `SubmitParachainHeads` call which tries to update a single parachain.
#[derive(PartialEq, RuntimeDebug)]
pub struct SubmitParachainHeadsInfo {
	/// Number and hash of the finalized relay block that has been used to prove parachain
	/// finality.
	pub at_relay_block: (RelayBlockNumber, RelayBlockHash),
	/// Parachain identifier.
	pub para_id: ParaId,
	/// Hash of the bundled parachain head.
	pub para_head_hash: ParaHash,
}

/// Helper struct that provides methods for working with the `SubmitParachainHeads` call.
pub struct SubmitParachainHeadsHelper<T: Config<I>, I: 'static> {
	_phantom_data: sp_std::marker::PhantomData<(T, I)>,
}

impl<T: Config<I>, I: 'static> SubmitParachainHeadsHelper<T, I> {
	/// Check if the para head provided by the `SubmitParachainHeads` is better than the best one
	/// we know.
	pub fn is_obsolete(update: &SubmitParachainHeadsInfo) -> bool {
		// check if we know better parachain head already
		let is_free_execution_expected = match crate::ParasInfo::<T, I>::get(update.para_id) {
			Some(stored_best_head) => {
				if stored_best_head.best_head_hash.at_relay_block_number >= update.at_relay_block.0
				{
					log::trace!(
						target: crate::LOG_TARGET,
						"The parachain head can't be updated. The parachain head for {:?} \
							was already updated at better relay chain block {} >= {}.",
						update.para_id,
						stored_best_head.best_head_hash.at_relay_block_number,
						update.at_relay_block.0
					);
					return true
				}

				if stored_best_head.best_head_hash.head_hash == update.para_head_hash {
					log::trace!(
						target: crate::LOG_TARGET,
						"The parachain head can't be updated. The parachain head hash for {:?} \
						was already updated to {} at block {} < {}.",
						update.para_id,
						update.para_head_hash,
						stored_best_head.best_head_hash.at_relay_block_number,
						update.at_relay_block.0
					);
					return true
				}

				T::FreeHeadsUpdateFilter::is_free(
					update.at_relay_block,
					&[(update.para_id, update.para_head_hash)],
				)
			},
			None => true,
		};

		// the relayer may expect free execution, so let's double check if our chain has no
		// reorgs and we still know the relay chain header used to craft the proof
		if is_free_execution_expected {
			if GrandpaPalletOf::<T, I>::finalized_header_state_root(update.at_relay_block.1)
				.is_none()
			{
				log::trace!(
					target: crate::LOG_TARGET,
					"The parachain {:?} head can't be updated. Relay chain header {}/{} used to create \
					parachain proof is missing from the storage.",
					update.para_id,
					update.at_relay_block.0,
					update.at_relay_block.1,
				);

				return true
			}
		}

		false
	}

	/// Check if the `SubmitParachainHeads` was successfully executed.
	pub fn was_successful(update: &SubmitParachainHeadsInfo) -> bool {
		match crate::ParasInfo::<T, I>::get(update.para_id) {
			Some(stored_best_head) =>
				stored_best_head.best_head_hash ==
					BestParaHeadHash {
						at_relay_block_number: update.at_relay_block.0,
						head_hash: update.para_head_hash,
					},
			None => false,
		}
	}
}

/// Trait representing a call that is a sub type of this pallet's call.
pub trait CallSubType<T: Config<I, RuntimeCall = Self>, I: 'static>:
	IsSubType<CallableCallFor<Pallet<T, I>, T>>
{
	/// Create a new instance of `SubmitParachainHeadsInfo` from a `SubmitParachainHeads` call with
	/// one single parachain entry.
	fn one_entry_submit_parachain_heads_info(&self) -> Option<SubmitParachainHeadsInfo> {
		if let Some(crate::Call::<T, I>::submit_parachain_heads {
			ref at_relay_block,
			ref parachains,
			..
		}) = self.is_sub_type()
		{
			if let &[(para_id, para_head_hash)] = parachains.as_slice() {
				return Some(SubmitParachainHeadsInfo {
					at_relay_block: *at_relay_block,
					para_id,
					para_head_hash,
				})
			}
		}

		None
	}

	/// Create a new instance of `SubmitParachainHeadsInfo` from a `SubmitParachainHeads` call with
	/// one single parachain entry, if the entry is for the provided parachain id.
	fn submit_parachain_heads_info_for(&self, para_id: u32) -> Option<SubmitParachainHeadsInfo> {
		self.one_entry_submit_parachain_heads_info()
			.filter(|update| update.para_id.0 == para_id)
	}

	/// Validate parachain heads in order to avoid "mining" transactions that provide
	/// outdated bridged parachain heads. Without this validation, even honest relayers
	/// may lose their funds if there are multiple relays running and submitting the
	/// same information.
	///
	/// This validation only works with transactions that are updating single parachain
	/// head. We can't use unbounded validation - it may take too long and either break
	/// block production, or "eat" significant portion of block production time literally
	/// for nothing. In addition, the single-parachain-head-per-transaction is how the
	/// pallet will be used in our environment.
	fn check_obsolete_submit_parachain_heads(&self) -> TransactionValidity
	where
		Self: Sized,
	{
		let update = match self.one_entry_submit_parachain_heads_info() {
			Some(update) => update,
			None => return Ok(ValidTransaction::default()),
		};

		if Pallet::<T, I>::ensure_not_halted().is_err() {
			return InvalidTransaction::Call.into()
		}

		if SubmitParachainHeadsHelper::<T, I>::is_obsolete(&update) {
			return InvalidTransaction::Stale.into()
		}

		Ok(ValidTransaction::default())
	}
}

impl<T, I: 'static> CallSubType<T, I> for T::RuntimeCall
where
	T: Config<I>,
	T::RuntimeCall: IsSubType<CallableCallFor<Pallet<T, I>, T>>,
{
}

#[cfg(test)]
mod tests {
	use crate::{
		mock::{run_test, RuntimeCall, TestRuntime},
		CallSubType, PalletOperatingMode, ParaInfo, ParasInfo, RelayBlockHash, RelayBlockNumber,
	};
	use bp_header_chain::StoredHeaderData;
	use bp_parachains::BestParaHeadHash;
	use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};
	use bp_runtime::BasicOperatingMode;

	fn validate_submit_parachain_heads(
		num: RelayBlockNumber,
		parachains: Vec<(ParaId, ParaHash)>,
	) -> bool {
		RuntimeCall::Parachains(crate::Call::<TestRuntime, ()>::submit_parachain_heads {
			at_relay_block: (num, [num as u8; 32].into()),
			parachains,
			parachain_heads_proof: ParaHeadsProof { storage_proof: Vec::new() },
		})
		.check_obsolete_submit_parachain_heads()
		.is_ok()
	}

	fn insert_relay_block(num: RelayBlockNumber) {
		pallet_bridge_grandpa::ImportedHeaders::<TestRuntime, crate::Instance1>::insert(
			RelayBlockHash::from([num as u8; 32]),
			StoredHeaderData { number: num, state_root: RelayBlockHash::from([10u8; 32]) },
		);
	}

	fn sync_to_relay_header_10() {
		ParasInfo::<TestRuntime, ()>::insert(
			ParaId(1),
			ParaInfo {
				best_head_hash: BestParaHeadHash {
					at_relay_block_number: 10,
					head_hash: [1u8; 32].into(),
				},
				next_imported_hash_position: 0,
			},
		);
	}

	#[test]
	fn extension_rejects_header_from_the_obsolete_relay_block() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#5 => tx is
			// rejected
			sync_to_relay_header_10();
			assert!(!validate_submit_parachain_heads(5, vec![(ParaId(1), [1u8; 32].into())]));
		});
	}

	#[test]
	fn extension_rejects_header_from_the_same_relay_block() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#10 => tx is
			// rejected
			sync_to_relay_header_10();
			assert!(!validate_submit_parachain_heads(10, vec![(ParaId(1), [1u8; 32].into())]));
		});
	}

	#[test]
	fn extension_rejects_header_from_new_relay_block_with_same_hash() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#10 => tx is
			// rejected
			sync_to_relay_header_10();
			assert!(!validate_submit_parachain_heads(20, vec![(ParaId(1), [1u8; 32].into())]));
		});
	}

	#[test]
	fn extension_rejects_header_if_pallet_is_halted() {
		run_test(|| {
			// when pallet is halted => tx is rejected
			sync_to_relay_header_10();
			PalletOperatingMode::<TestRuntime, ()>::put(BasicOperatingMode::Halted);

			assert!(!validate_submit_parachain_heads(15, vec![(ParaId(1), [2u8; 32].into())]));
		});
	}

	#[test]
	fn extension_accepts_new_header() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#15 => tx is
			// accepted
			sync_to_relay_header_10();
			assert!(validate_submit_parachain_heads(15, vec![(ParaId(1), [2u8; 32].into())]));
		});
	}

	#[test]
	fn extension_accepts_if_more_than_one_parachain_is_submitted() {
		run_test(|| {
			// when current best finalized is #10 and we're trying to import header#5, but another
			// parachain head is also supplied => tx is accepted
			sync_to_relay_header_10();
			assert!(validate_submit_parachain_heads(
				5,
				vec![(ParaId(1), [1u8; 32].into()), (ParaId(2), [1u8; 32].into())]
			));
		});
	}

	#[test]
	fn extension_rejects_initial_parachain_head_if_missing_relay_chain_header() {
		run_test(|| {
			// when relay chain header is unknown => "obsolete"
			assert!(!validate_submit_parachain_heads(10, vec![(ParaId(1), [1u8; 32].into())]));
			// when relay chain header is unknown => "ok"
			insert_relay_block(10);
			assert!(validate_submit_parachain_heads(10, vec![(ParaId(1), [1u8; 32].into())]));
		});
	}

	#[test]
	fn extension_rejects_free_parachain_head_if_missing_relay_chain_header() {
		run_test(|| {
			sync_to_relay_header_10();
			// when relay chain header is unknown => "obsolete"
			assert!(!validate_submit_parachain_heads(15, vec![(ParaId(2), [15u8; 32].into())]));
			// when relay chain header is unknown => "ok"
			insert_relay_block(15);
			assert!(validate_submit_parachain_heads(15, vec![(ParaId(2), [15u8; 32].into())]));
		});
	}
}
