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

//! # FRAME
//!
//! ```no_compile
//!   ______   ______    ________   ___ __ __   ______
//!  /_____/\ /_____/\  /_______/\ /__//_//_/\ /_____/\
//!  \::::_\/_\:::_ \ \ \::: _  \ \\::\| \| \ \\::::_\/_
//!   \:\/___/\\:(_) ) )_\::(_)  \ \\:.      \ \\:\/___/\
//!    \:::._\/ \: __ `\ \\:: __  \ \\:.\-/\  \ \\::___\/_
//!     \:\ \    \ \ `\ \ \\:.\ \  \ \\. \  \  \ \\:\____/\
//!      \_\/     \_\/ \_\/ \__\/\__\/ \__\/ \__\/ \_____\/
//! ```
//!
//! > **F**ramework for **R**untime **A**ggregation of **M**odularized **E**ntities: Substrate's
//! > State Transition Function (Runtime) Framework.
//!
//! ## Usage
//!
//! This crate is organized into 3 stages:
//!
//! 1. preludes: `prelude`, `testing_prelude` and `runtime::prelude`, `benchmarking`,
//!    `weights_prelude`, `try_runtime`.
//! 2. domain-specific modules: `traits`, `hashing`, `arithmetic` and `derive`.
//! 3. Accessing frame/substrate dependencies directly: `deps`.
//!
//! The main intended use of this crate is for it to be used with the former, preludes:
//!
//! ```
//! use polkadot_sdk_frame as frame;
//! #[frame::pallet]
//! pub mod pallet {
//! 	# use polkadot_sdk_frame as frame;
//! 	use frame::prelude::*;
//! 	// ^^ using the prelude!
//!
//! 	#[pallet::config]
//! 	pub trait Config: frame_system::Config {}
//!
//! 	#[pallet::pallet]
//! 	pub struct Pallet<T>(_);
//! }
//!
//! #[cfg(test)]
//! pub mod tests {
//! 	# use polkadot_sdk_frame as frame;
//! 	use frame::testing_prelude::*;
//! }
//!
//! #[cfg(feature = "runtime-benchmarks")]
//! pub mod benchmarking {
//! 	# use polkadot_sdk_frame as frame;
//! 	use frame::benchmarking::prelude::*;
//! }
//!
//! pub mod runtime {
//! 	# use polkadot_sdk_frame as frame;
//! 	use frame::runtime::prelude::*;
//! }
//! ```
//!
//! If not in preludes, one can look into the domain-specific modules. Finally, if an import is
//! still not feasible, one can look into `deps`.
//!
//! This crate also uses a `runtime` feature to include all of the types and tools needed to build
//! FRAME-based runtimes. So, if you want to build a runtime with this, import it as
//!
//! ```text
//! polkadot-sdk-frame = { version = "foo", features = ["runtime"] }
//! ```
//!
//! If you just want to build a pallet instead, import it as
//!
//! ```text
//! polkadot-sdk-frame = { version = "foo" }
//! ```
//!
//! Notice that the preludes overlap since they have imports in common. More in detail:
//! - `testing_prelude` brings in frame `prelude` and `runtime::prelude`;
//! - `runtime::prelude` brings in frame `prelude`;
//! - `benchmarking` brings in frame `prelude`.
//!
//! ## Naming
//!
//! Please note that this crate can only be imported as `polkadot-sdk-frame` or `frame`. This is due
//! to compatibility matters with `frame-support`.
//!
//! A typical pallet's `Cargo.toml` using this crate looks like:
//!
//! ```ignore
//! [dependencies]
//! codec = { features = ["max-encoded-len"], workspace = true }
//! scale-info = { features = ["derive"], workspace = true }
//! frame = { workspace = true, features = ["experimental", "runtime"] }
//!
//! [features]
//! default = ["std"]
//! std = [
//! 	"codec/std",
//! 	"scale-info/std",
//! 	"frame/std",
//! ]
//! runtime-benchmarks = [
//! 	"frame/runtime-benchmarks",
//! ]
//! try-runtime = [
//! 	"frame/try-runtime",
//! ]
//! ```
//!
//! ## Documentation
//!
//! See [`polkadot_sdk::frame`](../polkadot_sdk_docs/polkadot_sdk/frame_runtime/index.html).
//!
//! ## WARNING: Experimental
//!
//! **This crate and all of its content is experimental, and should not yet be used in production.**
//!
//! ## Maintenance Note
//!
//! > Notes for the maintainers of this crate, describing how the re-exports and preludes should
//! > work.
//!
//! * Preludes should be extensive. The goal of this pallet is to be ONLY used with the preludes.
//!   The domain-specific modules are just a backup, aiming to keep things organized. Don't hesitate
//!   in adding more items to the main prelude.
//! * The only non-module, non-prelude items exported from the top level crate is the `pallet`
//!   macro, such that we can have the `#[frame::pallet] mod pallet { .. }` syntax working.
//! * In most cases, you might want to create a domain-specific module, but also add it to the
//!   preludes, such as `hashing`.
//! * The only items that should NOT be in preludes are those that have been placed in
//!   `frame-support`/`sp-runtime`, but in truth are related to just one pallet.
//! * The currency related traits are kept out of the preludes to encourage a deliberate choice of
//!   one over the other.
//! * `runtime::apis` should expose all common runtime APIs that all FRAME-based runtimes need.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg(feature = "experimental")]

#[doc(no_inline)]
pub use frame_support::pallet;

#[doc(no_inline)]
pub use frame_support::pallet_macros::{import_section, pallet_section};

/// The logging library of the runtime. Can normally be the classic `log` crate.
pub use log;

#[doc(inline)]
pub use frame_support::storage_alias;

/// Macros used within the main [`pallet`] macro.
///
/// Note: All of these macros are "stubs" and not really usable outside `#[pallet] mod pallet { ..
/// }`. They are mainly provided for documentation and IDE support.
///
/// To view a list of all the macros and their documentation, follow the links in the 'Re-exports'
/// section below:
pub mod pallet_macros {
	#[doc(no_inline)]
	pub use frame_support::{derive_impl, pallet, pallet_macros::*};
}

/// The main prelude of FRAME.
///
/// This prelude should almost always be the first line of code in any pallet or runtime.
///
/// ```
/// use polkadot_sdk_frame::prelude::*;
///
/// // rest of your pallet..
/// mod pallet {}
/// ```
pub mod prelude {
	/// `frame_system`'s parent crate, which is mandatory in all pallets build with this crate.
	///
	/// Conveniently, the keyword `frame_system` is in scope as one uses `use
	/// polkadot_sdk_frame::prelude::*`
	#[doc(inline)]
	pub use frame_system;

	/// Pallet prelude of `frame-support`.
	///
	/// Note: this needs to revised once `frame-support` evolves.
	#[doc(no_inline)]
	pub use frame_support::pallet_prelude::*;

	/// Dispatch types from `frame-support`, other fundamental traits
	#[doc(no_inline)]
	pub use frame_support::dispatch::{GetDispatchInfo, PostDispatchInfo};
	pub use frame_support::traits::{Contains, IsSubType, OnRuntimeUpgrade};

	/// Pallet prelude of `frame-system`.
	#[doc(no_inline)]
	pub use frame_system::pallet_prelude::*;

	/// All FRAME-relevant derive macros.
	#[doc(no_inline)]
	pub use super::derive::*;

	/// All hashing related things
	pub use super::hashing::*;

	/// Runtime traits
	#[doc(no_inline)]
	pub use sp_runtime::traits::{
		Bounded, DispatchInfoOf, Dispatchable, SaturatedConversion, Saturating, StaticLookup,
		TrailingZeroInput,
	};

	/// Other error/result types for runtime
	#[doc(no_inline)]
	pub use sp_runtime::{DispatchErrorWithPostInfo, DispatchResultWithInfo, TokenError};
}

#[cfg(any(feature = "try-runtime", test))]
pub mod try_runtime {
	pub use sp_runtime::TryRuntimeError;
}

/// Prelude to be included in the `benchmarking.rs` of a pallet.
///
/// It supports both the `benchmarking::v1::benchmarks` and `benchmarking::v2::benchmark` syntax.
///
/// ```
/// use polkadot_sdk_frame::benchmarking::prelude::*;
/// // rest of your code.
/// ```
///
/// It already includes `polkadot_sdk_frame::prelude::*` and `polkadot_sdk_frame::testing_prelude`.
#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking {
	mod shared {
		pub use frame_benchmarking::{add_benchmark, v1::account, whitelist, whitelisted_caller};
		// all benchmarking functions.
		pub use frame_benchmarking::benchmarking::*;
		// The system origin, which is very often needed in benchmarking code. Might be tricky only
		// if the pallet defines its own `#[pallet::origin]` and call it `RawOrigin`.
		pub use frame_system::RawOrigin;
	}

	#[deprecated(
		note = "'The V1 benchmarking syntax is deprecated. Please use the V2 syntax. This warning may become a hard error any time after April 2025. For more info, see: https://github.com/paritytech/polkadot-sdk/pull/5995"
	)]
	pub mod v1 {
		pub use super::shared::*;
		pub use frame_benchmarking::benchmarks;
	}

	pub mod prelude {
		pub use super::shared::*;
		pub use crate::prelude::*;
		pub use frame_benchmarking::v2::*;
	}
}

/// Prelude to be included in the `weight.rs` of each pallet.
///
/// ```
/// pub use polkadot_sdk_frame::weights_prelude::*;
/// ```
pub mod weights_prelude {
	pub use core::marker::PhantomData;
	pub use frame_support::{
		traits::Get,
		weights::{
			constants::{ParityDbWeight, RocksDbWeight},
			Weight,
		},
	};
	pub use frame_system;
}

/// The main testing prelude of FRAME.
///
/// A test setup typically starts with:
///
/// ```
/// use polkadot_sdk_frame::testing_prelude::*;
/// // rest of your test setup.
/// ```
///
/// This automatically brings in `polkadot_sdk_frame::prelude::*` and
/// `polkadot_sdk_frame::runtime::prelude::*`.
#[cfg(feature = "std")]
pub mod testing_prelude {
	pub use crate::{prelude::*, runtime::prelude::*};

	/// Testing includes building a runtime, so we bring in all preludes related to runtimes as
	/// well.
	pub use super::runtime::testing_prelude::*;

	/// Other helper macros from `frame_support` that help with asserting in tests.
	pub use frame_support::{
		assert_err, assert_err_ignore_postinfo, assert_error_encoded_size, assert_noop, assert_ok,
		assert_storage_noop, storage_alias,
	};

	pub use frame_system::{self, mocking::*};

	#[deprecated(note = "Use `frame::testing_prelude::TestExternalities` instead.")]
	pub use sp_io::TestExternalities;

	pub use sp_io::TestExternalities as TestState;
}

/// All of the types and tools needed to build FRAME-based runtimes.
#[cfg(any(feature = "runtime", feature = "std"))]
pub mod runtime {
	/// The main prelude of `FRAME` for building runtimes.
	///
	/// A runtime typically starts with:
	///
	/// ```
	/// use polkadot_sdk_frame::runtime::prelude::*;
	/// ```
	///
	/// This automatically brings in `polkadot_sdk_frame::prelude::*`.
	pub mod prelude {
		pub use crate::prelude::*;

		/// All of the types related to the FRAME runtime executive.
		pub use frame_executive::*;

		/// Macro to amalgamate the runtime into `struct Runtime`.
		///
		/// Consider using the new version of this [`frame_construct_runtime`].
		pub use frame_support::construct_runtime;

		/// Macro to amalgamate the runtime into `struct Runtime`.
		///
		/// This is the newer version of [`construct_runtime`].
		pub use frame_support::runtime as frame_construct_runtime;

		/// Macro to easily derive the `Config` trait of various pallet for `Runtime`.
		pub use frame_support::derive_impl;

		/// Macros to easily impl traits such as `Get` for types.
		// TODO: using linking in the Get in the line above triggers an ICE :/
		pub use frame_support::{ord_parameter_types, parameter_types};

		/// For building genesis config.
		pub use frame_support::genesis_builder_helper::{build_state, get_preset};

		/// Const types that can easily be used in conjuncture with `Get`.
		pub use frame_support::traits::{
			ConstBool, ConstI128, ConstI16, ConstI32, ConstI64, ConstI8, ConstU128, ConstU16,
			ConstU32, ConstU64, ConstU8,
		};

		/// Used for simple fee calculation.
		pub use frame_support::weights::{self, FixedFee, NoFee};

		/// Primary types used to parameterize `EnsureOrigin` and `EnsureRootWithArg`.
		pub use frame_system::{
			EnsureNever, EnsureNone, EnsureRoot, EnsureRootWithSuccess, EnsureSigned,
			EnsureSignedBy,
		};

		/// Types to define your runtime version.
		pub use sp_version::{create_runtime_str, runtime_version, RuntimeVersion};

		#[cfg(feature = "std")]
		pub use sp_version::NativeVersion;

		/// Macro to implement runtime APIs.
		pub use sp_api::impl_runtime_apis;

		// Types often used in the runtime APIs.
		pub use sp_core::OpaqueMetadata;
		pub use sp_genesis_builder::{
			PresetId, Result as GenesisBuilderResult, DEV_RUNTIME_PRESET,
			LOCAL_TESTNET_RUNTIME_PRESET,
		};
		pub use sp_inherents::{CheckInherentsResult, InherentData};
		pub use sp_keyring::AccountKeyring;
		pub use sp_runtime::{ApplyExtrinsicResult, ExtrinsicInclusionMode};
	}

	/// Types and traits for runtimes that implement runtime APIs.
	///
	/// A testing runtime should not need this.
	///
	/// A non-testing runtime should have this enabled, as such:
	///
	/// ```
	/// use polkadot_sdk_frame::runtime::{prelude::*, apis::{*,}};
	/// ```
	// TODO: This is because of wildcard imports, and it should be not needed once we can avoid
	// that. Imports like that are needed because we seem to need some unknown types in the macro
	// expansion. See `sp_session::runtime_api::*;` as one example. All runtime api decls should be
	// moved to file similarly.
	#[allow(ambiguous_glob_reexports)]
	pub mod apis {
		pub use frame_system_rpc_runtime_api::*;
		pub use sp_api::{self, *};
		pub use sp_block_builder::*;
		pub use sp_consensus_aura::*;
		pub use sp_consensus_grandpa::*;
		pub use sp_genesis_builder::*;
		pub use sp_offchain::*;
		pub use sp_session::runtime_api::*;
		pub use sp_transaction_pool::runtime_api::*;
	}

	/// A set of opinionated types aliases commonly used in runtimes.
	///
	/// This is one set of opinionated types. They are compatible with one another, but are not
	/// guaranteed to work if you start tweaking a portion.
	///
	/// Some note-worthy opinions in this prelude:
	///
	/// - `u32` block number.
	/// - [`sp_runtime::MultiAddress`] and [`sp_runtime::MultiSignature`] are used as the account id
	///   and signature types. This implies that this prelude can possibly used with an
	///   "account-index" system (eg `pallet-indices`). And, in any case, it should be paired with
	///   `AccountIdLookup` in [`frame_system::Config::Lookup`].
	pub mod types_common {
		use frame_system::Config as SysConfig;
		use sp_runtime::{generic, traits, OpaqueExtrinsic};

		/// A signature type compatible capably of handling multiple crypto-schemes.
		pub type Signature = sp_runtime::MultiSignature;

		/// The corresponding account-id type of [`Signature`].
		pub type AccountId =
			<<Signature as traits::Verify>::Signer as traits::IdentifyAccount>::AccountId;

		/// The block-number type, which should be fed into [`frame_system::Config`].
		pub type BlockNumber = u32;

		/// TODO: Ideally we want the hashing type to be equal to SysConfig::Hashing?
		type HeaderInner = generic::Header<BlockNumber, traits::BlakeTwo256>;

		// NOTE: `AccountIndex` is provided for future compatibility, if you want to introduce
		// something like `pallet-indices`.
		type ExtrinsicInner<T, Extra, AccountIndex = ()> = generic::UncheckedExtrinsic<
			sp_runtime::MultiAddress<AccountId, AccountIndex>,
			<T as SysConfig>::RuntimeCall,
			Signature,
			Extra,
		>;

		/// The block type, which should be fed into [`frame_system::Config`].
		///
		/// Should be parameterized with `T: frame_system::Config` and a tuple of
		/// `TransactionExtension`. When in doubt, use [`SystemTransactionExtensionsOf`].
		// Note that this cannot be dependent on `T` for block-number because it would lead to a
		// circular dependency (self-referential generics).
		pub type BlockOf<T, Extra = ()> = generic::Block<HeaderInner, ExtrinsicInner<T, Extra>>;

		/// The opaque block type. This is the same [`BlockOf`], but it has
		/// [`sp_runtime::OpaqueExtrinsic`] as its final extrinsic type.
		///
		/// This should be provided to the client side as the extrinsic type.
		pub type OpaqueBlock = generic::Block<HeaderInner, OpaqueExtrinsic>;

		/// Default set of signed extensions exposed from the `frame_system`.
		///
		/// crucially, this does NOT contain any tx-payment extension.
		pub type SystemTransactionExtensionsOf<T> = (
			frame_system::CheckNonZeroSender<T>,
			frame_system::CheckSpecVersion<T>,
			frame_system::CheckTxVersion<T>,
			frame_system::CheckGenesis<T>,
			frame_system::CheckEra<T>,
			frame_system::CheckNonce<T>,
			frame_system::CheckWeight<T>,
		);
	}

	/// The main prelude of FRAME for building runtimes, and in the context of testing.
	///
	/// counter part of `runtime::prelude`.
	#[cfg(feature = "std")]
	pub mod testing_prelude {
		pub use sp_core::storage::Storage;
		pub use sp_runtime::BuildStorage;
	}
}

/// All traits often used in FRAME pallets.
///
/// Note that types implementing these traits can also be found in this module.
// TODO: `Hash` and `Bounded` are defined multiple times; should be fixed once these two crates are
// cleaned up.
#[allow(ambiguous_glob_reexports)]
pub mod traits {
	pub use frame_support::traits::*;
	pub use sp_runtime::traits::*;
}

/// The arithmetic types used for safe math.
pub mod arithmetic {
	pub use sp_arithmetic::{traits::*, *};
}

/// All derive macros used in frame.
///
/// This is already part of the [`prelude`].
pub mod derive {
	pub use codec::{Decode, Encode};
	pub use core::fmt::Debug;
	pub use frame_support::{
		CloneNoBound, DebugNoBound, DefaultNoBound, EqNoBound, OrdNoBound, PartialEqNoBound,
		PartialOrdNoBound, RuntimeDebugNoBound,
	};
	pub use scale_info::TypeInfo;
	pub use sp_runtime::RuntimeDebug;
}

pub mod hashing {
	pub use sp_core::{hashing::*, H160, H256, H512, U256, U512};
	pub use sp_runtime::traits::{BlakeTwo256, Hash, Keccak256};
}

/// Access to all of the dependencies of this crate. In case the prelude re-exports are not enough,
/// this module can be used.
///
/// Note for maintainers: Any time one uses this module to access a dependency, you can have a
/// moment to think about whether this item could have been placed in any of the other modules and
/// preludes in this crate. In most cases, hopefully the answer is yes.
pub mod deps {
	// TODO: It would be great to somehow instruct RA to prefer *not* suggesting auto-imports from
	// these. For example, we prefer `polkadot_sdk_frame::derive::CloneNoBound` rather than
	// `polkadot_sdk_frame::deps::frame_support::CloneNoBound`.
	pub use frame_support;
	pub use frame_system;

	pub use sp_arithmetic;
	pub use sp_core;
	pub use sp_io;
	pub use sp_runtime;

	pub use codec;
	pub use scale_info;

	#[cfg(feature = "runtime")]
	pub use frame_executive;
	#[cfg(feature = "runtime")]
	pub use sp_api;
	#[cfg(feature = "runtime")]
	pub use sp_block_builder;
	#[cfg(feature = "runtime")]
	pub use sp_consensus_aura;
	#[cfg(feature = "runtime")]
	pub use sp_consensus_grandpa;
	#[cfg(feature = "runtime")]
	pub use sp_genesis_builder;
	#[cfg(feature = "runtime")]
	pub use sp_inherents;
	#[cfg(feature = "runtime")]
	pub use sp_keyring;
	#[cfg(feature = "runtime")]
	pub use sp_offchain;
	#[cfg(feature = "runtime")]
	pub use sp_storage;
	#[cfg(feature = "runtime")]
	pub use sp_version;

	#[cfg(feature = "runtime-benchmarks")]
	pub use frame_benchmarking;
	#[cfg(feature = "runtime-benchmarks")]
	pub use frame_system_benchmarking;

	#[cfg(feature = "frame-try-runtime")]
	pub use frame_try_runtime;
}
