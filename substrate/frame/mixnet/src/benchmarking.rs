// This file is part of Substrate.

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

//! Benchmarking for `pallet-mixnet`.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame::{
	benchmarking::prelude::*,
	deps::{
		frame_support::traits::Authorize,
		sp_core::{sr25519, Pair},
	},
};
use sp_mixnet::types::{AuthorityId, AuthoritySignature};

fn setup_registration<T: Config>() -> (RegistrationFor<T>, AuthoritySignature) {
	let session_index = 0u32;
	let authority_index = 0u32;

	let (pair, _) = sr25519::Pair::generate();
	let authority_id = AuthorityId::from(pair.public());

	pallet::CurrentSessionIndex::<T>::put(session_index);
	pallet::NextAuthorityIds::<T>::insert(authority_index, authority_id);

	let registration = Registration {
		block_number: 1u32.into(),
		session_index,
		authority_index,
		mixnode: BoundedMixnode {
			kx_public: [1u8; 32],
			peer_id: [2u8; 32],
			external_addresses: Default::default(),
		},
	};
	let signature = AuthoritySignature::from(pair.sign(&registration.encode()));

	(registration, signature)
}

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Measures the execution time of `register` dispatch.
	#[benchmark]
	fn register() {
		let (registration, signature) = setup_registration::<T>();
		let session_index = registration.session_index;
		let authority_index = registration.authority_index;

		#[extrinsic_call]
		_(RawOrigin::Authorized, registration, signature);

		assert!(pallet::Mixnodes::<T>::contains_key(session_index + 1, authority_index));
	}

	/// Measures the weight of the authorize closure for `register`.
	#[benchmark]
	fn authorize_register() {
		let (registration, signature) = setup_registration::<T>();
		let call = pallet::Call::<T>::register { registration, signature };
		let source = TransactionSource::External;

		#[block]
		{
			call.authorize(source)
				.expect("Call should have authorize logic")
				.expect("Authorization should succeed");
		}
	}

	impl_benchmark_test_suite!(Pallet, crate::tests::new_test_ext(), crate::tests::Test);
}
