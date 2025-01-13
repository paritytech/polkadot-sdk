//! # Currency Pallet
//!
//! By the end of this guide, you will have written a small FRAME pallet (see
//! [`crate::polkadot_sdk::frame_runtime`]) that is capable of handling a simple crypto-currency.
//! This pallet will:
//!
//! 1. Allow anyone to mint new tokens into accounts (which is obviously not a great idea for a real
//!    system).
//! 2. Allow any user that owns tokens to transfer them to others.
//! 3. Track the total issuance of all tokens at all times.
//!
//! > This guide will build a currency pallet from scratch using only the lowest primitives of
//! > FRAME, and is mainly intended for education, not *applicability*. For example, almost all
//! > FRAME-based runtimes use various techniques to re-use a currency pallet instead of writing
//! > one. Further advanced FRAME related topics are discussed in [`crate::reference_docs`].
//!
//! ## Writing Your First Pallet
//!
//! To get started, clone one of the templates mentioned in [`crate::polkadot_sdk::templates`]. We
//! recommend using the `polkadot-sdk-minimal-template`. You might need to change small parts of
//! this guide, namely the crate/package names, based on which template you use.
//!
//! > Be aware that you can read the entire source code backing this tutorial by clicking on the
//! > `source` button at the top right of the page.
//!
//! You should have studied the following modules as a prelude to this guide:
//!
//! - [`crate::reference_docs::blockchain_state_machines`]
//! - [`crate::reference_docs::trait_based_programming`]
//! - [`crate::polkadot_sdk::frame_runtime`]
//!
//! ## Topics Covered
//!
//! The following FRAME topics are covered in this guide:
//!
//! - [`pallet::storage`]
//! - [`pallet::call`]
//! - [`pallet::event`]
//! - [`pallet::error`]
//! - Basics of testing a pallet
//! - [Constructing a runtime](frame::runtime::prelude::construct_runtime)
//!
//! ### Shell Pallet
//!
//! Consider the following as a "shell pallet". We continue building the rest of this pallet based
//! on this template.
//!
//! [`pallet::config`] and [`pallet::pallet`] are both mandatory parts of any
//! pallet. Refer to the documentation of each to get an overview of what they do.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", shell_pallet)]
//!
//! All of the code that follows in this guide should live inside of the `mod pallet`.
//!
//! ### Storage
//!
//! First, we will need to create two onchain storage declarations.
//!
//! One should be a mapping from account-ids to a balance type, and one value that is the total
//! issuance.
//!
//! > For the rest of this guide, we will opt for a balance type of `u128`. For the sake of
//! > simplicity, we are hardcoding this type. In a real pallet is best practice to define it as a
//! > generic bounded type in the `Config` trait, and then specify it in the implementation.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", Balance)]
//!
//! The definition of these two storage items, based on [`pallet::storage`] details, is as follows:
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", TotalIssuance)]
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", Balances)]
//!
//! ### Dispatchables
//!
//! Next, we will define the dispatchable functions. As per [`pallet::call`], these will be defined
//! as normal `fn`s attached to `struct Pallet`.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", impl_pallet)]
//!
//! The logic of these functions is self-explanatory. Instead, we will focus on the FRAME-related
//! details:
//!
//! - Where do `T::AccountId` and `T::RuntimeOrigin` come from? These are both defined in
//!  [`frame::prelude::frame_system::Config`], therefore we can access them in `T`.
//! - What is `ensure_signed`, and what does it do with the aforementioned `T::RuntimeOrigin`? This
//!   is outside the scope of this guide, and you can learn more about it in the origin reference
//!   document ([`crate::reference_docs::frame_origin`]). For now, you should only know the
//!   signature of the function: it takes a generic `T::RuntimeOrigin` and returns a
//!   `Result<T::AccountId, _>`. So by the end of this function call, we know that this dispatchable
//!   was signed by `sender`.
#![doc = docify::embed!("../../substrate/frame/system/src/lib.rs", ensure_signed)]
//!
//! - Where does `mutate`, `get` and `insert` and other storage APIs come from? All of them are
//! explained in the corresponding `type`, for example, for `Balances::<T>::insert`, you can look
//! into [`frame::prelude::StorageMap::insert`].
//!
//! - The return type of all dispatchable functions is [`frame::prelude::DispatchResult`]:
#![doc = docify::embed!("../../substrate/frame/support/src/dispatch.rs", DispatchResult)]
//!
//! Which is more or less a normal Rust `Result`, with a custom [`frame::prelude::DispatchError`] as
//! the `Err` variant. We won't cover this error in detail here, but importantly you should know
//! that there is an `impl From<&'static string> for DispatchError` provided (see
//! [here](`frame::prelude::DispatchError#impl-From<%26str>-for-DispatchError`)). Therefore,
//! we can use basic string literals as our error type and `.into()` them into `DispatchError`.
//!
//! - Why are all `get` and `mutate` functions returning an `Option`? This is the default behavior
//!   of FRAME storage APIs. You can learn more about how to override this by looking into
//!   [`pallet::storage`], and [`frame::prelude::ValueQuery`]/[`frame::prelude::OptionQuery`]
//!
//! ### Improving Errors
//!
//! How we handle error in the above snippets is fairly rudimentary. Let's look at how this can be
//! improved. First, we can use [`frame::prelude::ensure`] to express the error slightly better.
//! This macro will call `.into()` under the hood.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", transfer_better)]
//!
//! Moreover, you will learn in the [Defensive Programming
//! section](crate::reference_docs::defensive_programming) that it is always recommended to use
//! safe arithmetic operations in your runtime. By using [`frame::traits::CheckedSub`], we can not
//! only take a step in that direction, but also improve the error handing and make it slightly more
//! ergonomic.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", transfer_better_checked)]
//!
//! This is more or less all the logic that there is in this basic currency pallet!
//!
//! ### Your First (Test) Runtime
//!
//! The typical testing code of a pallet lives in a module that imports some preludes useful for
//! testing, similar to:
//!
//! ```
//! pub mod pallet {
//! 	// snip -- actually pallet code.
//! }
//!
//! #[cfg(test)]
//! mod tests {
//! 	// bring in the testing prelude of frame
//! 	use frame::testing_prelude::*;
//! 	// bring in all pallet items
//! 	use super::pallet::*;
//!
//! 	// snip -- rest of the testing code.
//! }
//! ```
//!
//! Next, we create a "test runtime" in order to test our pallet. Recall from
//! [`crate::polkadot_sdk::frame_runtime`] that a runtime is a collection of pallets, expressed
//! through [`frame::runtime::prelude::construct_runtime`]. All runtimes also have to include
//! [`frame::prelude::frame_system`]. So we expect to see a runtime with two pallet, `frame_system`
//! and the one we just wrote.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", runtime)]
//!
//! > [`frame::pallet_macros::derive_impl`] is a FRAME feature that enables developers to have
//! > defaults for associated types.
//!
//! Recall that within our pallet, (almost) all blocks of code are generic over `<T: Config>`. And,
//! because `trait Config: frame_system::Config`, we can get access to all items in `Config` (or
//! `frame_system::Config`) using `T::NameOfItem`. This is all within the boundaries of how
//! Rust traits and generics work. If unfamiliar with this pattern, read
//! [`crate::reference_docs::trait_based_programming`] before going further.
//!
//! Crucially, a typical FRAME runtime contains a `struct Runtime`. The main role of this `struct`
//! is to implement the `trait Config` of all pallets. That is, anywhere within your pallet code
//! where you see `<T: Config>` (read: *"some type `T` that implements `Config`"*), in the runtime,
//! it can be replaced with `<Runtime>`, because `Runtime` implements `Config` of all pallets, as we
//! see above.
//!
//! Another way to think about this is that within a pallet, a lot of types are "unknown" and, we
//! only know that they will be provided at some later point. For example, when you write
//! `T::AccountId` (which is short for `<T as frame_system::Config>::AccountId`) in your pallet,
//! you are in fact saying "*Some type `AccountId` that will be known later*". That "later" is in
//! fact when you specify these types when you implement all `Config` traits for `Runtime`.
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
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", first_test)]
//!
//! In the first test, we simply assert that there is no total issuance, and no balance associated
//! with Alice's account. Then, we mint some balance into Alice's, and re-check.
//!
//! As noted above, the `T::AccountId` is now `u64`. Moreover, `Runtime` is replacing `<T: Config>`.
//! This is why for example you see `Balances::<Runtime>::get(..)`. Finally, notice that the
//! dispatchables are simply functions that can be called on top of the `Pallet` struct.
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
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", StateBuilder)]
//!
//! This struct is meant to contain the same list of accounts and balances that we want to have at
//! the beginning of each block. We hardcoded this to `let accounts = vec![(ALICE, 100), (2, 100)];`
//! so far. Then, if desired, we attach a default value for this struct.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", default_state_builder)]
//!
//! Like any other builder pattern, we attach functions to the type to mutate its internal
//! properties.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", impl_state_builder_add)]
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
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", impl_state_builder_build)]
//!
//! We can write tests that specifically check the initial state, and making sure our `StateBuilder`
//! is working exactly as intended.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", state_builder_works)]
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", state_builder_add_balance)]
//!
//! ### More Tests
//!
//! Now that we have a more ergonomic test setup, let's see how a well written test for transfer and
//! mint would look like.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", transfer_works)]
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", mint_works)]
//!
//! It is always a good idea to build a mental model where you write *at least* one test for each
//! "success path" of a dispatchable, and one test for each "failure path", such as:
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", transfer_from_non_existent_fails)]
//!
//! We leave it up to you to write a test that triggers the `InsufficientBalance` error.
//!
//! ### Event and Error
//!
//! Our pallet is mainly missing two parts that are common in most FRAME pallets: Events, and
//! Errors. First, let's understand what each is.
//!
//! - **Error**: The static string-based error scheme we used so far is good for readability, but it
//!   has a few drawbacks. The biggest problem with strings are that they are not type safe, e.g. a
//!   match statement cannot be exhaustive. These string literals will bloat the final wasm blob,
//!   and are relatively heavy to transmit and encode/decode. Moreover, it is easy to mistype them
//!   by one character. FRAME errors are exactly a solution to maintain readability, whilst fixing
//!   the drawbacks mentioned. In short, we use an enum to represent different variants of our
//!   error. These variants are then mapped in an efficient way (using only `u8` indices) to
//!   [`sp_runtime::DispatchError::Module`]. Read more about this in [`pallet::error`].
//!
//! - **Event**: Events are akin to the return type of dispatchables. They are mostly data blobs
//!   emitted by the runtime to let outside world know what is happening inside the pallet. Since
//!   otherwise, the outside world does not have an easy access to the state changes. They should
//!   represent what happened at the end of a dispatch operation. Therefore, the convention is to
//!   use passive tense for event names (eg. `SomethingHappened`). This allows other sub-systems or
//!   external parties (eg. a light-node, a DApp) to listen to particular events happening, without
//!   needing to re-execute the whole state transition function.
//!
//! With the explanation out of the way, let's see how these components can be added. Both follow a
//! fairly familiar syntax: normal Rust enums, with extra [`pallet::event`] and [`pallet::error`]
//! attributes attached.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", Event)]
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", Error)]
//!
//! One slightly custom part of this is the [`pallet::generate_deposit`] part. Without going into
//! too much detail, in order for a pallet to emit events to the rest of the system, it needs to do
//! two things:
//!
//! 1. Declare a type in its `Config` that refers to the overarching event type of the runtime. In
//! short, by doing this, the pallet is expressing an important bound: `type RuntimeEvent:
//! From<Event<Self>>`. Read: a `RuntimeEvent` exists, and it can be created from the local `enum
//! Event` of this pallet. This enables the pallet to convert its `Event` into `RuntimeEvent`, and
//! store it where needed.
//!
//! 2. But, doing this conversion and storing is too much to expect each pallet to define. FRAME
//! provides a default way of storing events, and this is what [`pallet::generate_deposit`] is
//! doing.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", config_v2)]
//!
//! > These `Runtime*` types are better explained in
//! > [`crate::reference_docs::frame_runtime_types`].
//!
//! Then, we can rewrite the `transfer` dispatchable as such:
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", transfer_v2)]
//!
//! Then, notice how now we would need to provide this `type RuntimeEvent` in our test runtime
//! setup.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", runtime_v2)]
//!
//! In this snippet, the actual `RuntimeEvent` type (right hand side of `type RuntimeEvent =
//! RuntimeEvent`) is generated by
//! [`construct_runtime`](frame::runtime::prelude::construct_runtime). An interesting way to inspect
//! this type is to see its definition in rust-docs:
//! [`crate::guides::your_first_pallet::pallet_v2::tests::runtime_v2::RuntimeEvent`].
//!
//!
//! ## What Next?
//!
//! The following topics where used in this guide, but not covered in depth. It is suggested to
//! study them subsequently:
//!
//! - [`crate::reference_docs::defensive_programming`].
//! - [`crate::reference_docs::frame_origin`].
//! - [`crate::reference_docs::frame_runtime_types`].
//! - The pallet we wrote in this guide was using `dev_mode`, learn more in [`pallet::config`].
//! - Learn more about the individual pallet items/macros, such as event and errors and call, in
//!   [`frame::pallet_macros`].
//!
//! [`pallet::storage`]: frame_support::pallet_macros::storage
//! [`pallet::call`]: frame_support::pallet_macros::call
//! [`pallet::event`]: frame_support::pallet_macros::event
//! [`pallet::error`]: frame_support::pallet_macros::error
//! [`pallet::pallet`]: frame_support::pallet
//! [`pallet::config`]: frame_support::pallet_macros::config
//! [`pallet::generate_deposit`]: frame_support::pallet_macros::generate_deposit

#[docify::export]
#[frame::pallet(dev_mode)]
pub mod shell_pallet {
	use frame::prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);
}

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
				return Err("InsufficientBalance".into())
			}
			let remainder = sender_balance - amount;

			// update sender and dest balances.
			Balances::<T>::mutate(dest, |b| *b = Some(b.unwrap_or(0) + amount));
			Balances::<T>::insert(&sender, remainder);

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
			ensure!(sender_balance >= amount, "InsufficientBalance");
			let remainder = sender_balance - amount;

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
			let remainder = sender_balance.checked_sub(amount).ok_or("InsufficientBalance")?;

			// .. snip
			Ok(())
		}
	}

	#[cfg(any(test, doc))]
	pub(crate) mod tests {
		use crate::guides::your_first_pallet::pallet::*;

		#[docify::export(testing_prelude)]
		use frame::testing_prelude::*;

		pub(crate) const ALICE: u64 = 1;
		pub(crate) const BOB: u64 = 2;
		pub(crate) const CHARLIE: u64 = 3;

		#[docify::export]
		// This runtime is only used for testing, so it should be somewhere like `#[cfg(test)] mod
		// tests { .. }`
		mod runtime {
			use super::*;
			// we need to reference our `mod pallet` as an identifier to pass to
			// `construct_runtime`.
			// YOU HAVE TO CHANGE THIS LINE BASED ON YOUR TEMPLATE
			use crate::guides::your_first_pallet::pallet as pallet_currency;

			construct_runtime!(
				pub enum Runtime {
					// ---^^^^^^ This is where `enum Runtime` is defined.
					System: frame_system,
					Currency: pallet_currency,
				}
			);

			#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
			impl frame_system::Config for Runtime {
				type Block = MockBlock<Runtime>;
				// within pallet we just said `<T as frame_system::Config>::AccountId`, now we
				// finally specified it.
				type AccountId = u64;
			}

			// our simple pallet has nothing to be configured.
			impl pallet_currency::Config for Runtime {}
		}

		pub(crate) use runtime::*;

		#[allow(unused)]
		#[docify::export]
		fn new_test_state_basic() -> TestState {
			let mut state = TestState::new_empty();
			let accounts = vec![(ALICE, 100), (BOB, 100)];
			state.execute_with(|| {
				for (who, amount) in &accounts {
					Balances::<Runtime>::insert(who, amount);
					TotalIssuance::<Runtime>::mutate(|b| *b = Some(b.unwrap_or(0) + amount));
				}
			});

			state
		}

		#[docify::export]
		pub(crate) struct StateBuilder {
			balances: Vec<(<Runtime as frame_system::Config>::AccountId, Balance)>,
		}

		#[docify::export(default_state_builder)]
		impl Default for StateBuilder {
			fn default() -> Self {
				Self { balances: vec![(ALICE, 100), (BOB, 100)] }
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
			pub(crate) fn build_and_execute(self, test: impl FnOnce() -> ()) {
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
				// We expect Alice's account to have no funds.
				assert_eq!(Balances::<Runtime>::get(&ALICE), None);
				assert_eq!(TotalIssuance::<Runtime>::get(), None);

				// mint some funds into Alice's account.
				assert_ok!(Pallet::<Runtime>::mint_unsafe(
					RuntimeOrigin::signed(ALICE),
					ALICE,
					100
				));

				// re-check the above
				assert_eq!(Balances::<Runtime>::get(&ALICE), Some(100));
				assert_eq!(TotalIssuance::<Runtime>::get(), Some(100));
			})
		}

		#[docify::export]
		#[test]
		fn state_builder_works() {
			StateBuilder::default().build_and_execute(|| {
				assert_eq!(Balances::<Runtime>::get(&ALICE), Some(100));
				assert_eq!(Balances::<Runtime>::get(&BOB), Some(100));
				assert_eq!(Balances::<Runtime>::get(&CHARLIE), None);
				assert_eq!(TotalIssuance::<Runtime>::get(), Some(200));
			});
		}

		#[docify::export]
		#[test]
		fn state_builder_add_balance() {
			StateBuilder::default().add_balance(CHARLIE, 42).build_and_execute(|| {
				assert_eq!(Balances::<Runtime>::get(&CHARLIE), Some(42));
				assert_eq!(TotalIssuance::<Runtime>::get(), Some(242));
			})
		}

		#[test]
		#[should_panic]
		fn state_builder_duplicate_genesis_fails() {
			StateBuilder::default()
				.add_balance(CHARLIE, 42)
				.add_balance(CHARLIE, 43)
				.build_and_execute(|| {
					assert_eq!(Balances::<Runtime>::get(&CHARLIE), None);
					assert_eq!(TotalIssuance::<Runtime>::get(), Some(242));
				})
		}

		#[docify::export]
		#[test]
		fn mint_works() {
			StateBuilder::default().build_and_execute(|| {
				// given the initial state, when:
				assert_ok!(Pallet::<Runtime>::mint_unsafe(RuntimeOrigin::signed(ALICE), BOB, 100));

				// then:
				assert_eq!(Balances::<Runtime>::get(&BOB), Some(200));
				assert_eq!(TotalIssuance::<Runtime>::get(), Some(300));

				// given:
				assert_ok!(Pallet::<Runtime>::mint_unsafe(
					RuntimeOrigin::signed(ALICE),
					CHARLIE,
					100
				));

				// then:
				assert_eq!(Balances::<Runtime>::get(&CHARLIE), Some(100));
				assert_eq!(TotalIssuance::<Runtime>::get(), Some(400));
			});
		}

		#[docify::export]
		#[test]
		fn transfer_works() {
			StateBuilder::default().build_and_execute(|| {
				// given the initial state, when:
				assert_ok!(Pallet::<Runtime>::transfer(RuntimeOrigin::signed(ALICE), BOB, 50));

				// then:
				assert_eq!(Balances::<Runtime>::get(&ALICE), Some(50));
				assert_eq!(Balances::<Runtime>::get(&BOB), Some(150));
				assert_eq!(TotalIssuance::<Runtime>::get(), Some(200));

				// when:
				assert_ok!(Pallet::<Runtime>::transfer(RuntimeOrigin::signed(BOB), ALICE, 50));

				// then:
				assert_eq!(Balances::<Runtime>::get(&ALICE), Some(100));
				assert_eq!(Balances::<Runtime>::get(&BOB), Some(100));
				assert_eq!(TotalIssuance::<Runtime>::get(), Some(200));
			});
		}

		#[docify::export]
		#[test]
		fn transfer_from_non_existent_fails() {
			StateBuilder::default().build_and_execute(|| {
				// given the initial state, when:
				assert_err!(
					Pallet::<Runtime>::transfer(RuntimeOrigin::signed(CHARLIE), ALICE, 10),
					"NonExistentAccount"
				);

				// then nothing has changed.
				assert_eq!(Balances::<Runtime>::get(&ALICE), Some(100));
				assert_eq!(Balances::<Runtime>::get(&BOB), Some(100));
				assert_eq!(Balances::<Runtime>::get(&CHARLIE), None);
				assert_eq!(TotalIssuance::<Runtime>::get(), Some(200));
			});
		}
	}
}

#[frame::pallet(dev_mode)]
pub mod pallet_v2 {
	use super::pallet::Balance;
	use frame::prelude::*;

	#[docify::export(config_v2)]
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type of the runtime.
		type RuntimeEvent: From<Event<Self>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>
			+ TryInto<Event<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type Balances<T: Config> = StorageMap<_, _, T::AccountId, Balance>;

	#[pallet::storage]
	pub type TotalIssuance<T: Config> = StorageValue<_, Balance>;

	#[docify::export]
	#[pallet::error]
	pub enum Error<T> {
		/// Account does not exist.
		NonExistentAccount,
		/// Account does not have enough balance.
		InsufficientBalance,
	}

	#[docify::export]
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A transfer succeeded.
		Transferred { from: T::AccountId, to: T::AccountId, amount: Balance },
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[docify::export(transfer_v2)]
		pub fn transfer(
			origin: T::RuntimeOrigin,
			dest: T::AccountId,
			amount: Balance,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			// ensure sender has enough balance, and if so, calculate what is left after `amount`.
			let sender_balance =
				Balances::<T>::get(&sender).ok_or(Error::<T>::NonExistentAccount)?;
			let remainder =
				sender_balance.checked_sub(amount).ok_or(Error::<T>::InsufficientBalance)?;

			Balances::<T>::mutate(&dest, |b| *b = Some(b.unwrap_or(0) + amount));
			Balances::<T>::insert(&sender, remainder);

			Self::deposit_event(Event::<T>::Transferred { from: sender, to: dest, amount });

			Ok(())
		}
	}

	#[cfg(any(test, doc))]
	pub mod tests {
		use super::{super::pallet::tests::StateBuilder, *};
		use frame::testing_prelude::*;
		const ALICE: u64 = 1;
		const BOB: u64 = 2;

		#[docify::export]
		pub mod runtime_v2 {
			use super::*;
			use crate::guides::your_first_pallet::pallet_v2 as pallet_currency;

			construct_runtime!(
				pub enum Runtime {
					System: frame_system,
					Currency: pallet_currency,
				}
			);

			#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
			impl frame_system::Config for Runtime {
				type Block = MockBlock<Runtime>;
				type AccountId = u64;
			}

			impl pallet_currency::Config for Runtime {
				type RuntimeEvent = RuntimeEvent;
			}
		}

		pub(crate) use runtime_v2::*;

		#[docify::export(transfer_works_v2)]
		#[test]
		fn transfer_works() {
			StateBuilder::default().build_and_execute(|| {
				// skip the genesis block, as events are not deposited there and we need them for
				// the final assertion.
				System::set_block_number(ALICE);

				// given the initial state, when:
				assert_ok!(Pallet::<Runtime>::transfer(RuntimeOrigin::signed(ALICE), BOB, 50));

				// then:
				assert_eq!(Balances::<Runtime>::get(&ALICE), Some(50));
				assert_eq!(Balances::<Runtime>::get(&BOB), Some(150));
				assert_eq!(TotalIssuance::<Runtime>::get(), Some(200));

				// now we can also check that an event has been deposited:
				assert_eq!(
					System::read_events_for_pallet::<Event<Runtime>>(),
					vec![Event::Transferred { from: ALICE, to: BOB, amount: 50 }]
				);
			});
		}
	}
}
