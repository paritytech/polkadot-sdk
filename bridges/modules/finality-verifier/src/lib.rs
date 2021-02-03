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

//! Substrate Finality Verifier Pallet
//!
//! The goal of this pallet is to provide a safe interface for writing finalized headers to an
//! external pallet which tracks headers and finality proofs. By safe, we mean that only headers
//! whose finality has been verified will be written to the underlying pallet.
//!
//! By verifying the finality of headers before writing them to storage we prevent DoS vectors in
//! which unfinalized headers get written to storage even if they don't have a chance of being
//! finalized in the future (such as in the case where a different fork gets finalized).
//!
//! The underlying pallet used for storage is assumed to be a pallet which tracks headers and
//! GRANDPA authority set changes. This information is used during the verification of GRANDPA
//! finality proofs.

#![cfg_attr(not(feature = "std"), no_std)]
// Runtime-generated enums
#![allow(clippy::large_enum_variant)]

use bp_header_chain::{justification::verify_justification, AncestryChecker, HeaderChain};
use bp_runtime::{Chain, HeaderOf};
use finality_grandpa::voter_set::VoterSet;
use frame_support::{dispatch::DispatchError, ensure, traits::Get};
use frame_system::ensure_signed;
use num_traits::AsPrimitive;
use sp_runtime::traits::Header as HeaderT;
use sp_std::vec::Vec;

#[cfg(test)]
mod mock;

// Re-export in crate namespace for `construct_runtime!`
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// Header of the bridged chain.
	pub(crate) type BridgedHeader<T> = HeaderOf<<T as Config>::BridgedChain>;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The chain we are bridging to here.
		type BridgedChain: Chain;

		/// The pallet which we will use as our underlying storage mechanism.
		type HeaderChain: HeaderChain<<Self::BridgedChain as Chain>::Header, DispatchError>;

		/// The type through which we will verify that a given header is related to the last
		/// finalized header in our storage pallet.
		type AncestryChecker: AncestryChecker<
			<Self::BridgedChain as Chain>::Header,
			Vec<<Self::BridgedChain as Chain>::Header>,
		>;

		/// The maximum length of headers we can have in a single ancestry proof. This prevents
		/// unbounded iteration when verifying proofs.
		#[pallet::constant]
		type MaxHeadersInSingleProof: Get<<Self::BridgedChain as Chain>::BlockNumber>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Verify a target header is finalized according to the given finality proof.
		///
		/// Will use the underlying storage pallet to fetch information about the current
		/// authorities and best finalized header in order to verify that the header is finalized.
		///
		/// If successful in verification, it will write the header as well as its ancestors (from
		/// the given `ancestry_proof`) to the underlying storage pallet.
		///
		/// Note that the expected format for `ancestry_proof` is a continguous list of finalized
		/// headers containing (current_best_finalized_header, finality_target]
		#[pallet::weight(0)]
		pub fn submit_finality_proof(
			origin: OriginFor<T>,
			finality_target: BridgedHeader<T>,
			justification: Vec<u8>,
			ancestry_proof: Vec<BridgedHeader<T>>,
		) -> DispatchResultWithPostInfo {
			let _ = ensure_signed(origin)?;

			ensure!(
				ancestry_proof.len() <= T::MaxHeadersInSingleProof::get().as_(),
				<Error<T>>::OversizedAncestryProof
			);

			let authority_set = T::HeaderChain::authority_set();
			let voter_set = VoterSet::new(authority_set.authorities).ok_or(<Error<T>>::InvalidAuthoritySet)?;
			let set_id = authority_set.set_id;

			verify_justification::<BridgedHeader<T>>(
				(finality_target.hash(), *finality_target.number()),
				set_id,
				voter_set,
				&justification,
			)
			.map_err(|_| <Error<T>>::InvalidJustification)?;

			let best_finalized = T::HeaderChain::best_finalized();
			ensure!(
				T::AncestryChecker::are_ancestors(&best_finalized, &finality_target, &ancestry_proof),
				<Error<T>>::InvalidAncestryProof
			);

			// Note that this won't work if we ever change the `ancestry_proof` format to be
			// sparse since this expects a contiguous set of finalized headers.
			let _ =
				T::HeaderChain::append_finalized_chain(ancestry_proof).map_err(|_| <Error<T>>::FailedToWriteHeader)?;

			Ok(().into())
		}
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The given justification is invalid for the given header.
		InvalidJustification,
		/// The given ancestry proof is unable to verify that the child and ancestor headers are
		/// related.
		InvalidAncestryProof,
		/// The authority set from the underlying header chain is invalid.
		InvalidAuthoritySet,
		/// Failed to write a header to the underlying header chain.
		FailedToWriteHeader,
		/// The given ancestry proof is too large to be verified in a single transaction.
		OversizedAncestryProof,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{run_test, test_header, Origin, TestRuntime};
	use bp_test_utils::{authority_list, make_justification_for_header};
	use codec::Encode;
	use frame_support::{assert_err, assert_ok};

	fn initialize_substrate_bridge() {
		let genesis = test_header(0);

		let init_data = pallet_substrate_bridge::InitializationData {
			header: genesis,
			authority_list: authority_list(),
			set_id: 1,
			scheduled_change: None,
			is_halted: false,
		};

		assert_ok!(pallet_substrate_bridge::Module::<TestRuntime>::initialize(
			Origin::root(),
			init_data
		));
	}

	#[test]
	fn succesfully_imports_header_with_valid_finality_and_ancestry_proofs() {
		run_test(|| {
			initialize_substrate_bridge();

			let child = test_header(1);
			let header = test_header(2);

			let set_id = 1;
			let grandpa_round = 1;
			let justification =
				make_justification_for_header(&header, grandpa_round, set_id, &authority_list()).encode();
			let ancestry_proof = vec![child, header.clone()];

			assert_ok!(Module::<TestRuntime>::submit_finality_proof(
				Origin::signed(1),
				header.clone(),
				justification,
				ancestry_proof,
			));

			assert_eq!(
				pallet_substrate_bridge::Module::<TestRuntime>::best_headers(),
				vec![(*header.number(), header.hash())]
			);

			assert_eq!(pallet_substrate_bridge::Module::<TestRuntime>::best_finalized(), header);
		})
	}

	#[test]
	fn rejects_justification_that_skips_authority_set_transition() {
		run_test(|| {
			initialize_substrate_bridge();

			let child = test_header(1);
			let header = test_header(2);

			let set_id = 2;
			let grandpa_round = 1;
			let justification =
				make_justification_for_header(&header, grandpa_round, set_id, &authority_list()).encode();
			let ancestry_proof = vec![child, header.clone()];

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification, ancestry_proof,),
				<Error<TestRuntime>>::InvalidJustification
			);
		})
	}

	#[test]
	fn does_not_import_header_with_invalid_finality_proof() {
		run_test(|| {
			initialize_substrate_bridge();

			let child = test_header(1);
			let header = test_header(2);

			let justification = [1u8; 32].encode();
			let ancestry_proof = vec![child, header.clone()];

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification, ancestry_proof,),
				<Error<TestRuntime>>::InvalidJustification
			);
		})
	}

	#[test]
	fn does_not_import_header_with_invalid_ancestry_proof() {
		run_test(|| {
			initialize_substrate_bridge();

			let header = test_header(2);

			let set_id = 1;
			let grandpa_round = 1;
			let justification =
				make_justification_for_header(&header, grandpa_round, set_id, &authority_list()).encode();

			// For testing, we've made it so that an empty ancestry proof is invalid
			let ancestry_proof = vec![];

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification, ancestry_proof,),
				<Error<TestRuntime>>::InvalidAncestryProof
			);
		})
	}

	#[test]
	fn disallows_ancestry_proofs_which_are_too_large() {
		run_test(|| {
			initialize_substrate_bridge();

			let header = test_header(1);
			let justification = [1u8; 32].encode();

			let mut ancestry_proof = vec![];
			let max_len = <TestRuntime as Config>::MaxHeadersInSingleProof::get();
			for i in 1..=max_len + 1 {
				ancestry_proof.push(test_header(i as u64));
			}

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification, ancestry_proof,),
				<Error<TestRuntime>>::OversizedAncestryProof
			);
		})
	}

	#[test]
	fn disallows_invalid_authority_set() {
		run_test(|| {
			use bp_test_utils::{alice, bob};

			let genesis = test_header(0);

			let invalid_authority_list = vec![(alice(), u64::MAX), (bob(), u64::MAX)];
			let init_data = pallet_substrate_bridge::InitializationData {
				header: genesis,
				authority_list: invalid_authority_list,
				set_id: 1,
				scheduled_change: None,
				is_halted: false,
			};

			assert_ok!(pallet_substrate_bridge::Module::<TestRuntime>::initialize(
				Origin::root(),
				init_data
			));

			let header = test_header(1);
			let justification = [1u8; 32].encode();
			let ancestry_proof = vec![];

			assert_err!(
				Module::<TestRuntime>::submit_finality_proof(Origin::signed(1), header, justification, ancestry_proof,),
				<Error<TestRuntime>>::InvalidAuthoritySet
			);
		})
	}
}
