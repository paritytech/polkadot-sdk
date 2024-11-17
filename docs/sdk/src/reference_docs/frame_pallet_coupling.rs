//! # FRAME Pallet Coupling
//!
//!
//!
//!
//!
//!
//!
//!
//! * *linked to other pallets*: But, adhering extensively to the above also hinders the ability to
//!
//!
//!
//!
//!
//!
//!
//! ```
//!
//! of `F`, which may be `B`, or another implementation of `F`.
//!
//!
//!    type F: F;
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_foo)]
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_author)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", tight_config)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", tight_usage)]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", AuthorProvider)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", loose_config)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", loose_usage)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_author_provider)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", runtime_author_provider)]
//!
//! module, you can find [`OtherAuthorProvider`], which is an alternative implementation of
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", other_author_provider)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", unit_author_provider)]
//!
//!
//! [`AccountId`],
//!
//!
//!
//!   dispatchables.
//!   be foreseen, consider loosely coupling pallets.
//!
//! with balances or assets pallet. More on this in [`frame_tokens`].
//!
//!
//!

#![allow(unused)]

use frame::prelude::*;

#[docify::export]
#[frame::pallet]
pub mod pallet_foo {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	impl<T: Config> Pallet<T> {
		fn do_stuff_with_author() {
			// needs block author here
		}
	}
}

#[docify::export]
#[frame::pallet]
pub mod pallet_author {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	impl<T: Config> Pallet<T> {
		pub fn author() -> T::AccountId {
			todo!("somehow has access to the block author and can return it here")
		}
	}
}

#[frame::pallet]
pub mod pallet_foo_tight {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(tight_config)]
	/// This pallet can only live in a runtime that has both `frame_system` and `pallet_author`.
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_author::Config {}

	#[docify::export(tight_usage)]
	impl<T: Config> Pallet<T> {
		// anywhere in `pallet-foo`, we can call into `pallet-author` directly, namely because
		// `T: pallet_author::Config`
		fn do_stuff_with_author() {
			let _ = pallet_author::Pallet::<T>::author();
		}
	}
}

#[docify::export]
/// Abstraction over "something that can provide the block author".
pub trait AuthorProvider<AccountId> {
	fn author() -> AccountId;
}

#[frame::pallet]
pub mod pallet_foo_loose {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(loose_config)]
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// This pallet relies on the existence of something that implements [`AuthorProvider`],
		/// which may or may not be `pallet-author`.
		type AuthorProvider: AuthorProvider<Self::AccountId>;
	}

	#[docify::export(loose_usage)]
	impl<T: Config> Pallet<T> {
		fn do_stuff_with_author() {
			let _ = T::AuthorProvider::author();
		}
	}
}

#[docify::export(pallet_author_provider)]
impl<T: pallet_author::Config> AuthorProvider<T::AccountId> for pallet_author::Pallet<T> {
	fn author() -> T::AccountId {
		pallet_author::Pallet::<T>::author()
	}
}

pub struct OtherAuthorProvider;

#[docify::export(other_author_provider)]
impl<AccountId> AuthorProvider<AccountId> for OtherAuthorProvider {
	fn author() -> AccountId {
		todo!("somehow get the block author here")
	}
}

#[docify::export(unit_author_provider)]
impl<AccountId> AuthorProvider<AccountId> for () {
	fn author() -> AccountId {
		todo!("somehow get the block author here")
	}
}

pub mod runtime {
	use super::*;
	use cumulus_pallet_aura_ext::pallet;
	use frame::{runtime::prelude::*, testing_prelude::*};

	construct_runtime!(
		pub struct Runtime {
			System: frame_system,
			PalletFoo: pallet_foo_loose,
			PalletAuthor: pallet_author,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = MockBlock<Self>;
	}

	impl pallet_author::Config for Runtime {}

	#[docify::export(runtime_author_provider)]
	impl pallet_foo_loose::Config for Runtime {
		type AuthorProvider = pallet_author::Pallet<Runtime>;
		// which is also equivalent to
		// type AuthorProvider = PalletAuthor;
	}
}

// Link References

// Link References







//!
//!
//!
//!
//!
//!
//!
//!
//! * *linked to other pallets*: But, adhering extensively to the above also hinders the ability to
//!
//!
//!
//!
//!
//!
//!
//! ```
//!
//! of `F`, which may be `B`, or another implementation of `F`.
//!
//!
//!    type F: F;
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_foo)]
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_author)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", tight_config)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", tight_usage)]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", AuthorProvider)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", loose_config)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", loose_usage)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_author_provider)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", runtime_author_provider)]
//!
//! module, you can find [`OtherAuthorProvider`], which is an alternative implementation of
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", other_author_provider)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", unit_author_provider)]
//!
//!
//! [`AccountId`],
//!
//!
//!
//!   dispatchables.
//!   be foreseen, consider loosely coupling pallets.
//!
//! with balances or assets pallet. More on this in [`frame_tokens`].
//!
//!
//!

#![allow(unused)]

use frame::prelude::*;

#[docify::export]
#[frame::pallet]
pub mod pallet_foo {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	impl<T: Config> Pallet<T> {
		fn do_stuff_with_author() {
			// needs block author here
		}
	}
}

#[docify::export]
#[frame::pallet]
pub mod pallet_author {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	impl<T: Config> Pallet<T> {
		pub fn author() -> T::AccountId {
			todo!("somehow has access to the block author and can return it here")
		}
	}
}

#[frame::pallet]
pub mod pallet_foo_tight {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(tight_config)]
	/// This pallet can only live in a runtime that has both `frame_system` and `pallet_author`.
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_author::Config {}

	#[docify::export(tight_usage)]
	impl<T: Config> Pallet<T> {
		// anywhere in `pallet-foo`, we can call into `pallet-author` directly, namely because
		// `T: pallet_author::Config`
		fn do_stuff_with_author() {
			let _ = pallet_author::Pallet::<T>::author();
		}
	}
}

#[docify::export]
/// Abstraction over "something that can provide the block author".
pub trait AuthorProvider<AccountId> {
	fn author() -> AccountId;
}

#[frame::pallet]
pub mod pallet_foo_loose {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(loose_config)]
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// This pallet relies on the existence of something that implements [`AuthorProvider`],
		/// which may or may not be `pallet-author`.
		type AuthorProvider: AuthorProvider<Self::AccountId>;
	}

	#[docify::export(loose_usage)]
	impl<T: Config> Pallet<T> {
		fn do_stuff_with_author() {
			let _ = T::AuthorProvider::author();
		}
	}
}

#[docify::export(pallet_author_provider)]
impl<T: pallet_author::Config> AuthorProvider<T::AccountId> for pallet_author::Pallet<T> {
	fn author() -> T::AccountId {
		pallet_author::Pallet::<T>::author()
	}
}

pub struct OtherAuthorProvider;

#[docify::export(other_author_provider)]
impl<AccountId> AuthorProvider<AccountId> for OtherAuthorProvider {
	fn author() -> AccountId {
		todo!("somehow get the block author here")
	}
}

#[docify::export(unit_author_provider)]
impl<AccountId> AuthorProvider<AccountId> for () {
	fn author() -> AccountId {
		todo!("somehow get the block author here")
	}
}

pub mod runtime {
	use super::*;
	use cumulus_pallet_aura_ext::pallet;
	use frame::{runtime::prelude::*, testing_prelude::*};

	construct_runtime!(
		pub struct Runtime {
			System: frame_system,
			PalletFoo: pallet_foo_loose,
			PalletAuthor: pallet_author,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = MockBlock<Self>;
	}

	impl pallet_author::Config for Runtime {}

	#[docify::export(runtime_author_provider)]
	impl pallet_foo_loose::Config for Runtime {
		type AuthorProvider = pallet_author::Pallet<Runtime>;
		// which is also equivalent to
		// type AuthorProvider = PalletAuthor;
	}
}

// Link References

// Link References








//!
//!
//!
//!
//!
//!
//!
//!
//! * *linked to other pallets*: But, adhering extensively to the above also hinders the ability to
//!
//!
//!
//!
//!
//!
//!
//! ```
//!
//! of `F`, which may be `B`, or another implementation of `F`.
//!
//!
//!    type F: F;
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_foo)]
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_author)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", tight_config)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", tight_usage)]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", AuthorProvider)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", loose_config)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", loose_usage)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_author_provider)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", runtime_author_provider)]
//!
//! module, you can find [`OtherAuthorProvider`], which is an alternative implementation of
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", other_author_provider)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", unit_author_provider)]
//!
//!
//! [`AccountId`],
//!
//!
//!
//!   dispatchables.
//!   be foreseen, consider loosely coupling pallets.
//!
//! with balances or assets pallet. More on this in [`frame_tokens`].
//!
//!
//!

#![allow(unused)]

use frame::prelude::*;

#[docify::export]
#[frame::pallet]
pub mod pallet_foo {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	impl<T: Config> Pallet<T> {
		fn do_stuff_with_author() {
			// needs block author here
		}
	}
}

#[docify::export]
#[frame::pallet]
pub mod pallet_author {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	impl<T: Config> Pallet<T> {
		pub fn author() -> T::AccountId {
			todo!("somehow has access to the block author and can return it here")
		}
	}
}

#[frame::pallet]
pub mod pallet_foo_tight {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(tight_config)]
	/// This pallet can only live in a runtime that has both `frame_system` and `pallet_author`.
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_author::Config {}

	#[docify::export(tight_usage)]
	impl<T: Config> Pallet<T> {
		// anywhere in `pallet-foo`, we can call into `pallet-author` directly, namely because
		// `T: pallet_author::Config`
		fn do_stuff_with_author() {
			let _ = pallet_author::Pallet::<T>::author();
		}
	}
}

#[docify::export]
/// Abstraction over "something that can provide the block author".
pub trait AuthorProvider<AccountId> {
	fn author() -> AccountId;
}

#[frame::pallet]
pub mod pallet_foo_loose {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(loose_config)]
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// This pallet relies on the existence of something that implements [`AuthorProvider`],
		/// which may or may not be `pallet-author`.
		type AuthorProvider: AuthorProvider<Self::AccountId>;
	}

	#[docify::export(loose_usage)]
	impl<T: Config> Pallet<T> {
		fn do_stuff_with_author() {
			let _ = T::AuthorProvider::author();
		}
	}
}

#[docify::export(pallet_author_provider)]
impl<T: pallet_author::Config> AuthorProvider<T::AccountId> for pallet_author::Pallet<T> {
	fn author() -> T::AccountId {
		pallet_author::Pallet::<T>::author()
	}
}

pub struct OtherAuthorProvider;

#[docify::export(other_author_provider)]
impl<AccountId> AuthorProvider<AccountId> for OtherAuthorProvider {
	fn author() -> AccountId {
		todo!("somehow get the block author here")
	}
}

#[docify::export(unit_author_provider)]
impl<AccountId> AuthorProvider<AccountId> for () {
	fn author() -> AccountId {
		todo!("somehow get the block author here")
	}
}

pub mod runtime {
	use super::*;
	use cumulus_pallet_aura_ext::pallet;
	use frame::{runtime::prelude::*, testing_prelude::*};

	construct_runtime!(
		pub struct Runtime {
			System: frame_system,
			PalletFoo: pallet_foo_loose,
			PalletAuthor: pallet_author,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = MockBlock<Self>;
	}

	impl pallet_author::Config for Runtime {}

	#[docify::export(runtime_author_provider)]
	impl pallet_foo_loose::Config for Runtime {
		type AuthorProvider = pallet_author::Pallet<Runtime>;
		// which is also equivalent to
		// type AuthorProvider = PalletAuthor;
	}
}

// Link References

// Link References







//!
//!
//!
//!
//!
//!
//!
//!
//! * *linked to other pallets*: But, adhering extensively to the above also hinders the ability to
//!
//!
//!
//!
//!
//!
//!
//! ```
//!
//! of `F`, which may be `B`, or another implementation of `F`.
//!
//!
//!    type F: F;
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_foo)]
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_author)]
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", tight_config)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", tight_usage)]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", AuthorProvider)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", loose_config)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", loose_usage)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_author_provider)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", runtime_author_provider)]
//!
//! module, you can find [`OtherAuthorProvider`], which is an alternative implementation of
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", other_author_provider)]
//!
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", unit_author_provider)]
//!
//!
//! [`AccountId`],
//!
//!
//!
//!   dispatchables.
//!   be foreseen, consider loosely coupling pallets.
//!
//! with balances or assets pallet. More on this in [`frame_tokens`].
//!
//!
//!

#![allow(unused)]

use frame::prelude::*;

#[docify::export]
#[frame::pallet]
pub mod pallet_foo {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	impl<T: Config> Pallet<T> {
		fn do_stuff_with_author() {
			// needs block author here
		}
	}
}

#[docify::export]
#[frame::pallet]
pub mod pallet_author {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	impl<T: Config> Pallet<T> {
		pub fn author() -> T::AccountId {
			todo!("somehow has access to the block author and can return it here")
		}
	}
}

#[frame::pallet]
pub mod pallet_foo_tight {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(tight_config)]
	/// This pallet can only live in a runtime that has both `frame_system` and `pallet_author`.
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_author::Config {}

	#[docify::export(tight_usage)]
	impl<T: Config> Pallet<T> {
		// anywhere in `pallet-foo`, we can call into `pallet-author` directly, namely because
		// `T: pallet_author::Config`
		fn do_stuff_with_author() {
			let _ = pallet_author::Pallet::<T>::author();
		}
	}
}

#[docify::export]
/// Abstraction over "something that can provide the block author".
pub trait AuthorProvider<AccountId> {
	fn author() -> AccountId;
}

#[frame::pallet]
pub mod pallet_foo_loose {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export(loose_config)]
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// This pallet relies on the existence of something that implements [`AuthorProvider`],
		/// which may or may not be `pallet-author`.
		type AuthorProvider: AuthorProvider<Self::AccountId>;
	}

	#[docify::export(loose_usage)]
	impl<T: Config> Pallet<T> {
		fn do_stuff_with_author() {
			let _ = T::AuthorProvider::author();
		}
	}
}

#[docify::export(pallet_author_provider)]
impl<T: pallet_author::Config> AuthorProvider<T::AccountId> for pallet_author::Pallet<T> {
	fn author() -> T::AccountId {
		pallet_author::Pallet::<T>::author()
	}
}

pub struct OtherAuthorProvider;

#[docify::export(other_author_provider)]
impl<AccountId> AuthorProvider<AccountId> for OtherAuthorProvider {
	fn author() -> AccountId {
		todo!("somehow get the block author here")
	}
}

#[docify::export(unit_author_provider)]
impl<AccountId> AuthorProvider<AccountId> for () {
	fn author() -> AccountId {
		todo!("somehow get the block author here")
	}
}

pub mod runtime {
	use super::*;
	use cumulus_pallet_aura_ext::pallet;
	use frame::{runtime::prelude::*, testing_prelude::*};

	construct_runtime!(
		pub struct Runtime {
			System: frame_system,
			PalletFoo: pallet_foo_loose,
			PalletAuthor: pallet_author,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = MockBlock<Self>;
	}

	impl pallet_author::Config for Runtime {}

	#[docify::export(runtime_author_provider)]
	impl pallet_foo_loose::Config for Runtime {
		type AuthorProvider = pallet_author::Pallet<Runtime>;
		// which is also equivalent to
		// type AuthorProvider = PalletAuthor;
	}
}

// Link References

// Link References











// [`pallet_balances`]: pallet_balances

// [`frame_tokens`]: frame_tokens
// [`pallet_balances`]: pallet_balances

// [`frame_runtime`]: frame_runtime
// [`frame_tokens`]: frame_tokens
// [`pallet_assets`]: pallet_assets
// [`pallet_balances`]: pallet_balances
