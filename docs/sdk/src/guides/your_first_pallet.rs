//! # Currency Pallet
//!
//! By the end of this guide, you will write a small FRAME pallet (see
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
//! > FRAME-based runtimes use various techniques to re-use a currency pallet ([`pallet_balances`]
//! > or [`pallet_assets`] to be precise) instead of writing one. Further advanced FRAME related
//! > topics are discussed in [`crate::reference_docs`].
//!
//! ## Topics Covered
//!
//! The following FRAME topics are covered in this guide:
//!
//! - [Storage](frame::pallet_macros::storage)
//! - [Call](frame::pallet_macros::call)
//! - [Event](frame::pallet_macros::event)
//! - [Error](frame::pallet_macros::error)
//! - Basics of testing a pallet
//! - [Constructing a runtime](frame::runtime::prelude::construct_runtime)
// TODO: fix links to pallet macros
//!
//! ## Background Knowledge
//!
//! You should have studied the following modules as a prelude to this guide:
//!
//! - [`crate::reference_docs::blockchain_state_machines`]
//! - [`crate::reference_docs::trait_based_programming`]
//! - [`crate::polkadot_sdk::frame_runtime`]
//!
//! ## Setup
//!
//! A pallet is typically a normal rust-crate with the following properties:
//!
//! 1. it imports [`frame`], [`parity_scale_codec`] and [`scale_info`] as bare minimum dependencies.
//! 2. it follows the `std` pattern explained [here](crate::polkadot_sdk::substrate#wasm-build) to
//!    optionally compile itself to native and wasm.
//! 3. it contains `#![cfg_attr(not(feature = "std"), no_std)]`.
//!
//! There is nothing preventing multiple pallets from existing in the same crate, although most
//! developers prefer one-pallet-per-crate in production.
//!
//! > You can find the full source code of this guide in
//! > [`polkadot_sdk_docs_packages_guides_first_pallet`]. This page contains snippets from this
//! > crate.
//!
//! ## Writing Your First Pallet
//!
//! ### Shell Pallet
//!
//! Consider the following as a "shell pallet". We continue building the rest of this pallet based
//! on this template.
//!
//! [`pallet::config`](frame::pallet_macros::config) and
//! [`pallet::pallet`](frame::pallet_macros::pallet) are both mandatory parts of any pallet. Refer
//! to the documentation of each to get an overview of what they do.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", shell_pallet)]
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
//! The definition of these two storage items, based on [`frame::pallet_macros::storage`] details,
//! is as follows:
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", TotalIssuance)]
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", Balances)]
//!
//! ### Dispatchables
//!
//! Next, we will define the dispatchable functions. As per [`frame::pallet_macros::call`], these
//! will be defined as normal `fn`s attached to `struct Pallet`.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", impl_pallet)]
//!
//! The logic of the functions is self-explanatory. Instead, we will focus on the FRAME-related
//! details:
//!
//! - Where do `T::AccountId` and `T::RuntimeOrigin` come from? These are both defined in
//!  [`frame::prelude::frame_system::Config`], therefore we can access them in `T`. Such types are
//! defined further in [`crate::reference_docs::frame_runtime_types`].
//! - What is [`ensure_signed`](frame_system::ensure_signed), and what does it do with the
//!   aforementioned `T::RuntimeOrigin`? This is outside the scope of this guide, and you can learn
//!   more about it [`crate::reference_docs::frame_origin`]. For now, you should only know the
//!   signature of the function: it takes a generic `T::RuntimeOrigin` and returns a
//!   `Result<T::AccountId, _>`. So by the end of this function call, we know that this dispatchable
//!   was signed by some account.
#![doc = docify::embed!("../../substrate/frame/system/src/lib.rs", ensure_signed)]
//!
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
//! [here](`frame::prelude::DispatchError#impl-From<%26'static+str>-for-DispatchError`)). Therefore,
//! we can use basic string literals as our error type and `.into()` them into `DispatchError`.
//!
//! - Why are all `get` and `mutate` functions returning an `Option`? This is the default behavior
//!   of FRAME storage APIs. You can learn more about how to override this by looking into
//!   [`frame::pallet_macros::storage`], and
//!   [`frame::prelude::ValueQuery`]/[`frame::prelude::OptionQuery`]
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
//! This is more or less all the logic that there is this basic currency pallet!
//!
//! ### Your First (Test) Runtime
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
//! being so generic, different types can always be customized to simplify things when needed.
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
//! `RuntimeOrigin` is a [`crate::reference_docs::frame_runtime_types`] generated by the
//! `construct_runtime` macro. Please refer to its rust-docs to inspect the type and how a signed
//! variant is constructed.
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
//!   [`sp_runtime::DispatchError::Module`]. Read more about this in
//!   [`frame::pallet_macros::error`].
//!
//! - **Event**: Events are akin to the return type of dispatchables. They are mostly data blobs
//!   emitted by the runtime to let outside world know what is happening inside the pallet. Since
//!   otherwise, the outside world does not have an easy access to the state changes. They should
//!   represent what happened at the end of a dispatch operation. Therefore, the convention is to
//!   use passive tense for event names (eg. `SomethingHappened`). This allows other sub-systems or
//!   external parties (eg. a light-node, a DApp) to listen to particular events happening, without
//!   needing to re-execute the whole state transition function.
// TODO: both need to be improved a lot at the pallet-macro rust-doc level. Also my explanation
// of event is probably not the best.
//!
//! With the explanation out of the way, let's see how these components can be added. Both follow a
//! fairly familiar syntax: normal Rust enums, with extra
//! [`#[frame::event]`](frame::pallet_macros::event) and
//! [`#[frame::error]`](frame::pallet_macros::error) attributes attached.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", Event)]
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", Error)]
//!
//! One slightly custom part of this is the [`#[pallet::generate_deposit(pub(super) fn
//! deposit_event)]`](frame::pallet_macros::generate_deposit) part. Without going into too
//! much detail, in order for a pallet to emit events to the rest of the system, it needs to do two
//! things:
//!
//! 1. Declare a type in its `Config` that refers to the overarching event type of the runtime. In
//! short, by doing this, the pallet is expressing an important bound: `type RuntimeEvent:
//! From<Event<Self>>`. Read: a `RuntimeEvent` exists, and it can be created from the local `enum
//! Event` of this pallet. This enables the pallet to convert its `Event` into `RuntimeEvent`, and
//! store it where needed.
//!
//! 2. But, doing this conversion and storing is too much to expect each pallet to define. FRAME
//! provides a default way of storing events, and this is what
//! [`pallet::generate_deposit`](frame::pallet_macros::generate_deposit) is doing.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", config_v2)]
//!
//! Then, we can rewrite the `transfer` dispatchable as such:
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", transfer_v2)]
//!
//! Then, notice how now we would need to provide this `type RuntimeEvent` in our test runtime
//! setup.
#![doc = docify::embed!("./packages/guides/first-pallet/src/lib.rs", runtime_v2_event)]
//!
//! In this snippet, the actual `RuntimeEvent` type (right hand side of `type RuntimeEvent =
//! RuntimeEvent`) is generated by
//! [`construct_runtime`](frame::runtime::prelude::construct_runtime). This is explained in depth in
//! [`crate::reference_docs::frame_runtime_types`].
//!
// TODO: we should add a genesis config here as well. Or at least mention it.
//!
//! ## What Next?
//!
//! The next logic step to this guide is to go to [`crate::guides::your_first_runtime`].
//!
//! The following topics where used in this guide, but not covered in depth. It is suggested to
//! study them subsequently:
//!
//! - [`crate::reference_docs::defensive_programming`].
//! - [`crate::reference_docs::frame_origin`].
//! - [`crate::reference_docs::frame_runtime_types`].
//! - The pallet we wrote in this guide was using `dev_mode`, learn more in
//!   [`frame::pallet_macros::config`].
//! - Learn more about the individual pallet items/macros, such as event and errors and call, in
//!   [`frame::pallet_macros`].
