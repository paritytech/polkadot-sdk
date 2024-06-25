// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Mock runtime for tests.

use frame_support::{
	construct_runtime, derive_impl, parameter_types, sp_runtime,
	sp_runtime::traits::{IdentifyAccount, IdentityLookup, Verify},
};
use xcm::VersionedLocation;
use xcm_builder::ParentIsPreset;
use xcm_runtime_api::conversions::{
	Error as LocationToAccountApiError, LocationToAccountApi, LocationToAccountHelper,
};

construct_runtime! {
	pub enum TestRuntime {
		System: frame_system,
	}
}

type Block = frame_system::mocking::MockBlock<TestRuntime>;
pub type Signature = sp_runtime::MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
}

parameter_types! {
	pub const DefaultSs58Prefix: u16 = 0;
}

/// We alias local account locations to actual local accounts.
/// We also allow sovereign accounts for other sibling chains.
// pub type LocationToAccountId = (AccountIndex64Aliases<AnyNetwork, u64>, SiblingChainToIndex64);
pub type LocationToAccountId = (ParentIsPreset<AccountId>,);

pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	sp_io::TestExternalities::default().execute_with(test)
}

#[derive(Clone)]
pub(crate) struct TestClient;

pub(crate) struct RuntimeApi {
	_inner: TestClient,
}

impl sp_api::ProvideRuntimeApi<Block> for TestClient {
	type Api = RuntimeApi;
	fn runtime_api(&self) -> sp_api::ApiRef<Self::Api> {
		RuntimeApi { _inner: self.clone() }.into()
	}
}

sp_api::mock_impl_runtime_apis! {
	impl LocationToAccountApi<Block> for RuntimeApi {
		fn convert_location(location: VersionedLocation) -> Result<Vec<u8>, LocationToAccountApiError> {
			LocationToAccountHelper::<
				AccountId,
				LocationToAccountId
			>::convert_location(location)
		}
	}
}
