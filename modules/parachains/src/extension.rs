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

use crate::{Config, Pallet, RelayBlockHash, RelayBlockHasher, RelayBlockNumber};
use bp_runtime::FilterCall;
use frame_support::{dispatch::CallableCallFor, traits::IsSubType};
use sp_runtime::transaction_validity::{InvalidTransaction, TransactionValidity, ValidTransaction};

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
impl<
		Call: IsSubType<CallableCallFor<Pallet<T, I>, T>>,
		T: frame_system::Config<RuntimeCall = Call> + Config<I>,
		I: 'static,
	> FilterCall<Call> for Pallet<T, I>
where
	<T as pallet_bridge_grandpa::Config<T::BridgesGrandpaPalletInstance>>::BridgedChain:
		bp_runtime::Chain<
			BlockNumber = RelayBlockNumber,
			Hash = RelayBlockHash,
			Hasher = RelayBlockHasher,
		>,
{
	fn validate(call: &Call) -> TransactionValidity {
		let (updated_at_relay_block_number, parachains) = match call.is_sub_type() {
			Some(crate::Call::<T, I>::submit_parachain_heads {
				ref at_relay_block,
				ref parachains,
				..
			}) => (at_relay_block.0, parachains),
			_ => return Ok(ValidTransaction::default()),
		};
		let (parachain, parachain_head_hash) = match parachains.as_slice() {
			&[(parachain, parachain_head_hash)] => (parachain, parachain_head_hash),
			_ => return Ok(ValidTransaction::default()),
		};

		let maybe_stored_best_head = crate::ParasInfo::<T, I>::get(parachain);
		let is_valid = Self::validate_updated_parachain_head(
			parachain,
			&maybe_stored_best_head,
			updated_at_relay_block_number,
			parachain_head_hash,
			"Rejecting obsolete parachain-head transaction",
		);

		if is_valid {
			Ok(ValidTransaction::default())
		} else {
			InvalidTransaction::Stale.into()
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		extension::FilterCall,
		mock::{run_test, RuntimeCall, TestRuntime},
		ParaInfo, ParasInfo, RelayBlockNumber,
	};
	use bp_parachains::BestParaHeadHash;
	use bp_polkadot_core::parachains::{ParaHash, ParaHeadsProof, ParaId};

	fn validate_submit_parachain_heads(
		num: RelayBlockNumber,
		parachains: Vec<(ParaId, ParaHash)>,
	) -> bool {
		crate::Pallet::<TestRuntime>::validate(&RuntimeCall::Parachains(crate::Call::<
			TestRuntime,
			(),
		>::submit_parachain_heads {
			at_relay_block: (num, Default::default()),
			parachains,
			parachain_heads_proof: ParaHeadsProof(Vec::new()),
		}))
		.is_ok()
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
}
