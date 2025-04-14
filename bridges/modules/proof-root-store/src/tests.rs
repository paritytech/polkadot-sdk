// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg(test)]

use super::*;
use mock::*;

use codec::Encode;
use frame_support::{assert_ok, BoundedVec};

#[test]
fn note_new_roots_works() {
	run_test(|| {
		assert_ok!(ProofRootStore::note_new_roots(
			RuntimeOrigin::root(),
			BoundedVec::truncate_from(vec![(1, 1), (2, 2),])
		));
		assert_eq!(ProofRootStore::get_root(&1), Some(1));
		assert_eq!(ProofRootStore::get_root(&2), Some(2));
		assert_eq!(ProofRootStore::get_root(&3), None);
		assert_eq!(ProofRootStore::get_root(&4), None);
		assert_eq!(ProofRootStore::get_root(&5), None);
		assert_eq!(ProofRootStore::get_root(&6), None);
		assert_eq!(RootIndex::<TestRuntime>::get().into_iter().collect::<Vec<_>>(), vec![1, 2],);
		assert_ok!(ProofRootStore::do_try_state());

		// `RootsToKeep = 4` rotates
		assert_ok!(ProofRootStore::note_new_roots(
			RuntimeOrigin::root(),
			BoundedVec::truncate_from(vec![(3, 3), (4, 4), (5, 5), (6, 6),])
		));
		assert_eq!(ProofRootStore::get_root(&1), None);
		assert_eq!(ProofRootStore::get_root(&2), None);
		assert_eq!(ProofRootStore::get_root(&3), Some(3));
		assert_eq!(ProofRootStore::get_root(&4), Some(4));
		assert_eq!(ProofRootStore::get_root(&5), Some(5));
		assert_eq!(ProofRootStore::get_root(&6), Some(6));
		assert_eq!(
			RootIndex::<TestRuntime>::get().into_iter().collect::<Vec<_>>(),
			vec![3, 4, 5, 6],
		);
		assert_ok!(ProofRootStore::do_try_state());

		// Add one more
		assert_ok!(ProofRootStore::note_new_roots(
			RuntimeOrigin::root(),
			BoundedVec::truncate_from(vec![(7, 7),])
		));
		assert_eq!(ProofRootStore::get_root(&3), None);
		assert_eq!(ProofRootStore::get_root(&4), Some(4));
		assert_eq!(ProofRootStore::get_root(&5), Some(5));
		assert_eq!(ProofRootStore::get_root(&6), Some(6));
		assert_eq!(ProofRootStore::get_root(&7), Some(7));
		assert_eq!(
			RootIndex::<TestRuntime>::get().into_iter().collect::<Vec<_>>(),
			vec![4, 5, 6, 7],
		);
		assert_ok!(ProofRootStore::do_try_state());
	})
}

#[test]
fn ensure_encoding_compatibility() {
	let roots = vec![(1, 1), (2, 2)];

	assert_eq!(
		bp_proof_root_store::ProofRootStoreCall::note_new_roots { roots: roots.clone() }.encode(),
		Call::<TestRuntime, ()>::note_new_roots { roots: BoundedVec::truncate_from(roots) }
			.encode()
	);
}
