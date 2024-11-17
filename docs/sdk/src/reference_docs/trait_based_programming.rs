//! # Trait-based Programming
//!
//!
//! Moreover, we use the [`Get`].
//!
//! amount, expressed as a [`Get`], which will dictate what is the minimum amount
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", basic)]
//!
//!
//!
//!
//!
//! }
//!
//!
//!     _marker: std::marker::PhantomData<AccountId>,
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", generic)]
//!
//!
//!
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", trait_based)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", with_system)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", fully_qualified)]
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", fully_qualified_complicated)]
//!
//!
#![doc = docify::embed!("../../substrate/frame/fast-unstake/src/types.rs", BalanceOf)]
//!
//!
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

	// Example of using fully qualified syntax.
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

//!
//!
//! Moreover, we use the [`Get`].
//!
//! amount, expressed as a [`Get`], which will dictate what is the minimum amount
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", basic)]
//!
//!
//!
//!
//!
//! }
//!
//!
//!     _marker: std::marker::PhantomData<AccountId>,
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", generic)]
//!
//!
//!
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", trait_based)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", with_system)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", fully_qualified)]
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", fully_qualified_complicated)]
//!
//!
#![doc = docify::embed!("../../substrate/frame/fast-unstake/src/types.rs", BalanceOf)]
//!
//!
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

	// Example of using fully qualified syntax.
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


//!
//!
//! Moreover, we use the [`Get`].
//!
//! amount, expressed as a [`Get`], which will dictate what is the minimum amount
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", basic)]
//!
//!
//!
//!
//!
//! }
//!
//!
//!     _marker: std::marker::PhantomData<AccountId>,
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", generic)]
//!
//!
//!
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", trait_based)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", with_system)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", fully_qualified)]
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", fully_qualified_complicated)]
//!
//!
#![doc = docify::embed!("../../substrate/frame/fast-unstake/src/types.rs", BalanceOf)]
//!
//!
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

	// Example of using fully qualified syntax.
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

//!
//!
//! Moreover, we use the [`Get`].
//!
//! amount, expressed as a [`Get`], which will dictate what is the minimum amount
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", basic)]
//!
//!
//!
//!
//!
//! }
//!
//!
//!     _marker: std::marker::PhantomData<AccountId>,
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", generic)]
//!
//!
//!
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", trait_based)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", with_system)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", fully_qualified)]
//!
#![doc = docify::embed!("./src/reference_docs/trait_based_programming.rs", fully_qualified_complicated)]
//!
//!
#![doc = docify::embed!("../../substrate/frame/fast-unstake/src/types.rs", BalanceOf)]
//!
//!
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

	// Example of using fully qualified syntax.
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




// [`pallet_default_config_example`]: pallet_default_config_example

// [`Rust book's Advanced Traits`]: https://doc.rust-lang.org/book/ch19-03-advanced-traits.html
// [`Substrate Seminar - Traits and Generic Types`]: https://www.youtube.com/watch?v=6cp10jVWNl4
// [`frame_system`]: frame_system
// [`pallet_default_config_example`]: pallet_default_config_example
