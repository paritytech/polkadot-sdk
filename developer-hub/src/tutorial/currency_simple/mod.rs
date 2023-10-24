//! # Currency Pallet
//!
//! By the end of this tutorial, you will write a small pallet (see
//! [`crate::polkadot_sdk::frame_runtime`]) that is capable of handling a simple crypto-currency.
//! This pallet will:
//!
//! 1. Allow a anyone to mint new tokens into accounts (which is obviously not a great idea for a
//!    real system).
//! 2. Allow any user that owns tokens to transfer them to others.
//! 3. Tracks of the total issuance of all tokens at all times.
//!
//! ## Topics Covered
//!
//! The following FRAME topics are covered in this tutorial. See the rust-doc of the associated
//! items to know more.
//!
//! - [Storage](frame::pallet_macros::storage`)
//! - [Call](frame::pallet_macros::call)
//! - [Event](frame::pallet_macros::event)
//! - [Error](frame::pallet_macros::error)
//! - Basics of testing a pallet.
//!
//! ## Writing Your First Pallet
//!
//! You should have studied the following reference documents as a prelude to this tutorial:
//!
//! - [`crate::reference_docs::blockchain_state_machines`]
//! - [`crate::reference_docs::trait_based_programming`]
//! - [`crate::polkadot_sdk::frame_runtime`]
//!
//! ### Shell Pallet
//!
//! TODO: a small link to what a shell pallet looks like, and a mention that this is our starting
//! point.
//!
//! ### Storage
//!
//! First, we will need to create two storage. One should be a mapping from account ids to a balance
//! type, and one value that is the total issuance. For the rest of this tutorial, we will opt for a
//! balance type of u128. to
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", Balance)]
//!
//! The definition of these two storage items, based on [`frame::pallet_macros::storage`], is as
//! follows:
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", TotalIssuance)]
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", Balances)]
//!
//! ### Dispatchables
//!
//! Next, we will define the dispatchable functions. As per [`frame::pallet_macros::call`], these
//! will be defined as normal `fn`s attached to `struct Pallet`.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", impl_pallet)]
//!
//! The logic of the functions is self-explanatory. Instead, we will focus on the FRAME-related
//! details:
//!
//! - Where do `T::AccountId` and `T::RuntimeOrigin` come from? These are both defined in
//!  [`frame::prelude::frame_system::Config`], therefore we can access them in `T`.
//! - What is `ensure_signed`, and what does it to with the aforementioned `T::RuntimeOrigin`? this
//!   is outside the scope of this tutorial, and you can learn more about it in the origin reference
//!   document (TODO). For now, you should only know the signature of the function: it takes a
//!   generic `T::RuntimeOrigin` and returns a `Result<T::AccountId, _>`. So by the end of this
//!   function call, we know that this dispatchable was signed by `who`.
#![doc = docify::embed!("../substrate/frame/system/src/lib.rs", ensure_signed)]
//!
//!
//! - Where does `mutate`, `get` and `insert` and other storage APIs come from? all of them are
//! explained in the corresponding `type`, for example, for `Balances::<T>::insert`, you can look
//! into [`frame::prelude::StorageMap::insert`].
//!
//! - The return type of all dispatchable functions is [`frame::prelude::DispatchResult`]:
#![doc = docify::embed!("../substrate/frame/support/src/dispatch.rs", DispatchResult)]
//!
//! Which is more or less a normal Rust `Result`, with a custom [`frame::prelude::DispatchError`] as
//! the `Err` variant. We won't cover this error in detail here, but importantly you should know
//! that there is an `impl From<&'static string> for DispatchError` provided (see here). Therefore,
//! we can use basic string literals as our error type and `.into()` them into `DispatchError`.
//!
//! - Why are all `get` and `mutate` functions return an `Option`? This is the default behavior of
//!   FRAME storage APIs. You can learn more about how to override this by looking into
//!   [`frame::pallet_macros::storage`], and
//!   [`frame::prelude::ValueQuery`]/[`frame::prelude::OptionQuery`]
//!
//! ### Improving Errors
//!
//! How we handle error in the above snippets is fairly rudimentary. Let's look at how this can be
//! improved. First, we can use [`frame::prelude::ensure`] to express the error slightly better.
//! This macro will call `.into()` under the hood for us.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", transfer_better)]
//!
//! Moreover, you will learn elsewhere (TODO link) that it is always recommended to use safe
//! arithmetic operations in your runtime. By using [`frame::traits::CheckedSub`], we can not only
//! take a step in that direction, but also improve the error handing and make it slightly more
//! ergonomic.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", transfer_better_checked)]
//!
//! This is more or less all the logic that there is this basic pallet!
//!
//! ### Your First (Test) Runtime
//!
//! Next, we create a "test runtime" in order to test our pallet. Recall from
//! [`crate::polkadot_sdk::frame_runtime`] that a runtime is a collection of pallets, expressed
//! through [`frame::runtime::prelude::construct_runtime`]. All runtimes also have to include
//! [`frame::frame_system`]. So we expect to see a runtime with two pallet, `frame_system` and the
//! one we just wrote.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", runtime)]
//!
//! > `derive_impl` is a FRAME feature that enables us to have defaults for associated types. You
//! > can learn more about it in TODO.
//!
//! Recall that within out pallet, (almost) all blocks of code are generic over `<T: Config>`. And,
//! because `trait Config: frame_system::Config`, we can get access to all items in `Config` (or
//! `frame_system::Config`) using `T::NameOfItem`. This is all within the boundaries of how Rust
//! traits and generics work. In unfamiliar with this pattern, read
//! [`crate::reference_docs::trait_based_programming`] before going further.
//!
//! Crucially, a typical FRAME runtime contains a `struct Runtime`. The main role of this `struct`
//! is to implement the `trait Config` of all pallets. That is, anywhere within your pallet code
//! where you see `<T: Config>` (read: *"some type `T` that implements `Config`"*), in the runtime,
//! it can be replaced with `Runtime`, because `Runtime` implements `Config` of all pallets, as we
//! see above.
//!
//! Another way to think about this is that within a pallet, a lot of types are "unknown" and, we
//! only know that they will be provided at some later point. For example, when you write
//! `T::AccountId` (which is short for `<T as frame_system::Config>`) in your pallet, you are in
//! fact saying "Some type `AccountId` that will be known later". That "later" is in fact when you
//! specify these types when you implement all `Config` traits for `Runtime`.
//!
//! As you see above, `frame_system::Config` is setting the `AccountId` to `u64`. Of course, a real
//! runtime will not use this type, and instead reside to a proper type like a 32-byte standard
//! public key. This is a HUGE benefit that FRAME developers can tap into: through the framework
//! being so generic, different types can always be customized to simple things when needed.
//!
//! > Imagine how hard it would have been if all tests had to use a real 32-byte account id, as
//! > opposed to just a u64 number 🙈.
//!
//! ### Your First Test
//!
//! The above is all you need to execute the dispatchables of your pallet. The last thing you need
//! to learn is that all of your pallet testing code should be wrapped in
//! [`frame::testing_prelude::TestState`]. This is a type that provides access to an in-memory state
//! to be used in our tests.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", first_test)]
//!
//! In the first test, we simply assert that there is no total issuance, and no balance associated
//! with account `1`. Then, we mint some balance into `1`, and re-check.
//!
//! As noted above, the `T::AccountId` is now `u64`. Moreover, `Runtime` is replacing `<T: Config>`.
//! This is why for example you see `Balances::<Runtime>::get(..)`. Finally, notice that the
//! dispatchables are simply functions that can be called on top of the `Pallet` struct.
//!
//! TODO: hard to explain exactly `RuntimeOrigin::signed(1)` at this point.
//!
//! Congratulations! You have written your first pallet and tested it! Next, we learn a few optional
//! steps to improve our pallet.
//!
//! ## Improving the Currency Pallet
//!
//! ### Better Test Setup
//!
//! Idiomatic FRAME pallets often use Builder pattern to define their initial state.
//!
//! > The Polkadot Blockchain Academy's Rust entrance exam has a
//! > [section](https://github.com/Polkadot-Blockchain-Academy/pba-qualifier-exam/blob/main/src/m_builder.rs)
//! > on this that you can use to learn the Builder Pattern.
//!
//! Let's see how we can implement a better test setup using this pattern. First, we define a
//! `struct StateBuilder`.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", StateBuilder)]
//!
//! This struct is meant to contain the same list of accounts and balances that we want to have at
//! the beginning of each block. We hardcoded this to `let accounts = vec![(1, 100), (2, 100)];` so
//! far. Then, if desired, we attach a default value for this struct.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", default_state_builder)]
//!
//! Like any other builder pattern, we attach functions to the type to mutate its internal
//! properties.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", impl_state_builder_add)]
//!
//!  Finally --the useful part-- we write our own custom `build_and_execute` function on
//! this type. This function will do multiple things:
//!
//! 1. It would consume `self` to produce our `TestState` based on the properties that we attached
//!    to `self`.
//! 2. It would execute any test function that we pass in as closure.
//! 3. A nifty trick, this allows our test setup to have some code that is executed both before and
//!    after each test. For example, in this test, we do some additional checking about the
//!    correctness of the `TotalIssuance`. We leave it up to you as an exercise to learn why the
//!    assertion should always hold, and how it is checked.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", impl_state_builder_build)]
//!
//! We can write tests that specifically check the initial state, and making sure our `StateBuilder`
//! is working exactly as intended.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", state_builder_works)]
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", state_builder_add_balance)]
//!
//! ### More Tests
//!
//! Now that we have a more ergonomic test setup, let's see how a well written test for transfer and
//! mint would look like.
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", transfer_works)]
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", mint_works)]
//!
//! It is always a good idea to build a mental model where you write *at least* one test for each
//! "success path" of a dispatchable, and one test for each "failure path", such as:
#![doc = docify::embed!("./src/tutorial/currency_simple/mod.rs", transfer_from_non_existent_fails)]
//!
//! We leave it up to you to write a test that triggers to `NotEnoughBalance` error.
//!
//! ## Part 5: Event and Error.
//!
//! ## What Next?
//!
//! The following topics where used in this tutorial, but not covered in depth. It is suggested to
//! study them subsequently:
//!
//! - [`crate::reference_docs::safe_defensive_programming`].
//! - [`crate::reference_docs::origin_account_abstraction`].
//! - The pallet we wrote in this tutorial was using `dev_mode`, learn more in
//!   [`frame::pallet_macros::config`].

mod parts;

#[doc(hidden)]
pub use pallet::*;

#[frame::pallet(dev_mode)]
pub mod pallet {
	use frame::prelude::*;

	#[docify::export]
	pub type Balance = u128;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[docify::export]
	/// Single storage item, of type `Balance`.
	#[pallet::storage]
	pub type TotalIssuance<T: Config> = StorageValue<_, Balance>;

	#[docify::export]
	/// A mapping from `T::AccountId` to `Balance`
	#[pallet::storage]
	pub type Balances<T: Config> = StorageMap<_, _, T::AccountId, Balance>;

	#[docify::export(impl_pallet)]
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// An unsafe mint that can be called by anyone. Not a great idea.
		pub fn mint_unsafe(
			origin: T::RuntimeOrigin,
			dest: T::AccountId,
			amount: Balance,
		) -> DispatchResult {
			// ensure that this is a signed account, but we don't really check `_anyone`.
			let _anyone = ensure_signed(origin)?;

			// update the balances map. Notice how all `<T: Config>` remains as `<T>`.
			Balances::<T>::mutate(dest, |b| *b = Some(b.unwrap_or(0) + amount));
			// update total issuance.
			TotalIssuance::<T>::mutate(|t| *t = Some(t.unwrap_or(0) + amount));

			Ok(())
		}

		/// Transfer `amount` from `origin` to `dest`.
		pub fn transfer(
			origin: T::RuntimeOrigin,
			dest: T::AccountId,
			amount: Balance,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			// ensure sender has enough balance, and if so, calculate what is left after `amount`.
			let sender_balance = Balances::<T>::get(&sender).ok_or("NonExistentAccount")?;
			if sender_balance < amount {
				return Err("NotEnoughBalance".into())
			}
			let reminder = sender_balance - amount;

			// update sender and dest balances.
			Balances::<T>::mutate(dest, |b| *b = Some(b.unwrap_or(0) + amount));
			Balances::<T>::insert(&sender, reminder);

			Ok(())
		}
	}

	#[allow(unused)]
	impl<T: Config> Pallet<T> {
		#[docify::export]
		pub fn transfer_better(
			origin: T::RuntimeOrigin,
			dest: T::AccountId,
			amount: Balance,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			let sender_balance = Balances::<T>::get(&sender).ok_or("NonExistentAccount")?;
			ensure!(sender_balance >= amount, "NotEnoughBalance");
			let reminder = sender_balance - amount;

			// .. snip
			Ok(())
		}

		#[docify::export]
		/// Transfer `amount` from `origin` to `dest`.
		pub fn transfer_better_checked(
			origin: T::RuntimeOrigin,
			dest: T::AccountId,
			amount: Balance,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			let sender_balance = Balances::<T>::get(&sender).ok_or("NonExistentAccount")?;
			let reminder = sender_balance.checked_sub(amount).ok_or("NotEnoughBalance")?;

			// .. snip
			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::tutorial::currency_simple::{
		pallet::{self as pallet_currency, *},
		TotalIssuance,
	};
	use frame::testing_prelude::*;

	#[docify::export]
	mod runtime {
		use super::*;
		// we need to reference our `mod pallet` as an identifier to pass to `construct_runtime`.
		use crate::tutorial::currency_simple::pallet as pallet_currency;

		construct_runtime!(
			pub struct Runtime {
				// ---^^^^^^ This is where `struct Runtime` is defined.
				System: frame_system,
				Currency: pallet_currency,
			}
		);

		#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
		impl frame_system::Config for Runtime {
			type Block = MockBlock<Runtime>;
			// within pallet we just said `<T as frame_system::Config>::AccountId`, now we finally
			// specified it.
			type AccountId = u64;
		}

		// our simple pallet has nothing to be configured.
		impl pallet_currency::Config for Runtime {}
	}

	use runtime::*;

	#[docify::export]
	fn new_test_state_basic() -> TestState {
		let mut state = TestState::new_empty();
		let accounts = vec![(1, 100), (2, 100)];
		state.execute_with(|| {
			for (who, amount) in &accounts {
				Balances::<Runtime>::insert(who, amount);
				TotalIssuance::<Runtime>::mutate(|b| *b = Some(b.unwrap_or(0) + amount));
			}
		});

		state
	}

	#[docify::export]
	struct StateBuilder {
		balances: Vec<(<Runtime as frame_system::Config>::AccountId, Balance)>,
	}

	#[docify::export(default_state_builder)]
	impl Default for StateBuilder {
		fn default() -> Self {
			Self { balances: vec![(1, 100), (2, 100)] }
		}
	}

	#[docify::export(impl_state_builder_add)]
	impl StateBuilder {
		fn add_balance(
			mut self,
			who: <Runtime as frame_system::Config>::AccountId,
			amount: Balance,
		) -> Self {
			self.balances.push((who, amount));
			self
		}
	}

	#[docify::export(impl_state_builder_build)]
	impl StateBuilder {
		fn build_and_execute(self, test: impl FnOnce() -> ()) {
			let mut ext = TestState::new_empty();
			ext.execute_with(|| {
				for (who, amount) in &self.balances {
					Balances::<Runtime>::insert(who, amount);
					TotalIssuance::<Runtime>::mutate(|b| *b = Some(b.unwrap_or(0) + amount));
				}
			});

			ext.execute_with(test);

			// assertions that must always hold
			ext.execute_with(|| {
				assert_eq!(
					Balances::<Runtime>::iter().map(|(_, x)| x).sum::<u128>(),
					TotalIssuance::<Runtime>::get().unwrap_or_default()
				);
			})
		}
	}

	#[docify::export]
	#[test]
	fn first_test() {
		TestState::new_empty().execute_with(|| {
			// We expect account 1 to have no funds.
			assert_eq!(Balances::<Runtime>::get(&1), None);
			assert_eq!(TotalIssuance::<Runtime>::get(), None);

			// mint some funds into 1
			assert_ok!(Pallet::<Runtime>::mint_unsafe(RuntimeOrigin::signed(1), 1, 100));

			// re-check the above
			assert_eq!(Balances::<Runtime>::get(&1), Some(100));
			assert_eq!(TotalIssuance::<Runtime>::get(), Some(100));
		})
	}

	#[docify::export]
	#[test]
	fn state_builder_works() {
		StateBuilder::default().build_and_execute(|| {
			assert_eq!(Balances::<Runtime>::get(&1), Some(100));
			assert_eq!(Balances::<Runtime>::get(&2), Some(100));
			assert_eq!(Balances::<Runtime>::get(&3), None);
			assert_eq!(TotalIssuance::<Runtime>::get(), Some(200));
		});
	}

	#[docify::export]
	#[test]
	fn state_builder_add_balance() {
		StateBuilder::default().add_balance(3, 42).build_and_execute(|| {
			assert_eq!(Balances::<Runtime>::get(&3), Some(42));
			assert_eq!(TotalIssuance::<Runtime>::get(), Some(242));
		})
	}

	#[test]
	#[should_panic]
	fn state_builder_duplicate_genesis_fails() {
		StateBuilder::default()
			.add_balance(3, 42)
			.add_balance(3, 43)
			.build_and_execute(|| {
				assert_eq!(Balances::<Runtime>::get(&3), None);
				assert_eq!(TotalIssuance::<Runtime>::get(), Some(242));
			})
	}

	#[docify::export]
	#[test]
	fn mint_works() {
		StateBuilder::default().build_and_execute(|| {
			// given the initial state, when:
			assert_ok!(Pallet::<Runtime>::mint_unsafe(RuntimeOrigin::signed(1), 2, 100));

			// then:
			assert_eq!(Balances::<Runtime>::get(&2), Some(200));
			assert_eq!(TotalIssuance::<Runtime>::get(), Some(300));

			// given:
			assert_ok!(Pallet::<Runtime>::mint_unsafe(RuntimeOrigin::signed(1), 3, 100));

			// then:
			assert_eq!(Balances::<Runtime>::get(&3), Some(100));
			assert_eq!(TotalIssuance::<Runtime>::get(), Some(400));
		});
	}

	#[docify::export]
	#[test]
	fn transfer_works() {
		StateBuilder::default().build_and_execute(|| {
			// given the the initial state, when:
			assert_ok!(Pallet::<Runtime>::transfer(RuntimeOrigin::signed(1), 2, 50));

			// then:
			assert_eq!(Balances::<Runtime>::get(&1), Some(50));
			assert_eq!(Balances::<Runtime>::get(&2), Some(150));
			assert_eq!(TotalIssuance::<Runtime>::get(), Some(200));

			// when:
			assert_ok!(Pallet::<Runtime>::transfer(RuntimeOrigin::signed(2), 1, 50));

			// then:
			assert_eq!(Balances::<Runtime>::get(&1), Some(100));
			assert_eq!(Balances::<Runtime>::get(&2), Some(100));
			assert_eq!(TotalIssuance::<Runtime>::get(), Some(200));
		});
	}

	#[docify::export]
	#[test]
	fn transfer_from_non_existent_fails() {
		StateBuilder::default().build_and_execute(|| {
			// given the the initial state, when:
			assert_err!(
				Pallet::<Runtime>::transfer(RuntimeOrigin::signed(3), 1, 10),
				"NonExistentAccount"
			);

			// then nothing has changed.
			assert_eq!(Balances::<Runtime>::get(&1), Some(100));
			assert_eq!(Balances::<Runtime>::get(&2), Some(100));
			assert_eq!(Balances::<Runtime>::get(&3), None);
			assert_eq!(TotalIssuance::<Runtime>::get(), Some(200));
		});
	}
}
