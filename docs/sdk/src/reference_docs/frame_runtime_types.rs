//! # FRAME Runtime Types
//!
//! This reference document briefly explores the idea around types generated at the runtime level by
//! the FRAME macros.
//!
//! > As of now, many of these important types are generated within the internals of
//! > [`construct_runtime`], and there is no easy way for you to visually know they exist.
//! > [#polkadot-sdk#1378](https://github.com/paritytech/polkadot-sdk/pull/1378) is meant to
//! > significantly improve this. Exploring the rust-docs of a runtime, such as [`runtime`] which is
//! > defined in this module is as of now the best way to learn about these types.
//!
//! ## Composite Enums
//!
//! Many types within a FRAME runtime follow the following structure:
//!
//! * Each individual pallet defines a type, for example `Foo`.
//! * At the runtime level, these types are amalgamated into a single type, for example
//!   `RuntimeFoo`.
//!
//! As the names suggest, all composite enums in a FRAME runtime start their name with `Runtime`.
//! For example, `RuntimeCall` is a representation of the most high level `Call`-able type in the
//! runtime.
//!
//! Composite enums are generally convertible to their individual parts as such:
#![doc = simple_mermaid::mermaid!("../../../mermaid/outer_runtime_types.mmd")]
//!
//! In that one can always convert from the inner type into the outer type, but not vice versa. This
//! is usually expressed by implementing `From`, `TryFrom`, `From<Result<_>>` and similar traits.
//!
//! ### Example
//!
//! We provide the following two pallets: [`pallet_foo`] and [`pallet_bar`]. Each define a
//! dispatchable, and `Foo` also defines a custom origin. Lastly, `Bar` defines an additional
//! `GenesisConfig`.
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_foo)]
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_bar)]
//!
//! Let's explore how each of these affect the [`RuntimeCall`], [`RuntimeOrigin`] and
//! [`RuntimeGenesisConfig`] generated in [`runtime`] by respectively.
//!
//! As observed, [`RuntimeCall`] has 3 variants, one for each pallet and one for `frame_system`. If
//! you explore further, you will soon realize that each variant is merely a pointer to the `Call`
//! type in each pallet, for example [`pallet_foo::Call`].
//!
//! [`RuntimeOrigin`]'s [`OriginCaller`] has two variants, one for system, and one for `pallet_foo`
//! which utilized [`frame::pallet_macros::origin`].
//!
//! Finally, [`RuntimeGenesisConfig`] is composed of `frame_system` and a variant for `pallet_bar`'s
//! [`pallet_bar::GenesisConfig`].
//!
//! You can find other composite enums by scanning [`runtime`] for other types who's name starts
//! with `Runtime`. Some of the more noteworthy ones are:
//!
//! - [`RuntimeEvent`]
//! - [`RuntimeError`]
//! - [`RuntimeHoldReason`]
//!
//! ### Adding Further Constraints to Runtime Composite Enums
//!
//! This section explores a common scenario where a pallet has access to one of these runtime
//! composite enums, but it wishes to further specify it by adding more trait bounds to it.
//!
//! Let's take the example of `RuntimeCall`. This is an associated type in
//! [`frame_system::Config::RuntimeCall`], and all pallets have access to this type, because they
//! have access to [`frame_system::Config`]. Finally, this type is meant to be set to outer call of
//! the entire runtime.
//!
//! But, let's not forget that this is information that *we know*, and the Rust compiler does not.
//! All that the rust compiler knows about this type is *ONLY* what the trait bounds of
//! [`frame_system::Config::RuntimeCall`] are specifying:
#![doc = docify::embed!("../../substrate/frame/system/src/lib.rs", system_runtime_call)]
//!
//! So, when at a given pallet, one accesses `<T as frame_system::Config>::RuntimeCall`, the type is
//! extremely opaque from the perspective of the Rust compiler.
//!
//! How can a pallet access the `RuntimeCall` type with further constraints? For example, each
//! pallet has its own `enum Call`, and knows that its local `Call` is a part of `RuntimeCall`,
//! therefore there should be a `impl From<Call<_>> for RuntimeCall`.
//!
//! The only way to express this using Rust's associated types is for the pallet to **define its own
//! associated type `RuntimeCall`, and further specify what it thinks `RuntimeCall` should be**.
//!
//! In this case, we will want to assert the existence of [`frame::traits::IsSubType`], which is
//! very similar to [`TryFrom`].
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", custom_runtime_call)]
//!
//! And indeed, at the runtime level, this associated type would be the same `RuntimeCall` that is
//! passed to `frame_system`.
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_with_specific_runtime_call_impl)]
//!
//! > In other words, the degree of specificity that [`frame_system::Config::RuntimeCall`] has is
//! > not enough for the pallet to work with. Therefore, the pallet has to define its own associated
//! > type representing `RuntimeCall`.
//!
//! Another way to look at this is:
//!
//! `pallet_with_specific_runtime_call::Config::RuntimeCall` and `frame_system::Config::RuntimeCall`
//! are two different representations of the same concrete type that is only known when the runtime
//! is being constructed.
//!
//! Now, within this pallet, this new `RuntimeCall` can be used, and it can use its new trait
//! bounds, such as being [`frame::traits::IsSubType`]:
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", custom_runtime_call_usages)]
//!
//! ### Asserting Equality of Multiple Runtime Composite Enums
//!
//! Recall that in the above example, `<T as Config>::RuntimeCall` and `<T as
//! frame_system::Config>::RuntimeCall` are expected to be equal types, but at the compile-time we
//! have to represent them with two different associated types with different bounds. Would it not
//! be cool if we had a test to make sure they actually resolve to the same concrete type once the
//! runtime is constructed? The following snippet exactly does that:
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", assert_equality)]
//!
//! We leave it to the reader to further explore what [`frame::traits::Hooks::integrity_test`] is,
//! and what [`core::any::TypeId`] is. Another way to assert this is using
//! [`frame::traits::IsType`].
//!
//! ## Type Aliases
//!
//! A number of type aliases are generated by the `construct_runtime` which are also noteworthy:
//!
//! * [`runtime::PalletFoo`] is an alias to [`pallet_foo::Pallet`]. Same for `PalletBar`, and
//!   `System`
//! * [`runtime::AllPalletsWithSystem`] is an alias for a tuple of all of the above. This type is
//!   important to FRAME internals such as `executive`, as it implements traits such as
//!   [`frame::traits::Hooks`].
//!
//! ## Further Details
//!
//! * [`crate::reference_docs::frame_origin`] explores further details about the usage of
//!   `RuntimeOrigin`.
//! * [`RuntimeCall`] is a particularly interesting composite enum as it dictates the encoding of an
//!   extrinsic. See [`crate::reference_docs::signed_extensions`] for more information.
//! * See the documentation of [`construct_runtime`].
//! * See the corresponding lecture in the [pba-book](https://polkadot-blockchain-academy.github.io/pba-book/frame/outer-enum/page.html).
//!
//!
//! [`construct_runtime`]: frame::runtime::prelude::construct_runtime
//! [`runtime::PalletFoo`]: crate::reference_docs::frame_runtime_types::runtime::PalletFoo
//! [`runtime::AllPalletsWithSystem`]: crate::reference_docs::frame_runtime_types::runtime::AllPalletsWithSystem
//! [`runtime`]: crate::reference_docs::frame_runtime_types::runtime
//! [`pallet_foo`]: crate::reference_docs::frame_runtime_types::pallet_foo
//! [`pallet_foo::Call`]: crate::reference_docs::frame_runtime_types::pallet_foo::Call
//! [`pallet_foo::Pallet`]: crate::reference_docs::frame_runtime_types::pallet_foo::Pallet
//! [`pallet_bar`]: crate::reference_docs::frame_runtime_types::pallet_bar
//! [`pallet_bar::GenesisConfig`]: crate::reference_docs::frame_runtime_types::pallet_bar::GenesisConfig
//! [`RuntimeEvent`]: crate::reference_docs::frame_runtime_types::runtime::RuntimeEvent
//! [`RuntimeGenesisConfig`]:
//!     crate::reference_docs::frame_runtime_types::runtime::RuntimeGenesisConfig
//! [`RuntimeOrigin`]: crate::reference_docs::frame_runtime_types::runtime::RuntimeOrigin
//! [`OriginCaller`]: crate::reference_docs::frame_runtime_types::runtime::OriginCaller
//! [`RuntimeError`]: crate::reference_docs::frame_runtime_types::runtime::RuntimeError
//! [`RuntimeCall`]: crate::reference_docs::frame_runtime_types::runtime::RuntimeCall
//! [`RuntimeHoldReason`]: crate::reference_docs::frame_runtime_types::runtime::RuntimeHoldReason

use frame::prelude::*;

#[docify::export]
#[frame::pallet(dev_mode)]
pub mod pallet_foo {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::origin]
	#[derive(PartialEq, Eq, Clone, RuntimeDebug, Encode, Decode, TypeInfo, MaxEncodedLen)]
	pub enum Origin {
		A,
		B,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn foo(_origin: OriginFor<T>) -> DispatchResult {
			todo!();
		}

		pub fn other(_origin: OriginFor<T>) -> DispatchResult {
			todo!();
		}
	}
}

#[docify::export]
#[frame::pallet(dev_mode)]
pub mod pallet_bar {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub initial_account: Option<T::AccountId>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn bar(_origin: OriginFor<T>) -> DispatchResult {
			todo!();
		}
	}
}

pub mod runtime {
	use super::{pallet_bar, pallet_foo};
	use frame::{runtime::prelude::*, testing_prelude::*};

	#[docify::export(runtime_exp)]
	construct_runtime!(
		pub struct Runtime {
			System: frame_system,
			PalletFoo: pallet_foo,
			PalletBar: pallet_bar,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = MockBlock<Self>;
	}

	impl pallet_foo::Config for Runtime {}
	impl pallet_bar::Config for Runtime {}
}

#[frame::pallet(dev_mode)]
pub mod pallet_with_specific_runtime_call {
	use super::*;
	use frame::traits::IsSubType;

	#[docify::export(custom_runtime_call)]
	/// A pallet that wants to further narrow down what `RuntimeCall` is.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeCall: IsSubType<Call<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// note that this pallet needs some `call` to have a `enum Call`.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn foo(_origin: OriginFor<T>) -> DispatchResult {
			todo!();
		}
	}

	#[docify::export(custom_runtime_call_usages)]
	impl<T: Config> Pallet<T> {
		fn _do_something_useful_with_runtime_call(call: <T as Config>::RuntimeCall) {
			// check if the runtime call given is of this pallet's variant.
			let _maybe_my_call: Option<&Call<T>> = call.is_sub_type();
			todo!();
		}
	}

	#[docify::export(assert_equality)]
	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			use core::any::TypeId;
			assert_eq!(
				TypeId::of::<<T as Config>::RuntimeCall>(),
				TypeId::of::<<T as frame_system::Config>::RuntimeCall>()
			);
		}
	}
}

pub mod runtime_with_specific_runtime_call {
	use super::pallet_with_specific_runtime_call;
	use frame::{runtime::prelude::*, testing_prelude::*};

	construct_runtime!(
		pub struct Runtime {
			System: frame_system,
			PalletWithSpecificRuntimeCall: pallet_with_specific_runtime_call,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = MockBlock<Self>;
	}

	#[docify::export(pallet_with_specific_runtime_call_impl)]
	impl pallet_with_specific_runtime_call::Config for Runtime {
		// an implementation of `IsSubType` is provided by `construct_runtime`.
		type RuntimeCall = RuntimeCall;
	}
}
