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

//! Substrate runtime api
//!
//! The Substrate runtime api is the interface between the node and the runtime. There isn't a fixed
//! set of runtime apis, instead it is up to the user to declare and implement these runtime apis.
//! The declaration of a runtime api is normally done outside of a runtime, while the implementation
//! of it has to be done in the runtime. We provide the [`decl_runtime_apis!`] macro for declaring
//! a runtime api and the [`impl_runtime_apis!`] for implementing them. The macro docs provide more
//! information on how to use them and what kind of attributes we support.
//!
//! It is required that each runtime implements at least the [`Core`] runtime api. This runtime api
//! provides all the core functions that Substrate expects from a runtime.
//!
//! # Versioning
//!
//! Runtime apis support versioning. Each runtime api itself has a version attached. It is also
//! supported to change function signatures or names in a non-breaking way. For more information on
//! versioning check the [`decl_runtime_apis!`] macro.
//!
//! All runtime apis and their versions are returned as part of the [`RuntimeVersion`]. This can be
//! used to check which runtime api version is currently provided by the on-chain runtime.
//!
//! # Testing
//!
//! For testing we provide the [`mock_impl_runtime_apis!`] macro that lets you implement a runtime
//! api for a mocked object to use it in tests.
//!
//! # Logging
//!
//! Substrate supports logging from the runtime in native and in wasm. For that purpose it provides
//! the [`RuntimeLogger`](sp_runtime::runtime_logger::RuntimeLogger). This runtime logger is
//! automatically enabled for each call into the runtime through the runtime api. As logging
//! introduces extra code that isn't actually required for the logic of your runtime and also
//! increases the final wasm blob size, it is recommended to disable the logging for on-chain
//! wasm blobs. This can be done by enabling the `disable-logging` feature of this crate. Be aware
//! that this feature instructs `log` and `tracing` to disable logging at compile time by setting
//! the `max_level_off` feature for these crates. So, you should not enable this feature for a
//! native build as otherwise the node will not output any log messages.
//!
//! # How does it work?
//!
//! Each runtime api is declared as a trait with functions. When compiled to WASM, each implemented
//! runtime api function is exported as a function with the following naming scheme
//! `${TRAIT_NAME}_${FUNCTION_NAME}`. Such a function has the following signature
//! `(ptr: *u8, length: u32) -> u64`. It takes a pointer to an `u8` array and its length as an
//! argument. This `u8` array is expected to be the SCALE encoded parameters of the function as
//! defined in the trait. The return value is an `u64` that represents `length << 32 | pointer` of
//! an `u8` array. This return value `u8` array contains the SCALE encoded return value as defined
//! by the trait function. The macros take care to encode the parameters and to decode the return
//! value.

#![cfg_attr(not(feature = "std"), no_std)]

// Make doc tests happy
extern crate self as sp_api;

/// Private exports used by the macros.
///
/// This is seen as internal API and can change at any point.
#[doc(hidden)]
pub mod __private {
	#[cfg(feature = "std")]
	mod std_imports {
		pub use hash_db::Hasher;
		pub use sp_core::traits::CallContext;
		pub use sp_externalities::{Extension, Extensions};
		pub use sp_runtime::StateVersion;
		pub use sp_state_machine::{
			Backend as StateBackend, InMemoryBackend, OverlayedChanges, StorageProof, TrieBackend,
			TrieBackendBuilder,
		};
	}
	#[cfg(feature = "std")]
	pub use std_imports::*;

	pub use crate::*;
	pub use codec::{self, Decode, DecodeLimit, Encode};
	pub use scale_info;
	pub use sp_core::offchain;
	#[cfg(not(feature = "std"))]
	pub use sp_core::to_substrate_wasm_fn_return_value;
	#[cfg(feature = "frame-metadata")]
	pub use sp_metadata_ir::{self as metadata_ir, frame_metadata as metadata};
	pub use sp_runtime::{
		generic::BlockId,
		traits::{Block as BlockT, Hash as HashT, HashingFor, Header as HeaderT, NumberFor},
		transaction_validity::TransactionValidity,
		RuntimeString, TransactionOutcome,
	};
	pub use sp_std::{mem, slice, vec};
	pub use sp_version::{create_apis_vec, ApiId, ApisVec, RuntimeVersion};
}

#[cfg(feature = "std")]
pub use sp_core::traits::CallContext;
use sp_core::OpaqueMetadata;
#[cfg(feature = "std")]
use sp_externalities::{Extension, Extensions};
use sp_runtime::traits::Block as BlockT;
#[cfg(feature = "std")]
use sp_runtime::traits::HashingFor;
#[cfg(feature = "std")]
pub use sp_runtime::TransactionOutcome;
#[cfg(feature = "std")]
pub use sp_state_machine::StorageProof;
#[cfg(feature = "std")]
use sp_state_machine::{Backend as StateBackend, OverlayedChanges};
use sp_version::RuntimeVersion;

/// Maximum nesting level for extrinsics.
pub const MAX_EXTRINSIC_DEPTH: u32 = 256;

/// Declares given traits as runtime apis.
///
/// The macro will create two declarations, one for using on the client side and one for using
/// on the runtime side. The declaration for the runtime side is hidden in its own module.
/// The client side declaration gets two extra parameters per function,
/// `&self` and `at: Block::Hash`. The runtime side declaration will match the given trait
/// declaration. Besides one exception, the macro adds an extra generic parameter `Block:
/// BlockT` to the client side and the runtime side. This generic parameter is usable by the
/// user.
///
/// For implementing these macros you should use the
/// [`impl_runtime_apis!`] macro.
///
/// # Example
///
/// ```rust
/// # use sp_runtime::traits::Block as BlockT;
/// sp_api::decl_runtime_apis! {
///     /// Declare the api trait.
///     pub trait Balance {
///         /// Get the balance.
///         fn get_balance() -> u64;
///         /// Set the balance.
///         fn set_balance(val: u64);
///     }
///
///     /// You can declare multiple api traits in one macro call.
///     /// In one module you can call the macro at maximum one time.
///     pub trait BlockBuilder<Block: BlockT> {
///         /// The macro adds an explicit `Block: BlockT` generic parameter for you.
///         /// You can use this generic parameter as you would defined it manually.
///         fn build_block() -> Block;
///     }
/// }
///
/// # fn main() {}
/// ```
///
/// # Runtime api trait versioning
///
/// To support versioning of the traits, the macro supports the attribute `#[api_version(1)]`.
/// The attribute supports any `u32` as version. By default, each trait is at version `1`, if
/// no version is provided. We also support changing the signature of a method. This signature
/// change is highlighted with the `#[changed_in(2)]` attribute above a method. A method that
/// is tagged with this attribute is callable by the name `METHOD_before_version_VERSION`. This
/// method will only support calling into wasm, trying to call into native will fail (change
/// the spec version!). Such a method also does not need to be implemented in the runtime. It
/// is required that there exist the "default" of the method without the `#[changed_in(_)]`
/// attribute, this method will be used to call the current default implementation.
///
/// ```rust
/// sp_api::decl_runtime_apis! {
///     /// Declare the api trait.
///     #[api_version(2)]
///     pub trait Balance {
///         /// Get the balance.
///         fn get_balance() -> u64;
///         /// Set balance.
///         fn set_balance(val: u64);
///         /// Set balance, old version.
///         ///
///         /// Is callable by `set_balance_before_version_2`.
///         #[changed_in(2)]
///         fn set_balance(val: u16);
///         /// In version 2, we added this new function.
///         fn increase_balance(val: u64);
///     }
/// }
///
/// # fn main() {}
/// ```
///
/// To check if a given runtime implements a runtime api trait, the `RuntimeVersion` has the
/// function `has_api<A>()`. Also the `ApiExt` provides a function `has_api<A>(at: Hash)`
/// to check if the runtime at the given block id implements the requested runtime api trait.
///
/// # Declaring multiple api versions
///
/// Optionally multiple versions of the same api can be declared. This is useful for
/// development purposes. For example you want to have a testing version of the api which is
/// available only on a testnet. You can define one stable and one development version. This
/// can be done like this:
/// ```rust
/// sp_api::decl_runtime_apis! {
///     /// Declare the api trait.
///     #[api_version(2)]
///     pub trait Balance {
///         /// Get the balance.
///         fn get_balance() -> u64;
///         /// Set the balance.
///         fn set_balance(val: u64);
///         /// Transfer the balance to another user id
///         #[api_version(3)]
///         fn transfer_balance(uid: u64);
///     }
/// }
///
/// # fn main() {}
/// ```
/// The example above defines two api versions - 2 and 3. Version 2 contains `get_balance` and
/// `set_balance`. Version 3 additionally contains `transfer_balance`, which is not available
/// in version 2. Version 2 in this case is considered the default/base version of the api.
/// More than two versions can be defined this way. For example:
/// ```rust
/// sp_api::decl_runtime_apis! {
///     /// Declare the api trait.
///     #[api_version(2)]
///     pub trait Balance {
///         /// Get the balance.
///         fn get_balance() -> u64;
///         /// Set the balance.
///         fn set_balance(val: u64);
///         /// Transfer the balance to another user id
///         #[api_version(3)]
///         fn transfer_balance(uid: u64);
///         /// Clears the balance
///         #[api_version(4)]
///         fn clear_balance();
///     }
/// }
///
/// # fn main() {}
/// ```
/// Note that the latest version (4 in our example above) always contains all methods from all
/// the versions before.
pub use sp_api_proc_macro::decl_runtime_apis;

/// Tags given trait implementations as runtime apis.
///
/// All traits given to this macro, need to be declared with the
/// [`decl_runtime_apis!`](macro.decl_runtime_apis.html) macro. The implementation of the trait
/// should follow the declaration given to the
/// [`decl_runtime_apis!`](macro.decl_runtime_apis.html) macro, besides the `Block` type that
/// is required as first generic parameter for each runtime api trait. When implementing a
/// runtime api trait, it is required that the trait is referenced by a path, e.g. `impl
/// my_trait::MyTrait for Runtime`. The macro will use this path to access the declaration of
/// the trait for the runtime side.
///
/// The macro also generates the api implementations for the client side and provides it
/// through the `RuntimeApi` type. The `RuntimeApi` is hidden behind a `feature` called `std`.
///
/// To expose version information about all implemented api traits, the constant
/// `RUNTIME_API_VERSIONS` is generated. This constant should be used to instantiate the `apis`
/// field of `RuntimeVersion`.
///
/// # Example
///
/// ```rust
/// use sp_version::create_runtime_str;
/// #
/// # use sp_runtime::traits::Block as BlockT;
/// # use sp_test_primitives::Block;
/// #
/// # /// The declaration of the `Runtime` type is done by the `construct_runtime!` macro
/// # /// in a real runtime.
/// # pub enum Runtime {}
/// #
/// # sp_api::decl_runtime_apis! {
/// #     /// Declare the api trait.
/// #     pub trait Balance {
/// #         /// Get the balance.
/// #         fn get_balance() -> u64;
/// #         /// Set the balance.
/// #         fn set_balance(val: u64);
/// #     }
/// #     pub trait BlockBuilder<Block: BlockT> {
/// #        fn build_block() -> Block;
/// #     }
/// # }
///
/// /// All runtime api implementations need to be done in one call of the macro!
/// sp_api::impl_runtime_apis! {
/// #   impl sp_api::Core<Block> for Runtime {
/// #       fn version() -> sp_version::RuntimeVersion {
/// #           unimplemented!()
/// #       }
/// #       fn execute_block(_block: Block) {}
/// #       fn initialize_block(_header: &<Block as BlockT>::Header) {}
/// #   }
///
///     impl self::Balance for Runtime {
///         fn get_balance() -> u64 {
///             1
///         }
///         fn set_balance(_bal: u64) {
///             // Store the balance
///         }
///     }
///
///     impl self::BlockBuilder<Block> for Runtime {
///         fn build_block() -> Block {
///              unimplemented!("Please implement me!")
///         }
///     }
/// }
///
/// /// Runtime version. This needs to be declared for each runtime.
/// pub const VERSION: sp_version::RuntimeVersion = sp_version::RuntimeVersion {
///     spec_name: create_runtime_str!("node"),
///     impl_name: create_runtime_str!("test-node"),
///     authoring_version: 1,
///     spec_version: 1,
///     impl_version: 0,
///     // Here we are exposing the runtime api versions.
///     apis: RUNTIME_API_VERSIONS,
///     transaction_version: 1,
///     state_version: 1,
/// };
///
/// # fn main() {}
/// ```
///
/// # Implementing specific api version
///
/// If `decl_runtime_apis!` declares multiple versions for an api `impl_runtime_apis!`
/// should specify which version it implements by adding `api_version` attribute to the
/// `impl` block. If omitted - the base/default version is implemented. Here is an example:
/// ```ignore
/// sp_api::impl_runtime_apis! {
///     #[api_version(3)]
///     impl self::Balance<Block> for Runtime {
///          // implementation
///     }
/// }
/// ```
/// In this case `Balance` api version 3 is being implemented for `Runtime`. The `impl` block
/// must contain all methods declared in version 3 and below.
///
/// # Conditional version implementation
///
/// `impl_runtime_apis!` supports `cfg_attr` attribute for conditional compilation. For example
/// let's say you want to implement a staging version of the runtime api and put it behind a
/// feature flag. You can do it this way:
/// ```ignore
/// pub enum Runtime {}
/// sp_api::decl_runtime_apis! {
///     pub trait ApiWithStagingMethod {
///         fn stable_one(data: u64);
///
///         #[api_version(99)]
///         fn staging_one();
///     }
/// }
///
/// sp_api::impl_runtime_apis! {
///     #[cfg_attr(feature = "enable-staging-api", api_version(99))]
///     impl self::ApiWithStagingMethod<Block> for Runtime {
///         fn stable_one(_: u64) {}
///
///         #[cfg(feature = "enable-staging-api")]
///         fn staging_one() {}
///     }
/// }
/// ```
///
/// [`decl_runtime_apis!`] declares two version of the api - 1 (the default one, which is
/// considered stable in our example) and 99 (which is considered staging). In
/// `impl_runtime_apis!` a `cfg_attr` attribute is attached to the `ApiWithStagingMethod`
/// implementation. If the code is compiled with  `enable-staging-api` feature a version 99 of
/// the runtime api will be built which will include `staging_one`. Note that `staging_one`
/// implementation is feature gated by `#[cfg(feature = ... )]` attribute.
///
/// If the code is compiled without `enable-staging-api` version 1 (the default one) will be
/// built which doesn't include `staging_one`.
///
/// `cfg_attr` can also be used together with `api_version`. For the next snippet will build
/// version 99 if `enable-staging-api` is enabled and version 2 otherwise because both
/// `cfg_attr` and `api_version` are attached to the impl block:
/// ```ignore
/// #[cfg_attr(feature = "enable-staging-api", api_version(99))]
/// #[api_version(2)]
/// impl self::ApiWithStagingAndVersionedMethods<Block> for Runtime {
///  // impl skipped
/// }
/// ```
pub use sp_api_proc_macro::impl_runtime_apis;

/// Mocks given trait implementations as runtime apis.
///
/// Accepts similar syntax as [`impl_runtime_apis!`] and generates simplified mock
/// implementations of the given runtime apis. The difference in syntax is that the trait does
/// not need to be referenced by a qualified path, methods accept the `&self` parameter and the
/// error type can be specified as associated type. If no error type is specified [`String`] is
/// used as error type.
///
/// Besides implementing the given traits, the [`Core`] and [`ApiExt`] are implemented
/// automatically.
///
/// # Example
///
/// ```rust
/// # use sp_runtime::traits::Block as BlockT;
/// # use sp_test_primitives::Block;
/// #
/// # sp_api::decl_runtime_apis! {
/// #     /// Declare the api trait.
/// #     pub trait Balance {
/// #         /// Get the balance.
/// #         fn get_balance() -> u64;
/// #         /// Set the balance.
/// #         fn set_balance(val: u64);
/// #     }
/// #     pub trait BlockBuilder {
/// #        fn build_block() -> Block;
/// #     }
/// # }
/// struct MockApi {
///     balance: u64,
/// }
///
/// /// All runtime api mock implementations need to be done in one call of the macro!
/// //sp_api::mock_impl_runtime_apis! {
/// //    impl Balance for MockApi {
/// //        /// Here we take the `&self` to access the instance.
/// //        fn get_balance(&self) -> u64 {
/// //            self.balance
/// //        }
/// //        fn set_balance(_bal: u64) {
/// //            // Store the balance
/// //        }
/// //    }
///
/// //    impl BlockBuilder<Block> for MockApi {
/// //        fn build_block() -> Block {
/// //             unimplemented!("Not Required in tests")
/// //        }
/// //    }
/// //}
///
/// # fn main() {}
/// ```
///
/// # `advanced` attribute
///
/// This attribute can be placed above individual function in the mock implementation to
/// request more control over the function declaration. From the client side each runtime api
/// function is called with the `at` parameter that is a [`Hash`](sp_runtime::traits::Hash).
/// When using the `advanced` attribute, the macro expects that the first parameter of the
/// function is this `at` parameter. Besides that the macro also doesn't do the automatic
/// return value rewrite, which means that full return value must be specified. The full return
/// value is constructed like [`Result`]`<<ReturnValue>, Error>` while `ReturnValue` being the
/// return value that is specified in the trait declaration.
///
/// ## Example
/// ```rust
/// # use sp_runtime::traits::Block as BlockT;
/// # use sp_test_primitives::Block;
/// # use codec;
/// #
/// # sp_api::decl_runtime_apis! {
/// #     /// Declare the api trait.
/// #     pub trait Balance {
/// #         /// Get the balance.
/// #         fn get_balance() -> u64;
/// #         /// Set the balance.
/// #         fn set_balance(val: u64);
/// #     }
/// # }
/// struct MockApi {
///     balance: u64,
/// }
///
/// // sp_api::mock_impl_runtime_apis! {
/// //     impl Balance<Block> for MockApi {
/// //         #[advanced]
/// //         fn get_balance(&self, at: <Block as BlockT>::Hash) -> Result<u64, sp_api::ApiError> {
/// //             println!("Being called at: {}", at);
/// //
/// //             Ok(self.balance.into())
/// //         }
/// //         #[advanced]
/// //         fn set_balance(at: <Block as BlockT>::Hash, val: u64) -> Result<(), sp_api::ApiError> {
/// //             println!("Being called at: {}", at);
/// //
/// //             Ok(().into())
/// //         }
/// //     }
/// // }
///
/// # fn main() {}
/// ```
pub use sp_api_proc_macro::mock_impl_runtime_apis;

/// A type that records all accessed trie nodes and generates a proof out of it.
#[cfg(feature = "std")]
pub type ProofRecorder<B> = sp_trie::recorder::Recorder<HashingFor<B>>;

#[cfg(feature = "std")]
pub type StorageChanges<Block> = sp_state_machine::StorageChanges<HashingFor<Block>>;

/// Something that can be constructed to a runtime api.
#[cfg(feature = "std")]
pub trait ConstructRuntimeApi<Block: BlockT, C: CallApiAt<Block>> {
	/// The actual runtime api that will be constructed.
	type RuntimeApi: ApiExt<Block>;

	/// Construct an instance of the runtime api.
	fn construct_runtime_api(call: &C) -> ApiRef<Self::RuntimeApi>;
}

/// Init the [`RuntimeLogger`](sp_runtime::runtime_logger::RuntimeLogger).
pub fn init_runtime_logger() {
	#[cfg(not(feature = "disable-logging"))]
	sp_runtime::runtime_logger::RuntimeLogger::init();
}

/// An error describing which API call failed.
#[cfg(feature = "std")]
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
	#[error("Failed to decode return value of {function}")]
	FailedToDecodeReturnValue {
		function: &'static str,
		#[source]
		error: codec::Error,
	},
	#[error("Failed to convert return value from runtime to node of {function}")]
	FailedToConvertReturnValue {
		function: &'static str,
		#[source]
		error: codec::Error,
	},
	#[error("Failed to convert parameter `{parameter}` from node to runtime of {function}")]
	FailedToConvertParameter {
		function: &'static str,
		parameter: &'static str,
		#[source]
		error: codec::Error,
	},
	#[error("The given `StateBackend` isn't a `TrieBackend`.")]
	StateBackendIsNotTrie,
	#[error(transparent)]
	Application(#[from] Box<dyn std::error::Error + Send + Sync>),
	#[error("Api called for an unknown Block: {0}")]
	UnknownBlock(String),
	#[error("Using the same api instance to call into multiple independent blocks.")]
	UsingSameInstanceForDifferentBlocks,
}

/// Extends the runtime api implementation with some common functionality.
#[cfg(feature = "std")]
pub trait ApiExt<Block: BlockT> {
	/// Execute the given closure inside a new transaction.
	///
	/// Depending on the outcome of the closure, the transaction is committed or rolled-back.
	///
	/// The internal result of the closure is returned afterwards.
	fn execute_in_transaction<F: FnOnce(&Self) -> TransactionOutcome<R>, R>(&self, call: F) -> R
	where
		Self: Sized;

	/// Checks if the given api is implemented and versions match.
	fn has_api<A: RuntimeApiInfo + ?Sized>(&self, at_hash: Block::Hash) -> Result<bool, ApiError>
	where
		Self: Sized;

	/// Check if the given api is implemented and the version passes a predicate.
	fn has_api_with<A: RuntimeApiInfo + ?Sized, P: Fn(u32) -> bool>(
		&self,
		at_hash: Block::Hash,
		pred: P,
	) -> Result<bool, ApiError>
	where
		Self: Sized;

	/// Returns the version of the given api.
	fn api_version<A: RuntimeApiInfo + ?Sized>(
		&self,
		at_hash: Block::Hash,
	) -> Result<Option<u32>, ApiError>
	where
		Self: Sized;

	/// Start recording all accessed trie nodes for generating proofs.
	fn record_proof(&mut self);

	/// Extract the recorded proof.
	///
	/// This stops the proof recording.
	///
	/// If `record_proof` was not called before, this will return `None`.
	fn extract_proof(&mut self) -> Option<StorageProof>;

	/// Returns the current active proof recorder.
	fn proof_recorder(&self) -> Option<ProofRecorder<Block>>;

	/// Convert the api object into the storage changes that were done while executing runtime
	/// api functions.
	///
	/// After executing this function, all collected changes are reset.
	fn into_storage_changes<B: StateBackend<HashingFor<Block>>>(
		&self,
		backend: &B,
		parent_hash: Block::Hash,
	) -> Result<StorageChanges<Block>, String>
	where
		Self: Sized;

	/// Set the [`CallContext`] to be used by the runtime api calls done by this instance.
	fn set_call_context(&mut self, call_context: CallContext);

	/// Register an [`Extension`] that will be accessible while executing a runtime api call.
	fn register_extension<E: Extension>(&mut self, extension: E);
}

/// Parameters for [`CallApiAt::call_api_at`].
#[cfg(feature = "std")]
pub struct CallApiAtParams<'a, Block: BlockT> {
	/// The block id that determines the state that should be setup when calling the function.
	pub at: Block::Hash,
	/// The name of the function that should be called.
	pub function: &'static str,
	/// The encoded arguments of the function.
	pub arguments: Vec<u8>,
	/// The overlayed changes that are on top of the state.
	pub overlayed_changes: &'a mut OverlayedChanges<HashingFor<Block>>,
	/// The call context of this call.
	pub call_context: CallContext,
	/// The optional proof recorder for recording storage accesses.
	pub recorder: Option<&'a ProofRecorder<Block>>,
	/// The extensions that should be used for this call.
	pub extensions: &'a mut Extensions,
}

/// Something that can call into the an api at a given block.
#[cfg(feature = "std")]
pub trait CallApiAt<Block: BlockT> {
	/// The state backend that is used to store the block states.
	type StateBackend: StateBackend<HashingFor<Block>>;

	/// Calls the given api function with the given encoded arguments at the given block and returns
	/// the encoded result.
	fn call_api_at(&self, params: CallApiAtParams<Block>) -> Result<Vec<u8>, ApiError>;

	/// Returns the runtime version at the given block.
	fn runtime_version_at(&self, at_hash: Block::Hash) -> Result<RuntimeVersion, ApiError>;

	/// Get the state `at` the given block.
	fn state_at(&self, at: Block::Hash) -> Result<Self::StateBackend, ApiError>;

	/// Initialize the `extensions` for the given block `at` by using the global extensions factory.
	fn initialize_extensions(
		&self,
		at: Block::Hash,
		extensions: &mut Extensions,
	) -> Result<(), ApiError>;
}

#[cfg(feature = "std")]
impl<T: CallApiAt<Block>, Block: BlockT> CallApiAt<Block> for &T {
	type StateBackend = T::StateBackend;

	fn call_api_at(&self, params: CallApiAtParams<Block>) -> Result<Vec<u8>, ApiError> {
		(*self).call_api_at(params)
	}

	fn runtime_version_at(&self, at_hash: Block::Hash) -> Result<RuntimeVersion, ApiError> {
		(*self).runtime_version_at(at_hash)
	}

	fn state_at(&self, at: Block::Hash) -> Result<Self::StateBackend, ApiError> {
		(*self).state_at(at)
	}

	fn initialize_extensions(
		&self,
		at: Block::Hash,
		extensions: &mut Extensions,
	) -> Result<(), ApiError> {
		(*self).initialize_extensions(at, extensions)
	}
}

#[cfg(feature = "std")]
impl<T: CallApiAt<Block>, Block: BlockT> CallApiAt<Block> for std::sync::Arc<T> {
	type StateBackend = T::StateBackend;

	fn call_api_at(&self, params: CallApiAtParams<Block>) -> Result<Vec<u8>, ApiError> {
		(**self).call_api_at(params)
	}

	fn runtime_version_at(&self, at_hash: Block::Hash) -> Result<RuntimeVersion, ApiError> {
		(**self).runtime_version_at(at_hash)
	}

	fn state_at(&self, at: Block::Hash) -> Result<Self::StateBackend, ApiError> {
		(**self).state_at(at)
	}

	fn initialize_extensions(
		&self,
		at: Block::Hash,
		extensions: &mut Extensions,
	) -> Result<(), ApiError> {
		(**self).initialize_extensions(at, extensions)
	}
}

/// Auxiliary wrapper that holds an api instance and binds it to the given lifetime.
#[cfg(feature = "std")]
pub struct ApiRef<'a, T>(T, std::marker::PhantomData<&'a ()>);

#[cfg(feature = "std")]
impl<'a, T> From<T> for ApiRef<'a, T> {
	fn from(api: T) -> Self {
		ApiRef(api, Default::default())
	}
}

#[cfg(feature = "std")]
impl<'a, T> std::ops::Deref for ApiRef<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(feature = "std")]
impl<'a, T> std::ops::DerefMut for ApiRef<'a, T> {
	fn deref_mut(&mut self) -> &mut T {
		&mut self.0
	}
}

/// Something that provides information about a runtime api.
#[cfg(feature = "std")]
pub trait RuntimeApiInfo {
	/// The identifier of the runtime api.
	const ID: [u8; 8];
	/// The version of the runtime api.
	const VERSION: u32;
}

/// The number of bytes required to encode a [`RuntimeApiInfo`].
///
/// 8 bytes for `ID` and 4 bytes for a version.
pub const RUNTIME_API_INFO_SIZE: usize = 12;

/// Crude and simple way to serialize the `RuntimeApiInfo` into a bunch of bytes.
pub const fn serialize_runtime_api_info(id: [u8; 8], version: u32) -> [u8; RUNTIME_API_INFO_SIZE] {
	let version = version.to_le_bytes();

	let mut r = [0; RUNTIME_API_INFO_SIZE];
	r[0] = id[0];
	r[1] = id[1];
	r[2] = id[2];
	r[3] = id[3];
	r[4] = id[4];
	r[5] = id[5];
	r[6] = id[6];
	r[7] = id[7];

	r[8] = version[0];
	r[9] = version[1];
	r[10] = version[2];
	r[11] = version[3];
	r
}

/// Deserialize the runtime API info serialized by [`serialize_runtime_api_info`].
pub fn deserialize_runtime_api_info(bytes: [u8; RUNTIME_API_INFO_SIZE]) -> ([u8; 8], u32) {
	let id: [u8; 8] = bytes[0..8]
		.try_into()
		.expect("the source slice size is equal to the dest array length; qed");

	let version = u32::from_le_bytes(
		bytes[8..12]
			.try_into()
			.expect("the source slice size is equal to the array length; qed"),
	);

	(id, version)
}

decl_runtime_apis! {
	/// The `Core` runtime api that every Substrate runtime needs to implement.
	#[core_trait]
	#[api_version(4)]
	pub trait Core<Block: BlockT> {
		/// Returns the version of the runtime.
		fn version() -> RuntimeVersion;
		/// Execute the given block.
		fn execute_block(block: Block);
		/// Initialize a block with the given header.
		#[renamed("initialise_block", 2)]
		fn initialize_block(header: &Block::Header);
	}

	/// The `Metadata` api trait that returns metadata for the runtime.
	#[api_version(2)]
	pub trait Metadata {
		/// Returns the metadata of a runtime.
		fn metadata() -> OpaqueMetadata;

		/// Returns the metadata at a given version.
		///
		/// If the given `version` isn't supported, this will return `None`.
		/// Use [`Self::metadata_versions`] to find out about supported metadata version of the runtime.
		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata>;

		/// Returns the supported metadata versions.
		///
		/// This can be used to call `metadata_at_version`.
		fn metadata_versions() -> sp_std::vec::Vec<u32>;
	}
}

#[cfg(feature = "std")]
pub struct RuntimeInstanceBuilder<C, Block: BlockT> {
	call_api_at: C,
	block: Block::Hash,
}

#[cfg(feature = "std")]
impl<C, Block: BlockT> RuntimeInstanceBuilder<C, Block> {
	pub fn create(call_api_at: C, block: Block::Hash) -> Self {
		Self { call_api_at, block }
	}

	pub fn on_chain_context(self) -> RuntimeInstanceBuilderStage2<C, Block, DisableProofRecording> {
		RuntimeInstanceBuilderStage2 {
			call_api_at: self.call_api_at,
			block: self.block,
			call_context: CallContext::Onchain,
			with_recorder: DisableProofRecording,
			extensions: Default::default(),
		}
	}

	pub fn off_chain_context(
		self,
	) -> RuntimeInstanceBuilderStage2<C, Block, DisableProofRecording> {
		RuntimeInstanceBuilderStage2 {
			call_api_at: self.call_api_at,
			block: self.block,
			call_context: CallContext::Offchain,
			with_recorder: DisableProofRecording,
			extensions: Default::default(),
		}
	}
}

#[cfg(feature = "std")]
pub struct RuntimeInstanceBuilderStage2<C, Block: BlockT, ProofRecorder> {
	call_api_at: C,
	block: Block::Hash,
	call_context: CallContext,
	with_recorder: ProofRecorder,
	extensions: Extensions,
}

#[cfg(feature = "std")]
impl<C, Block: BlockT, ProofRecording> RuntimeInstanceBuilderStage2<C, Block, ProofRecording> {
	pub fn enable_proof_recording(
		self,
	) -> RuntimeInstanceBuilderStage2<C, Block, EnableProofRecording<Block>> {
		RuntimeInstanceBuilderStage2 {
			with_recorder: Default::default(),
			call_api_at: self.call_api_at,
			block: self.block,
			call_context: self.call_context,
			extensions: self.extensions,
		}
	}

	pub fn with_proof_recording<WProofRecording: crate::ProofRecording<Block>>(
		self,
	) -> RuntimeInstanceBuilderStage2<C, Block, WProofRecording> {
		RuntimeInstanceBuilderStage2 {
			with_recorder: Default::default(),
			call_api_at: self.call_api_at,
			block: self.block,
			call_context: self.call_context,
			extensions: self.extensions,
		}
	}

	pub fn register_extension(mut self, ext: impl Extension) -> Self {
		self.extensions.register(ext);
		self
	}

	pub fn register_optional_extension(mut self, ext: Option<impl Extension>) -> Self {
		if let Some(ext) = ext {
			self.extensions.register(ext);
		}
		self
	}

	pub fn build(self) -> RuntimeInstance<C, Block, ProofRecording>
	where
		C: CallApiAt<Block>,
	{
		RuntimeInstance {
			recorder: self.with_recorder,
			call_api_at: self.call_api_at,
			block: self.block,
			call_context: self.call_context,
			overlayed_changes: Default::default(),
			extensions: self.extensions,
			transaction_depth: 0,
		}
	}
}

#[cfg(feature = "std")]
impl<C, Block: BlockT> RuntimeInstanceBuilderStage2<C, Block, EnableProofRecording<Block>> {
	pub fn proof_recorder(&self) -> ProofRecorder<Block> {
		self.with_recorder.recorder.clone()
	}
}

/// Express that proof recording is enabled.
///
/// For more information see [`ProofRecording`].
#[cfg(feature = "std")]
pub struct EnableProofRecording<Block: BlockT> {
	recorder: ProofRecorder<Block>,
}

#[cfg(feature = "std")]
impl<Block: BlockT> Default for EnableProofRecording<Block> {
	fn default() -> Self {
		Self { recorder: Default::default() }
	}
}

/// Express that proof recording is disabled.
///
/// For more information see [`ProofRecording`].
#[cfg(feature = "std")]
#[derive(Default)]
pub struct DisableProofRecording;

/// A trait to express the state of proof recording on type system level.
///
/// This is used by [`Proposer`] to signal if proof recording is enabled. This can be used by
/// downstream users of the [`Proposer`] trait to enforce that proof recording is activated when
/// required. The only two implementations of this trait are [`DisableProofRecording`] and
/// [`EnableProofRecording`].
///
/// This trait is sealed and can not be implemented outside of this crate!
#[cfg(feature = "std")]
pub trait ProofRecording<Block: BlockT>: private::Sealed + Default + Send + Sync + 'static {
	type Proof: Send + 'static;

	fn extract_proof(&self) -> Self::Proof;

	fn get_recorder(&self) -> Option<&ProofRecorder<Block>>;
}

#[cfg(feature = "std")]
impl<Block: BlockT> ProofRecording<Block> for EnableProofRecording<Block> {
	type Proof = StorageProof;

	fn extract_proof(&self) -> Self::Proof {
		self.recorder.to_storage_proof()
	}

	fn get_recorder(&self) -> Option<&ProofRecorder<Block>> {
		Some(&self.recorder)
	}
}

#[cfg(feature = "std")]
impl<Block: BlockT> ProofRecording<Block> for DisableProofRecording {
	type Proof = ();

	fn extract_proof(&self) -> Self::Proof {
		()
	}

	fn get_recorder(&self) -> Option<&ProofRecorder<Block>> {
		None
	}
}

#[cfg(feature = "std")]
mod private {
	use super::*;

	pub trait Sealed {}

	impl Sealed for DisableProofRecording {}

	impl<Block: BlockT> Sealed for EnableProofRecording<Block> {}
}

#[cfg(feature = "std")]
pub struct RuntimeInstance<C, Block: BlockT, ProofRecorder> {
	call_api_at: C,
	block: Block::Hash,
	call_context: CallContext,
	overlayed_changes: OverlayedChanges<HashingFor<Block>>,
	recorder: ProofRecorder,
	extensions: Extensions,
	transaction_depth: u16,
}

#[cfg(feature = "std")]
impl<C, B: BlockT> RuntimeInstance<C, B, DisableProofRecording> {
	pub fn builder(call_api_at: C, at: B::Hash) -> RuntimeInstanceBuilder<C, B> {
		RuntimeInstanceBuilder { call_api_at, block: at }
	}
}

#[cfg(feature = "std")]
impl<C: CallApiAt<B>, B: BlockT, ProofRecording: crate::ProofRecording<B>>
	RuntimeInstance<C, B, ProofRecording>
{
	pub fn __runtime_api_internal_call_api_at(
		&mut self,
		params: Vec<u8>,
		fn_name: &dyn Fn(RuntimeVersion) -> &'static str,
	) -> Result<Vec<u8>, ApiError> {
		let transaction_depth = self.transaction_depth;

		if transaction_depth == 0 {
			self.start_transaction();
		}

		let res = (|| {
			let version = self.call_api_at.runtime_version_at(self.block)?;

			let params = CallApiAtParams {
				at: self.block,
				function: (*fn_name)(version),
				arguments: params,
				overlayed_changes: &mut self.overlayed_changes,
				call_context: self.call_context,
				recorder: self.recorder.get_recorder(),
				extensions: &mut self.extensions,
			};

			self.call_api_at.call_api_at(params)
		})();

		if transaction_depth == 0 {
			self.commit_or_rollback_transaction(res.is_ok());
		}

		res
	}

	pub fn api_version<Api: ?Sized + RuntimeApiInfo>(&self) -> Result<Option<u32>, ApiError> {
		let version = self.call_api_at.runtime_version_at(self.block)?;
		Ok(version.api_version(&Api::ID))
	}

	pub fn has_api<Api: ?Sized + RuntimeApiInfo>(&self) -> Result<bool, ApiError> {
		let version = self.call_api_at.runtime_version_at(self.block)?;
		Ok(version.has_api_with(&Api::ID, |_| true))
	}

	pub fn execute_in_transaction<R>(
		&mut self,
		inner: impl FnOnce(&mut Self) -> TransactionOutcome<R>,
	) -> R {
		self.start_transaction();

		self.transaction_depth += 1;
		let res = (inner)(self);
		self.transaction_depth
			.checked_sub(1)
			.expect("Transactions are opened and closed together; qed");

		self.commit_or_rollback_transaction(matches!(res, TransactionOutcome::Commit(_)));

		res.into_inner()
	}

	pub fn into_storage_changes(mut self) -> Result<StorageChanges<B>, ApiError> {
		let state_version = self.call_api_at.runtime_version_at(self.block)?;

		self.overlayed_changes
			.drain_storage_changes(
				&self.call_api_at.state_at(self.block)?,
				state_version.state_version(),
			)
			.map_err(|e| ApiError::Application(Box::from(e)))
	}

	fn commit_or_rollback_transaction(&mut self, commit: bool) {
		let proof = "\
					We only close a transaction when we opened one ourself.
					Other parts of the runtime that make use of transactions (state-machine)
					also balance their transactions. The runtime cannot close client initiated
					transactions; qed";

		let res = if commit {
			let res = if let Some(recorder) = self.recorder.get_recorder() {
				recorder.commit_transaction()
			} else {
				Ok(())
			};

			let res2 = self.overlayed_changes.commit_transaction();

			// Will panic on an `Err` below, however we should call commit
			// on the recorder and the changes together.
			res.and(res2.map_err(drop))
		} else {
			let res = if let Some(recorder) = &self.recorder.get_recorder() {
				recorder.rollback_transaction()
			} else {
				Ok(())
			};

			let res2 = self.overlayed_changes.rollback_transaction();

			// Will panic on an `Err` below, however we should call commit
			// on the recorder and the changes together.
			res.and(res2.map_err(drop))
		};

		res.expect(proof)
	}

	fn start_transaction(&mut self) {
		self.overlayed_changes.start_transaction();
		if let Some(recorder) = self.recorder.get_recorder() {
			recorder.start_transaction();
		}
	}

	pub fn extract_proof(&self) -> ProofRecording::Proof {
		self.recorder.extract_proof()
	}
}

#[cfg(feature = "std")]
impl<C: CallApiAt<B>, B: BlockT> RuntimeInstance<C, B, EnableProofRecording<B>> {
	pub fn recorder(&self) -> ProofRecorder<B> {
		self.recorder.recorder.clone()
	}
}

sp_core::generate_feature_enabled_macro!(std_enabled, feature = "std", $);
sp_core::generate_feature_enabled_macro!(std_disabled, not(feature = "std"), $);
