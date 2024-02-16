//! # Trait-based Programming
//!
//! This document walks you over a peculiar way of using Rust's `trait` items. This pattern is
//! abundantly used within [`frame`] and is therefore paramount important for a smooth transition
//! into it.
//!
//! The rest of this document assumes familiarity with the
//! [Rust book's Advanced Traits](https://doc.rust-lang.org/book/ch19-03-advanced-traits.html)
//! section.
//! Moreover, we use the [`frame::traits::Get`].
//!
//! First, imagine we are writing a FRAME pallet. We represent this pallet with a `struct Pallet`,
//! and this pallet wants to implement the functionalities of that pallet, for example a simple
//! `transfer` function. For the sake of education, we are interested in having a `MinTransfer`
//! amount, expressed as a [`frame::traits::Get`], which will dictate what is the minimum amount
//! that can be transferred.
//!
//! We can foremost write this as simple as the following snippet:
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", basic)]
//!
//!
//! In this example, we use arbitrary choices for `AccountId`, `Balance` and the `MinTransfer` type.
//! This works great for **one team's purposes** but we have to remember that Substrate and FRAME
//! are written as generic frameworks, intended to be highly configurable.
//!
//! In a broad sense, there are two avenues in exposing configurability:
//!
//! 1. For *values* that need to be generic, for example `MinTransfer`, we attach them to the
//!    `Pallet` struct as fields:
//!
//! ```
//! struct Pallet {
//! 	min_transfer: u128,
//! }
//! ```
//!
//! 2. For *types* that need to be generic, we would have to use generic or associated types, such
//!    as:
//!
//! ```
//! struct Pallet<AccountId> {
//! 	min_transfer: u128,
//!     _marker: std::marker::PhantomData<AccountId>,
//! }
//! ```
//!
//! Substrate and FRAME, for various reasons (performance, correctness, type safety) has opted to
//! use *types* to declare both *values* and *types* as generic. This is the essence of why the
//! `Get` trait exists.
//!
//! This would bring us to the second iteration of the pallet, which would look like:
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", generic)]
//!
//! In this example, we managed to make all 3 of our types generic. Taking the example of the
//! `AccountId`, one should read the above as following:
//!
//! > The `Pallet` does not know what type `AccountId` concretely is, but it knows that it is
//! > something that adheres to being `From<[u8; 32]>`.
//!
//! This method would work, but it suffers from two downsides:
//!
//! 1. It is verbose, each `impl` block would have to reiterate all of the trait bounds.
//! 2. It cannot easily share/inherit generic types. Imagine multiple pallets wanting to be generic
//!    over a single `AccountId`. There is no easy way to express that in this model.
//!
//! Finally, this brings us to using traits and associated types on traits to express the above.
//! Trait associated types have the benefit of:
//!
//! 1. Being less verbose, as in effect they can *group multiple `type`s together*.
//! 2. Can inherit from one another by declaring
//! [supertraits](https://doc.rust-lang.org/rust-by-example/trait/supertraits.html).
//!
//! > Interestingly, one downside of associated types is that declaring defaults on them is not
//! > stable yet. In the meantime, we have built our own custom mechanics around declaring defaults
//! for associated types, see [`pallet_default_config_example`].
//!
//! The last iteration of our code would look like this:
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", trait_based)]
//!
//! Notice how instead of having multiple generics, everything is generic over a single `<T:
//! Config>`, and all types are fetched through `T`, for example `T::AccountId`, `T::MinTransfer`.
//!
//! Finally, imagine all pallets wanting to be generic over `AccountId`. This can be achieved by
//! having individual `trait Configs` declare a shared `trait SystemConfig` as their
//! [supertrait](https://doc.rust-lang.org/rust-by-example/trait/supertraits.html).
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", with_system)]
//! In FRAME, this shared supertrait is [`frame::prelude::frame_system`].
//!
//! Notice how this made no difference in the syntax of the rest of the code. `T::AccountId` is
//! still a valid type, since `T` implements `Config` and `Config` implies `SystemConfig`, which
//! has a `type AccountId`.
//!
//! Note, in some instances one would need to use what is known as the fully-qualified-syntax to
//! access a type to help the Rust compiler disambiguate.
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", fully_qualified)]
//!
//! This syntax can sometimes become more complicated when you are dealing with nested traits.
//! Consider the following example, in which we fetch the `type Balance` from another trait
//! `CurrencyTrait`.
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", fully_qualified_complicated)]
//!
//! Notice the final `type BalanceOf` and how it is defined. Using such aliases to shorten the
//! length of fully qualified syntax is a common pattern in FRAME.
//!
//! The above example is almost identical to the well-known (and somewhat notorious) `type
//! BalanceOf` that is often used in the context of [`frame::traits::fungible`].
#![doc = docify::embed!("../../substrate/frame/fast-unstake/src/types.rs", BalanceOf)]
//!
//! ## Additional Resources
//!
//! - <https://github.com/paritytech/substrate/issues/13836>
//! - [Substrate Seminar - Traits and Generic Types](https://www.youtube.com/watch?v=6cp10jVWNl4)
//! - <https://substrate.stackexchange.com/questions/2228/type-casting-to-trait-t-as-config>
#![allow(unused)]

use frame::traits::Get;

#[docify::export]
mod basic {
	struct Pallet;

	type AccountId = frame::deps::sp_runtime::AccountId32;
	type Balance = u128;
	type MinTransfer = frame::traits::ConstU128<10>;

	impl Pallet {
		fn transfer(_from: AccountId, _to: AccountId, _amount: Balance) {
			todo!()
		}
	}
}

#[docify::export]
mod generic {
	use super::*;

	struct Pallet<AccountId, Balance, MinTransfer> {
		_marker: std::marker::PhantomData<(AccountId, Balance, MinTransfer)>,
	}

	impl<AccountId, Balance, MinTransfer> Pallet<AccountId, Balance, MinTransfer>
	where
		Balance: frame::traits::AtLeast32BitUnsigned,
		MinTransfer: frame::traits::Get<Balance>,
		AccountId: From<[u8; 32]>,
	{
		fn transfer(_from: AccountId, _to: AccountId, amount: Balance) {
			assert!(amount >= MinTransfer::get());
			unimplemented!();
		}
	}
}

#[docify::export]
mod trait_based {
	use super::*;

	trait Config {
		type AccountId: From<[u8; 32]>;
		type Balance: frame::traits::AtLeast32BitUnsigned;
		type MinTransfer: frame::traits::Get<Self::Balance>;
	}

	struct Pallet<T: Config>(std::marker::PhantomData<T>);
	impl<T: Config> Pallet<T> {
		fn transfer(_from: T::AccountId, _to: T::AccountId, amount: T::Balance) {
			assert!(amount >= T::MinTransfer::get());
			unimplemented!();
		}
	}
}

#[docify::export]
mod with_system {
	use super::*;

	pub trait SystemConfig {
		type AccountId: From<[u8; 32]>;
	}

	pub trait Config: SystemConfig {
		type Balance: frame::traits::AtLeast32BitUnsigned;
		type MinTransfer: frame::traits::Get<Self::Balance>;
	}

	pub struct Pallet<T: Config>(std::marker::PhantomData<T>);
	impl<T: Config> Pallet<T> {
		fn transfer(_from: T::AccountId, _to: T::AccountId, amount: T::Balance) {
			assert!(amount >= T::MinTransfer::get());
			unimplemented!();
		}
	}
}

#[docify::export]
mod fully_qualified {
	use super::with_system::*;

	// Simple of using fully qualified syntax.
	type AccountIdOf<T> = <T as SystemConfig>::AccountId;
}

#[docify::export]
mod fully_qualified_complicated {
	use super::with_system::*;

	trait CurrencyTrait {
		type Balance: frame::traits::AtLeast32BitUnsigned;
		fn more_stuff() {}
	}

	trait Config: SystemConfig {
		type Currency: CurrencyTrait;
	}

	struct Pallet<T: Config>(std::marker::PhantomData<T>);
	impl<T: Config> Pallet<T> {
		fn transfer(
			_from: T::AccountId,
			_to: T::AccountId,
			_amount: <<T as Config>::Currency as CurrencyTrait>::Balance,
		) {
			unimplemented!();
		}
	}

	/// A common pattern in FRAME.
	type BalanceOf<T> = <<T as Config>::Currency as CurrencyTrait>::Balance;
}
