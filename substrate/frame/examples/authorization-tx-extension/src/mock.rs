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

use crate::*;
pub(crate) use example_runtime::*;
use extensions::AuthorizeCoownership;
use frame_support::derive_impl;
use frame_system::{CheckEra, CheckGenesis, CheckNonce, CheckTxVersion};
use pallet_verify_signature::VerifySignature;
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, IdentifyAccount, IdentityLookup, Verify},
	BuildStorage, MultiSignature, MultiSigner,
};

#[docify::export]
mod example_runtime {
	use super::*;

	/// Our `TransactionExtension` fit for general transactions.
	pub type TxExtension = (
		// Validate the signature of regular account transactions (substitutes the old signed
		// transaction).
		VerifySignature<Runtime>,
		// Nonce check (and increment) for the caller.
		CheckNonce<Runtime>,
		// If activated, will mutate the origin to a `pallet_coownership` origin of 2 accounts that
		// own something.
		AuthorizeCoownership<Runtime, MultiSigner, MultiSignature>,
		// Some other extensions that we want to run for every possible origin and we want captured
		// in any and all signature and authorization schemes (such as the traditional account
		// signature or the double signature in `pallet_coownership`).
		CheckGenesis<Runtime>,
		CheckTxVersion<Runtime>,
		CheckEra<Runtime>,
	);
	/// Convenience type to more easily construct the signature to be signed in case
	/// `AuthorizeCoownership` is activated.
	pub type InnerTxExtension = (CheckGenesis<Runtime>, CheckTxVersion<Runtime>, CheckEra<Runtime>);
	pub type UncheckedExtrinsic =
		generic::UncheckedExtrinsic<AccountId, RuntimeCall, Signature, TxExtension>;
	pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
	pub type Block = generic::Block<Header, UncheckedExtrinsic>;
	pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;
	pub type Signature = MultiSignature;
	pub type BlockNumber = u32;

	// For testing the pallet, we construct a mock runtime.
	frame_support::construct_runtime!(
		pub enum Runtime
		{
			System: frame_system,
			VerifySignaturePallet: pallet_verify_signature,

			Assets: pallet_assets,
			Coownership: pallet_coownership,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type AccountId = AccountId;
		type Block = Block;
		type Lookup = IdentityLookup<Self::AccountId>;
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub struct BenchmarkHelper;
	#[cfg(feature = "runtime-benchmarks")]
	impl pallet_verify_signature::BenchmarkHelper<MultiSignature, AccountId> for BenchmarkHelper {
		fn create_signature(_entropy: &[u8], msg: &[u8]) -> (MultiSignature, AccountId) {
			use sp_io::crypto::{sr25519_generate, sr25519_sign};
			use sp_runtime::traits::IdentifyAccount;
			let public = sr25519_generate(0.into(), None);
			let who_account: AccountId = MultiSigner::Sr25519(public).into_account().into();
			let signature = MultiSignature::Sr25519(sr25519_sign(0.into(), &public, msg).unwrap());
			(signature, who_account)
		}
	}

	impl pallet_verify_signature::Config for Runtime {
		type Signature = MultiSignature;
		type AccountIdentifier = MultiSigner;
		type WeightInfo = ();
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper = BenchmarkHelper;
	}

	/// Type that enables any pallet to ask for a coowner origin.
	pub struct EnsureCoowner;
	impl EnsureOrigin<RuntimeOrigin> for EnsureCoowner {
		type Success = (AccountId, AccountId);

		fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
			match o.clone().into() {
				Ok(pallet_coownership::Origin::<Runtime>::Coowners(first, second)) =>
					Ok((first, second)),
				_ => Err(o),
			}
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
			unimplemented!()
		}
	}

	impl pallet_assets::Config for Runtime {
		type CoownerOrigin = EnsureCoowner;
	}

	impl pallet_coownership::Config for Runtime {
		type RuntimeOrigin = RuntimeOrigin;
		type PalletsOrigin = OriginCaller;
	}
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = RuntimeGenesisConfig {
		// We use default for brevity, but you can configure as desired if needed.
		system: Default::default(),
	}
	.build_storage()
	.unwrap();
	t.into()
}
