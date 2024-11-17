//! # FRAME Runtime Types
//!
//!
//! > significantly improve this. Exploring the rust-docs of a runtime, such as [`runtime`] which is
//!
//!
//!
//!
//!
#![doc = simple_mermaid::mermaid!("../../../mermaid/outer_runtime_types.mmd")]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_foo)]
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_bar)]
//!
//!
//!
//!
//!
//!
//!
//!
//!
//! the entire runtime.
//!
#![doc = docify::embed!("../../substrate/frame/system/src/lib.rs", system_runtime_call)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", custom_runtime_call)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_with_specific_runtime_call_impl)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", custom_runtime_call_usages)]
//!
//!
//!
//! be cool if we had a test to make sure they actually resolve to the same concrete type once the
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", assert_equality)]
//!
//!
//!
//!
//!   important to FRAME internals such as `executive`, as it implements traits such as
//!
//!
//!   extrinsic. See [`transaction_extensions`] for more information.
//!
//!
//! [`runtime`]: crate::reference_docs::frame_runtime_types::runtime
//! [`pallet_bar`]: crate::reference_docs::frame_runtime_types::pallet_bar
//!     crate::reference_docs::frame_runtime_types::runtime::RuntimeGenesisConfig
//! [`RuntimeCall`]: crate::reference_docs::frame_runtime_types::runtime::RuntimeCall

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

// Link References

// Link References







//!
//!
//! > significantly improve this. Exploring the rust-docs of a runtime, such as [`runtime`] which is
//!
//!
//!
//!
//!
#![doc = simple_mermaid::mermaid!("../../../mermaid/outer_runtime_types.mmd")]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_foo)]
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_bar)]
//!
//!
//!
//!
//!
//!
//!
//!
//!
//! the entire runtime.
//!
#![doc = docify::embed!("../../substrate/frame/system/src/lib.rs", system_runtime_call)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", custom_runtime_call)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_with_specific_runtime_call_impl)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", custom_runtime_call_usages)]
//!
//!
//!
//! be cool if we had a test to make sure they actually resolve to the same concrete type once the
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", assert_equality)]
//!
//!
//!
//!
//!   important to FRAME internals such as `executive`, as it implements traits such as
//!
//!
//!   extrinsic. See [`transaction_extensions`] for more information.
//!
//!
//! [`runtime`]: crate::reference_docs::frame_runtime_types::runtime
//! [`pallet_bar`]: crate::reference_docs::frame_runtime_types::pallet_bar
//!     crate::reference_docs::frame_runtime_types::runtime::RuntimeGenesisConfig
//! [`RuntimeCall`]: crate::reference_docs::frame_runtime_types::runtime::RuntimeCall

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

// Link References

// Link References








//!
//!
//! > significantly improve this. Exploring the rust-docs of a runtime, such as [`runtime`] which is
//!
//!
//!
//!
//!
#![doc = simple_mermaid::mermaid!("../../../mermaid/outer_runtime_types.mmd")]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_foo)]
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_bar)]
//!
//!
//!
//!
//!
//!
//!
//!
//!
//! the entire runtime.
//!
#![doc = docify::embed!("../../substrate/frame/system/src/lib.rs", system_runtime_call)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", custom_runtime_call)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_with_specific_runtime_call_impl)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", custom_runtime_call_usages)]
//!
//!
//!
//! be cool if we had a test to make sure they actually resolve to the same concrete type once the
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", assert_equality)]
//!
//!
//!
//!
//!   important to FRAME internals such as `executive`, as it implements traits such as
//!
//!
//!   extrinsic. See [`transaction_extensions`] for more information.
//!
//!
//! [`runtime`]: crate::reference_docs::frame_runtime_types::runtime
//! [`pallet_bar`]: crate::reference_docs::frame_runtime_types::pallet_bar
//!     crate::reference_docs::frame_runtime_types::runtime::RuntimeGenesisConfig
//! [`RuntimeCall`]: crate::reference_docs::frame_runtime_types::runtime::RuntimeCall

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

// Link References

// Link References







//!
//!
//! > significantly improve this. Exploring the rust-docs of a runtime, such as [`runtime`] which is
//!
//!
//!
//!
//!
#![doc = simple_mermaid::mermaid!("../../../mermaid/outer_runtime_types.mmd")]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_foo)]
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_bar)]
//!
//!
//!
//!
//!
//!
//!
//!
//!
//! the entire runtime.
//!
#![doc = docify::embed!("../../substrate/frame/system/src/lib.rs", system_runtime_call)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", custom_runtime_call)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", pallet_with_specific_runtime_call_impl)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", custom_runtime_call_usages)]
//!
//!
//!
//! be cool if we had a test to make sure they actually resolve to the same concrete type once the
#![doc = docify::embed!("./src/reference_docs/frame_runtime_types.rs", assert_equality)]
//!
//!
//!
//!
//!   important to FRAME internals such as `executive`, as it implements traits such as
//!
//!
//!   extrinsic. See [`transaction_extensions`] for more information.
//!
//!
//! [`runtime`]: crate::reference_docs::frame_runtime_types::runtime
//! [`pallet_bar`]: crate::reference_docs::frame_runtime_types::pallet_bar
//!     crate::reference_docs::frame_runtime_types::runtime::RuntimeGenesisConfig
//! [`RuntimeCall`]: crate::reference_docs::frame_runtime_types::runtime::RuntimeCall

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

// Link References

// Link References












// [`pallet_bar`]: pallet_bar

// [`frame_origin`]: frame_origin
// [`pallet_bar`]: pallet_bar
// [`pallet_foo`]: pallet_foo
// [`pba-book`]: https://polkadot-blockchain-academy.github.io/pba-book/frame/outer-enum/page.html
// [`this`]: https://github.com/paritytech/polkadot-sdk/issues/3743
