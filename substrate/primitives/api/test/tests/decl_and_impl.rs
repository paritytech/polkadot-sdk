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

use sp_api::{
	decl_runtime_apis, impl_runtime_apis, mock_impl_runtime_apis, RuntimeApiInfo, RuntimeInstance,
};
use sp_runtime::traits::Block as BlockT;

use substrate_test_runtime_client::runtime::{Block, Hash};

/// The declaration of the `Runtime` type is done by the `construct_runtime!` macro in a real
/// runtime.
pub struct Runtime {}

decl_runtime_apis! {
<<<<<<< HEAD
	pub trait Api<Block: BlockT> {
||||||| 07e55006ad0
	pub trait Api {
=======
	#[deprecated]
	pub trait Api {
>>>>>>> origin/master
		fn test(data: u64);
		fn something_with_block(block: Block) -> Block;
		fn function_with_two_args(data: u64, block: Block);
		fn same_name();
		fn wild_card(_: u32);
	}

	#[api_version(2)]
	pub trait ApiWithCustomVersion {
		fn same_name();
		#[changed_in(2)]
		fn same_name() -> String;
	}

	#[api_version(2)]
	pub trait ApiWithMultipleVersions {
		fn stable_one(data: u64);
		#[api_version(3)]
		fn new_one();
		#[api_version(4)]
		fn glory_one();
	}

	pub trait ApiWithStagingMethod {
		fn stable_one(data: u64);
		#[api_version(99)]
		fn staging_one();
	}

	pub trait ApiWithStagingAndVersionedMethods {
		fn stable_one(data: u64);
		#[api_version(2)]
		fn new_one();
		#[api_version(99)]
		fn staging_one();
	}

	#[api_version(2)]
	pub trait ApiWithStagingAndChangedBase {
		fn stable_one(data: u64);
		fn new_one();
		#[api_version(99)]
		fn staging_one();
	}

}

impl_runtime_apis! {
	#[allow(deprecated)]
	impl self::Api<Block> for Runtime {
		fn test(_: u64) {
			unimplemented!()
		}

		// Ensure that we accept `mut`
		fn something_with_block(mut _block: Block) -> Block {
			unimplemented!()
		}

		fn function_with_two_args(_: u64, _: Block) {
			unimplemented!()
		}

		fn same_name() {}

		fn wild_card(_: u32) {}
	}

	impl self::ApiWithCustomVersion for Runtime {
		fn same_name() {}
	}

	#[api_version(3)]
	impl self::ApiWithMultipleVersions for Runtime {
		fn stable_one(_: u64) {}

		fn new_one() {}
	}

	#[cfg_attr(feature = "enable-staging-api", api_version(99))]
	impl self::ApiWithStagingMethod for Runtime {
		fn stable_one(_: u64) {}

		#[cfg(feature = "enable-staging-api")]
		fn staging_one() { }
	}

	#[cfg_attr(feature = "enable-staging-api", api_version(99))]
	#[api_version(2)]
	impl self::ApiWithStagingAndVersionedMethods for Runtime {
		fn stable_one(_: u64) {}
		fn new_one() {}

		#[cfg(feature = "enable-staging-api")]
		fn staging_one() {}
	}

	#[cfg_attr(feature = "enable-staging-api", api_version(99))]
	impl self::ApiWithStagingAndChangedBase for Runtime {
		fn stable_one(_: u64) {}
		fn new_one() {}

		#[cfg(feature = "enable-staging-api")]
		fn staging_one() {}
	}

	impl sp_api::Core<Block> for Runtime {
		fn version() -> sp_version::RuntimeVersion {
			unimplemented!()
		}
		fn execute_block(_: Block) {
			unimplemented!()
		}
		fn initialize_block(_: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
			unimplemented!()
		}
	}
}

struct MockApi {
	block: Option<Block>,
}

mock_impl_runtime_apis! {
	#[allow(deprecated)]
	impl Api<Block> for MockApi {
		fn test(_: u64) {
			unimplemented!()
		}

		fn something_with_block(&mut self, _: Block) -> Block {
			self.block.clone().unwrap()
		}

		fn function_with_two_args(_: u64, _: Block) {
			unimplemented!()
		}

		fn same_name() {}

		fn wild_card() {}
	}

	impl ApiWithCustomVersion for MockApi {
		fn same_name() {}
	}
}

<<<<<<< HEAD
||||||| 07e55006ad0
type TestClient = substrate_test_runtime_client::client::Client<
	substrate_test_runtime_client::Backend,
	substrate_test_runtime_client::ExecutorDispatch,
	Block,
	RuntimeApi,
>;

#[test]
fn test_client_side_function_signature() {
	let _test: fn(
		&RuntimeApiImpl<Block, TestClient>,
		<Block as BlockT>::Hash,
		u64,
	) -> Result<(), ApiError> = RuntimeApiImpl::<Block, TestClient>::test;
	let _something_with_block: fn(
		&RuntimeApiImpl<Block, TestClient>,
		<Block as BlockT>::Hash,
		Block,
	) -> Result<Block, ApiError> = RuntimeApiImpl::<Block, TestClient>::something_with_block;

	#[allow(deprecated)]
	let _same_name_before_version_2: fn(
		&RuntimeApiImpl<Block, TestClient>,
		<Block as BlockT>::Hash,
	) -> Result<String, ApiError> = RuntimeApiImpl::<Block, TestClient>::same_name_before_version_2;
}

=======
type TestClient = substrate_test_runtime_client::client::Client<
	substrate_test_runtime_client::Backend,
	substrate_test_runtime_client::ExecutorDispatch,
	Block,
	RuntimeApi,
>;

#[test]
#[allow(deprecated)]
fn test_client_side_function_signature() {
	let _test: fn(
		&RuntimeApiImpl<Block, TestClient>,
		<Block as BlockT>::Hash,
		u64,
	) -> Result<(), ApiError> = RuntimeApiImpl::<Block, TestClient>::test;
	let _something_with_block: fn(
		&RuntimeApiImpl<Block, TestClient>,
		<Block as BlockT>::Hash,
		Block,
	) -> Result<Block, ApiError> = RuntimeApiImpl::<Block, TestClient>::something_with_block;

	#[allow(deprecated)]
	let _same_name_before_version_2: fn(
		&RuntimeApiImpl<Block, TestClient>,
		<Block as BlockT>::Hash,
	) -> Result<String, ApiError> = RuntimeApiImpl::<Block, TestClient>::same_name_before_version_2;
}

>>>>>>> origin/master
#[test]
#[allow(deprecated)]
fn check_runtime_api_info() {
	assert_eq!(&<dyn Api::<Block>>::ID, &runtime_decl_for_api::ID);
	assert_eq!(<dyn Api::<Block>>::VERSION, runtime_decl_for_api::VERSION);
	assert_eq!(<dyn Api::<Block>>::VERSION, 1);

	assert_eq!(
		<dyn ApiWithCustomVersion>::VERSION,
		runtime_decl_for_api_with_custom_version::VERSION,
	);
	assert_eq!(&<dyn ApiWithCustomVersion>::ID, &runtime_decl_for_api_with_custom_version::ID,);
	assert_eq!(<dyn ApiWithCustomVersion>::VERSION, 2);

	// The stable version of the API
	assert_eq!(<dyn ApiWithMultipleVersions>::VERSION, 2);

	assert_eq!(<dyn ApiWithStagingMethod>::VERSION, 1);
	assert_eq!(<dyn ApiWithStagingAndVersionedMethods>::VERSION, 1);
	assert_eq!(<dyn ApiWithStagingAndChangedBase>::VERSION, 2);
}

fn check_runtime_api_versions_contains<T: RuntimeApiInfo + ?Sized>() {
	assert!(RUNTIME_API_VERSIONS.iter().any(|v| v == &(T::ID, T::VERSION)));
}

fn check_staging_runtime_api_versions<T: RuntimeApiInfo + ?Sized>(_staging_ver: u32) {
	// Staging APIs should contain staging version if the feature is set...
	#[cfg(feature = "enable-staging-api")]
	assert!(RUNTIME_API_VERSIONS.iter().any(|v| v == &(T::ID, _staging_ver)));
	//... otherwise the base version should be set
	#[cfg(not(feature = "enable-staging-api"))]
	check_runtime_api_versions_contains::<dyn ApiWithStagingMethod>();
}

#[allow(unused_assignments)]
fn check_staging_multiver_runtime_api_versions<T: RuntimeApiInfo + ?Sized>(
	_staging_ver: u32,
	_stable_ver: u32,
) {
	// Staging APIs should contain staging version if the feature is set...
	#[cfg(feature = "enable-staging-api")]
	assert!(RUNTIME_API_VERSIONS.iter().any(|v| v == &(T::ID, _staging_ver)));
	//... otherwise the base version should be set
	#[cfg(not(feature = "enable-staging-api"))]
	assert!(RUNTIME_API_VERSIONS.iter().any(|v| v == &(T::ID, _stable_ver)));
}

#[test]
#[allow(deprecated)]
fn check_runtime_api_versions() {
	check_runtime_api_versions_contains::<dyn Api<Block>>();
	check_runtime_api_versions_contains::<dyn ApiWithCustomVersion>();
	assert!(RUNTIME_API_VERSIONS
		.iter()
		.any(|v| v == &(<dyn ApiWithMultipleVersions>::ID, 3)));

	check_staging_runtime_api_versions::<dyn ApiWithStagingMethod>(99);
	check_staging_multiver_runtime_api_versions::<dyn ApiWithStagingAndVersionedMethods>(99, 2);
	check_staging_runtime_api_versions::<dyn ApiWithStagingAndChangedBase>(99);

	check_runtime_api_versions_contains::<dyn sp_api::Core<Block>>();
}

#[test]
#[allow(deprecated)]
fn mock_runtime_api_has_api() {
	let runtime_api =
		RuntimeInstance::<_, Block, _>::builder(MockApi { block: None }, Hash::default())
			.off_chain_context()
			.build();

	assert!(runtime_api.has_api::<dyn ApiWithCustomVersion>().unwrap());
	assert!(runtime_api.has_api::<dyn Api<Block>>().unwrap());
}

#[test]
#[allow(deprecated)]
fn mock_runtime_api_works_with_advanced() {
	let mut runtime_api =
		RuntimeInstance::<_, Block, _>::builder(MockApi { block: None }, Hash::default())
			.off_chain_context()
			.build();

<<<<<<< HEAD
	Api::<Block>::same_name(&mut runtime_api).unwrap();
	Api::<Block>::wild_card(&mut runtime_api, 1).unwrap();
||||||| 07e55006ad0
	Api::<Block>::same_name(&mock, Hash::default()).unwrap();
	mock.wild_card(Hash::repeat_byte(0x0f), 1).unwrap();
	assert_eq!(
		"Test error".to_string(),
		mock.wild_card(Hash::repeat_byte(0x01), 1).unwrap_err().to_string(),
	);
=======
	Api::<Block>::same_name(&mock, Hash::default()).unwrap();
	mock.wild_card(Hash::repeat_byte(0x0f), 1).unwrap();
	assert_eq!(
		"Test error".to_string(),
		mock.wild_card(Hash::repeat_byte(0x01), 1).unwrap_err().to_string(),
	);
}

#[test]
fn runtime_api_metadata_matches_version_implemented() {
	use sp_metadata_ir::InternalImplRuntimeApis;

	let rt = Runtime {};
	let runtime_metadata = rt.runtime_metadata();

	// Check that the metadata for some runtime API matches expectation.
	let assert_has_api_with_methods = |api_name: &str, api_methods: &[&str]| {
		let Some(api) = runtime_metadata.iter().find(|api| api.name == api_name) else {
			panic!("Can't find runtime API '{api_name}'");
		};
		if api.methods.len() != api_methods.len() {
			panic!(
				"Wrong number of methods in '{api_name}'; expected {} methods but got {}: {:?}",
				api_methods.len(),
				api.methods.len(),
				api.methods
			);
		}
		for expected_name in api_methods {
			if !api.methods.iter().any(|method| &method.name == expected_name) {
				panic!("Can't find API method '{expected_name}' in '{api_name}'");
			}
		}
	};

	assert_has_api_with_methods("ApiWithCustomVersion", &["same_name"]);

	assert_has_api_with_methods("ApiWithMultipleVersions", &["stable_one", "new_one"]);

	assert_has_api_with_methods(
		"ApiWithStagingMethod",
		&[
			"stable_one",
			#[cfg(feature = "enable-staging-api")]
			"staging_one",
		],
	);

	assert_has_api_with_methods(
		"ApiWithStagingAndVersionedMethods",
		&[
			"stable_one",
			"new_one",
			#[cfg(feature = "enable-staging-api")]
			"staging_one",
		],
	);

	assert_has_api_with_methods(
		"ApiWithStagingAndChangedBase",
		&[
			"stable_one",
			"new_one",
			#[cfg(feature = "enable-staging-api")]
			"staging_one",
		],
	);
>>>>>>> origin/master
}
