//! # FRAME Pallet Coupling
//!
//! This reference document explains how FRAME pallets can be combined to interact together.
//!
//! It is suggested to re-read [`crate::polkadot_sdk::frame_runtime`], notably the information
//! around [`frame::pallet_macros::config`]. Recall that:
//!
//! > Configuration trait of a pallet: It allows a pallet to receive types at a later
//! > point from the runtime that wishes to contain it. It allows the pallet to be parameterized
//! > over both types and values.
//!
//! ## Context, Background
//!
//! FRAME pallets, as per described in [`crate::polkadot_sdk::frame_runtime`] are:
//!
//! > A pallet is a unit of encapsulated logic. It has a clearly defined responsibility and can be
//! linked to other pallets.
//!
//! That is to say:
//!
//! * *encapsulated*: Ideally, a FRAME pallet contains encapsulated logic which has clear
//!   boundaries. It is generally a bad idea to build a single monolithic pallet that does multiple
//!   things, such as handling currencies, identities and staking all at the same time.
//! * *linked to other pallets*: But, adhering extensively to the above also hinders the ability to
//!   write useful applications. Pallets often need to work with each other, communicate and use
//!   each other's functionalities.
//!
//! The broad principle that allows pallets to be linked together is the same way through which a
//! pallet uses its `Config` trait to receive types and values from the runtime that contains it.
//!
//! There are generally two ways to achieve this:
//!
//! 1. Tight coupling pallets
//! 2. Loose coupling pallets
//!
//! To explain the difference between the two, consider two pallets, `A` and `B`. In both cases, `A`
//! wants to use some functionality exposed by `B`.
//!
//! When tightly coupling pallets, `A` can only exist in a runtime if `B` is also present in the
//! same runtime. That is, `A` is expressing that can only work if `B` is present.
//!
//! This translates to the following Rust code:
//!
//! ```
//! trait Pallet_B_Config {}
//! trait Pallet_A_Config: Pallet_B_Config {}
//! ```
//!
//! Contrary, when pallets are loosely coupled, `A` expresses that some functionality, expressed via
//! a trait `F`, needs to be fulfilled. This trait is then implemented by `B`, and the two pallets
//! are linked together at the runtime level. This means that `A` only relies on the implementation
//! of `F`, which may be `B`, or another implementation of `F`.
//!
//! This translates to the following Rust code:
//!
//! ```
//! trait F {}
//! trait Pallet_A_Config {
//!    type F: F;
//! }
//! // Pallet_B will implement and fulfill `F`.
//! ```
//!
//! ## Example
//!
//! Consider the following example, in which `pallet-foo` needs another pallet to provide the block
//! author to it, and `pallet-author` which has access to this information.
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_foo)]
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_author)]
//!
//! ### Tight Coupling Pallets
//!
//! To tightly couple `pallet-foo` and `pallet-author`, we use Rust's supertrait system. When a
//! pallet makes its own `trait Config` be bounded by another pallet's `trait Config`, it is
//! expressing two things:
//!
//! 1. that it can only exist in a runtime if the other pallet is also present.
//! 2. that it can use the other pallet's functionality.
//!
//! `pallet-foo`'s `Config` would then look like:
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", tight_config)]
//!
//! And `pallet-foo` can use the method exposed by `pallet_author::Pallet` directly:
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", tight_usage)]
//!
//!
//! ### Loosely  Coupling Pallets
//!
//! If `pallet-foo` wants to *not* rely on `pallet-author` directly, it can leverage its
//! `Config`'s associated types. First, we need a trait to express the functionality that
//! `pallet-foo` wants to obtain:
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", AuthorProvider)]
//!
//! > We sometimes refer to such traits that help two pallets interact as "glue traits".
//!
//! Next, `pallet-foo` states that it needs this trait to be provided to it, at the runtime level,
//! via an associated type:
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", loose_config)]
//!
//! Then, `pallet-foo` can use this trait to obtain the block author, without knowing where it comes
//! from:
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", loose_usage)]
//!
//! Then, if `pallet-author` implements this glue-trait:
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", pallet_author_provider)]
//!
//! And upon the creation of the runtime, the two pallets are linked together as such:
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", runtime_author_provider)]
//!
//! Crucially, when using loose coupling, we gain the flexibility of providing different
//! implementations of `AuthorProvider`, such that different users of a `pallet-foo` can use
//! different ones, without any code change being needed. For example, in the code snippets of this
//! module, you can fund [`OtherAuthorProvider`] which is an alternative implementation of
//! [`AuthorProvider`].
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", other_author_provider)]
//!
//! A common pattern in polkadot-sdk is to provide an implementation of such glu traits for the unit
//! type as a "default/test behavior".
#![doc = docify::embed!("./src/reference_docs/frame_pallet_coupling.rs", unit_author_provider)]
//!
//! ## Frame System
//!
//! With the above information in context, we can conclude that **`frame_system` is a special pallet
//! that is tightly coupled with every other pallet**. This is because it provides the fundamental
//! system functionality that every pallet needs, such as some types like
//! [`frame::prelude::frame_system::Config::AccountId`],
//! [`frame::prelude::frame_system::Config::Hash`], and some functionality such as block number,
//! etc.
//!
//! ## Recap
//!
//! To recap, consider the following rules of thumb:
//!
//! * In all cases, try and break down big pallets apart with clear boundaries of responsibility. In
//!   general, it is easier to argue about multiple pallet if they only communicate together via a
//!   known trait, rather than having access to all of each others public items, such as storage and
//!   dispatchables.
//! * If a group of pallets are meant to work together, and but are not foreseen to be generalized,
//!   or used by others, consider tightly coupling pallets, *if it simplifies the development*.
//! * If a pallet needs a functionality provided by another pallet, but multiple implementations can
//!   be foreseen, consider loosely coupling pallets.
//!
//! For example, all pallets in `polkadot-sdk` that needed to work with currencies could have been
//! tightly coupled with [`pallet_balances`]. But, `polkadot-sdk` also provides [`pallet_assets`]
//! (and more implementations by the community), therefore all pallets use traits to loosely couple
//! with balances or assets pallet. More on this in [`crate::reference_docs::frame_tokens`].
//!
//! ## Further References
//!
//! - <https://www.youtube.com/watch?v=0eNGZpNkJk4>
//! - <https://substrate.stackexchange.com/questions/922/pallet-loose-couplingtight-coupling-and-missing-traits>
//!
//! [`AuthorProvider`]: crate::reference_docs::frame_pallet_coupling::AuthorProvider
//! [`OtherAuthorProvider`]: crate::reference_docs::frame_pallet_coupling::OtherAuthorProvider

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
