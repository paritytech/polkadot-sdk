// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! [Defensive programming](https://en.wikipedia.org/wiki/Defensive_programming) is a design paradigm that enables a program to continue
//! running despite unexpected behavior, input, or events that may arise in runtime.
//! Usually, unforeseen circumstances may cause the program to stop or, in the Rust context,
//! panic!. Defensive practices allow for these circumstances to be accounted for ahead of time
//! and for them to be handled gracefully, which is in line with the intended fault-tolerant and
//! deterministic nature of blockchains.
//!
//! The Polkadot SDK is built to reflect these principles and to facilitate their usage accordingly.
//!
//! ## General Overview
//!
//! When developing within the context of the Substrate runtime, there is one golden rule:
//!
//! ***DO NOT PANIC***. There are some exceptions, but generally, this is the default precedent.
//!
//! > It’s important to differentiate between the runtime and node. The runtime refers to the core
//! > business logic of a Substrate-based chain, whereas the node refers to the outer client, which
//! > deals with telemetry and gossip from other nodes. For more information, read about
//! > [Substrate's node
//! > architecture](crate::reference_docs::wasm_meta_protocol#node-vs-runtime). It’s also important
//! > to note that the criticality of the node is slightly lesser
//! > than that of the runtime, which is why you may see `unwrap()` or other “non-defensive”
//! > approaches
//! in a few places of the node's code repository.
//!
//! Most of these practices fall within Rust's
//! colloquial usage of proper error propagation, handling, and arithmetic-based edge cases.
//!
//!  General guidelines:
//!
//! - **Avoid writing functions that could explicitly panic,** such as directly using `unwrap()` on
//!   a [`Result`], or  accessing an out-of-bounds index on a collection. Safer methods to access
//!   collection types, i.e., `get()` which allow defensive handling of the resulting [`Option`] are
//!   recommended to be used.
//! - **It may be acceptable to use `except()`,** but only if one is completely certain (and has
//!   performed a check beforehand) that a value won't panic upon unwrapping.  *Even this is
//!   discouraged*, however, as future changes to that function could then cause that statement to
//!   panic.  It is important to ensure all possible errors are propagated and handled effectively.
//! - **If a function *can* panic,** it usually is prefaced with `unchecked_` to indicate its
//!   unsafety.
//! - **If you are writing a function that could panic,** [document it!](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#documenting-components)
//! - **Carefully handle mathematical operations.**  Many seemingly, simplistic operations, such as
//!   **arithmetic** in the runtime, could present a number of issues [(see more later in this
//!   document)](#integer-overflow). Use checked arithmetic wherever possible.
//!
//! These guidelines could be summarized in the following example, where `bad_pop` is prone to
//! panicking, and `good_pop` allows for proper error handling to take place:
//!
//!```ignore
//! // Bad pop always requires that we return something, even if vector/array is empty.
//! fn bad_pop<T>(v: Vec<T>) -> T {}
//! // Good pop allows us to return None from the Option if need be.
//! fn good_pop<T>(v: Vec<T>) -> Option<T> {}
//! ```
//!
//! ### Defensive Traits
//!
//! The [`Defensive`](frame::traits::Defensive) trait provides a number of functions, all of which
//! provide an alternative to 'vanilla' Rust functions, e.g.,:
//!
//! - [`defensive_unwrap_or()`](frame::traits::Defensive::defensive_unwrap_or) instead of
//!   `unwrap_or()`
//! - [`defensive_ok_or()`](frame::traits::DefensiveOption::defensive_ok_or) instead of `ok_or()`
//!
//! Defensive methods use [`debug_assertions`](https://doc.rust-lang.org/reference/conditional-compilation.html#debug_assertions), which panic in development, but in
//! production/release, they will merely log an error (i.e., `log::error`).
//!
//! The [`Defensive`](frame::traits::Defensive) trait and its various implementations can be found
//! [here](frame::traits::Defensive).
//!
//! ## Integer Overflow
//!
//! The Rust compiler prevents static overflow from happening at compile time.
//! The compiler panics in **debug** mode in the event of an integer overflow. In
//! **release** mode, it resorts to silently _wrapping_ the overflowed amount in a modular fashion
//! (from the `MAX` back to zero).
//!
//! In runtime development, we don't always have control over what is being supplied
//! as a parameter. For example, even this simple add function could present one of two outcomes
//! depending on whether it is in **release** or **debug** mode:
//!
//! ```ignore
//! fn naive_add(x: u8, y: u8) -> u8 {
//!     x + y
//! }
//! ```
//! If we passed overflow-able values at runtime, this could panic (or wrap if in release).
//!
//! ```ignore
//! naive_add(250u8, 10u8); // In debug mode, this would panic. In release, this would return 4.
//! ```
//!
//! It is the silent portion of this behavior that presents a real issue. Such behavior should be
//! made obvious, especially in blockchain development, where unsafe arithmetic could produce
//! unexpected consequences like a user balance over or underflowing.
//!
//! Fortunately, there are ways to both represent and handle these scenarios depending on our
//! specific use case natively built into Rust and libraries like [`sp_arithmetic`].
//!
//! ## Infallible Arithmetic
//!
//! Both Rust and Substrate provide safe ways to deal with numbers and alternatives to floating
//! point arithmetic.
//!
//! Known scenarios that could be fallible should be avoided: i.e., avoiding the possibility of
//! dividing/modulo by zero at any point should be mitigated. One should be opting for a
//! `checked_*` method to introduce safe arithmetic in their code in most cases.
//!
//! A developer should use fixed-point instead of floating-point arithmetic to mitigate the
//! potential for inaccuracy, rounding errors, or other unexpected behavior.
//!
//! - [Fixed point types](sp_arithmetic::fixed_point) and their associated usage can be found here.
//! - [PerThing](sp_arithmetic::per_things) and its associated types can be found here.
//!
//! Using floating point number types (i.e., f32. f64) in the runtime should be avoided, as a single non-deterministic result could cause chaos for blockchain consensus along with the issues above. For more on the specifics of the peculiarities of floating point calculations, [watch this video by the Computerphile](https://www.youtube.com/watch?v=PZRI1IfStY0).
//!
//! The following methods demonstrate different ways to handle numbers natively in Rust safely,
//! without fear of panic or unexpected behavior from wrapping.
//!
//! ### Checked Arithmetic
//!
//! **Checked operations** utilize an `Option<T>` as a return type. This allows for
//! catching any unexpected behavior in the event of an overflow through simple pattern matching.
//!
//! This is an example of a valid operation:
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", checked_add_example)]
//!
//! This is an example of an invalid operation. In this case, a simulated integer overflow, which
//! would simply result in `None`:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    checked_add_handle_error_example
)]
//!
//! Suppose you aren’t sure which operation to use for runtime math. In that case, checked
//! operations are the safest bet, presenting two predictable (and erroring) outcomes that can be
//! handled accordingly (Some and None).
//!
//! The following conventions can be seen within the Polkadot SDK, where it is
//! handled in two ways:
//!
//! - As an [`Option`], using the `if let` / `if` or `match`
//! - As a [`Result`], via `ok_or` (or similar conversion to [`Result`] from [`Option`])
//!
//! #### Handling via Option - More Verbose
//!
//! Because wrapped operations return `Option<T>`, you can use a more verbose/explicit form of error
//! handling via `if` or `if let`:
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance)]
//!
//! Optionally, match may also be directly used in a more concise manner:
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance_match)]
//!
//! This is generally a useful convention for handling checked types and most types that return
//! `Option<T>`.
//!
//! #### Handling via Result - Less Verbose
//!
//! In the Polkadot SDK codebase, checked operations are handled as a `Result` via `ok_or`. This is
//! a less verbose way of expressing the above. This usage often boils down to the developer’s
//! preference:
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance_result)]
//!
//! ### Saturating Operations
//!
//! Saturating a number limits it to the type’s upper or lower bound, even if the integer type
//! overflowed in runtime. For example, adding to `u32::MAX` would simply limit itself to
//! `u32::MAX`:
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", saturated_add_example)]
//!
//! Saturating calculations can be used if one is very sure that something won't overflow, but wants
//! to avoid introducing the notion of any potential-panic or wrapping behavior.
//!
//! There is also a series of defensive alternatives via
//! [`DefensiveSaturating`](frame::traits::DefensiveSaturating), which introduces the same behavior
//! of the [`Defensive`](frame::traits::Defensive) trait, only with saturating, mathematical
//! operations:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    saturated_defensive_example
)]
//!
//! ### Mathematical Operations in Substrate Development - Further Context
//!
//! As a recap, we covered the following concepts:
//!
//! 1. **Checked** operations - using [`Option`] or [`Result`]
//! 2. **Saturating** operations - limited to the lower and upper bounds of a number type
//! 3. **Wrapped** operations (the default) - wrap around to above or below the bounds of a type
//!
//! #### The problem with 'default' wrapped operations
//!
//! **Wrapped operations** cause the overflow to wrap around to either the maximum or minimum of
//! that type. Imagine this in the context of a blockchain, where there are account balances, voting
//! counters, nonces for transactions, and other aspects of a blockchain.
//!
//! While it may seem trivial, choosing how to handle numbers is quite important. As a thought
//! exercise, here are some scenarios of which will shed more light on when to use which.
//!
//! #### Bob's Overflowed Balance
//!
//! **Bob's** balance exceeds the `Balance` type on the `EduChain`. Because the pallet developer did
//! not handle the calculation to add to Bob's balance with any regard to this overflow, **Bob's**
//! balance is now essentially `0`, the operation **wrapped**.
//!
//! <details>
//!   <summary><b>Solution: Saturating or Checked</b></summary>
//!     For Bob's balance problems, using a `saturating_add` or `checked_add` could've mitigated
//! this issue.  They simply would've reached the upper, or lower bounds, of the particular type for
//! an on-chain balance.  In other words: Bob's balance would've stayed at the maximum of the
//! Balance type. </details>
//!
//! #### Alice's 'Underflowed' Balance
//!
//! Alice’s balance has reached `0` after a transfer to Bob. Suddenly, she has been slashed on
//! EduChain, causing her balance to reach near the limit of `u32::MAX` - a very large amount - as
//! wrapped operations can go both ways. Alice can now successfully vote using her new, overpowered
//! token balance, destroying the chain's integrity.
//!
//! <details>
//!   <summary><b>Solution: Saturating</b></summary>
//!   For Alice's balance problem, using `saturated_sub` could've mitigated this issue. A saturating
//! calculation would've simply limited her balance to the lower bound of u32, as having a negative
//! balance is not a concept within blockchains.   In other words: Alice's balance would've stayed
//! at "0", even after being slashed.
//!
//!   This is also an example that while one system may work in isolation, shared interfaces, such
//!   as the notion of balances, are often shared across multiple pallets - meaning these small
//!   changes can make a big difference depending on the scenario. </details>
//!
//! #### Proposal ID Overwrite
//!
//! A `u8` parameter, called `proposals_count`, represents the type for counting the number of
//! proposals on-chain. Every time a new proposal is added to the system, this number increases.
//! With the proposal pallet's high usage, it has reached `u8::MAX`’s limit of 255, causing
//! `proposals_count` to go to 0. Unfortunately, this results in new proposals overwriting old ones,
//! effectively erasing any notion of past proposals!
//!
//! <details>
//!  <summary><b>Solution: Checked</b></summary>
//! For the proposal IDs, proper handling via `checked` math would've been suitable,
//! Saturating could've been used - but it also would've 'failed' silently. Using `checked_add` to
//! ensure that the next proposal ID would've been valid would've been a viable way to let the user
//! know the state of their proposal:
//!
//! ```ignore
//! let next_proposal_id = current_count.checked_add(1).ok_or_else(|| Error::TooManyProposals)?;
//! ```
//!
//! </details>
//!
//! From the above, we can clearly see the problematic nature of seemingly simple operations in the
//! runtime, and care should be given to ensure a defensive approach is taken.
//!
//! ### Edge cases of `panic!`-able instances in Substrate
//!
//! As you traverse through the codebase (particularly in `substrate/frame`, where the majority of
//! runtime code lives), you may notice that there (only a few!) occurrences where `panic!` is used
//! explicitly. This is used when the runtime should stall, rather than keep running, as that is
//! considered safer. Particularly when it comes to mission-critical components, such as block
//! authoring, consensus, or other protocol-level dependencies, going through with an action may
//! actually cause harm to the network, and thus stalling would be the better option.
//!
//! Take the example of the BABE pallet ([`pallet_babe`]), which doesn't allow for a validator to
//! participate if it is disabled (see: [`frame::traits::DisabledValidators`]):
//!
//! ```ignore
//! if T::DisabledValidators::is_disabled(authority_index) {
//!     panic!(
//!       "Validator with index {:?} is disabled and should not be attempting to author blocks.",
//!         authority_index,
//!     );
//! }
//! ```
//!
//! There are other examples in various pallets, mostly those crucial to the blockchain’s
//! functionality. Most of the time, you will not be writing pallets which operate at this level,
//! but these exceptions should be noted regardless.
//!
//! ## Other Resources
//!
//! - [PBA Book - FRAME Tips & Tricks](https://polkadot-blockchain-academy.github.io/pba-book/substrate/tips-tricks/page.html?highlight=perthing#substrate-and-frame-tips-and-tricks)
#![allow(dead_code)]
#[allow(unused_variables)]
mod fake_runtime_types {
	// Note: The following types are purely for the purpose of example, and do not contain any
	// *real* use case other than demonstrating various concepts.
	pub enum RuntimeError {
		Overflow,
		UserDoesntExist,
	}

	pub type Address = ();

	pub struct Runtime;

	impl Runtime {
		fn get_balance(account: Address) -> Result<u64, RuntimeError> {
			Ok(0u64)
		}

		fn set_balance(account: Address, new_balance: u64) {}
	}

	#[docify::export]
	fn increase_balance(account: Address, amount: u64) -> Result<(), RuntimeError> {
		// Get a user's current balance
		let balance = Runtime::get_balance(account)?;
		// SAFELY increase the balance by some amount
		if let Some(new_balance) = balance.checked_add(amount) {
			Runtime::set_balance(account, new_balance);
			Ok(())
		} else {
			Err(RuntimeError::Overflow)
		}
	}

	#[docify::export]
	fn increase_balance_match(account: Address, amount: u64) -> Result<(), RuntimeError> {
		// Get a user's current balance
		let balance = Runtime::get_balance(account)?;
		// SAFELY increase the balance by some amount
		let new_balance = match balance.checked_add(amount) {
			Some(balance) => balance,
			None => {
				return Err(RuntimeError::Overflow);
			},
		};
		Runtime::set_balance(account, new_balance);
		Ok(())
	}

	#[docify::export]
	fn increase_balance_result(account: Address, amount: u64) -> Result<(), RuntimeError> {
		// Get a user's current balance
		let balance = Runtime::get_balance(account)?;
		// SAFELY increase the balance by some amount - this time, by using `ok_or`
		let new_balance = balance.checked_add(amount).ok_or(RuntimeError::Overflow)?;
		Runtime::set_balance(account, new_balance);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use frame::traits::DefensiveSaturating;
	#[docify::export]
	#[test]
	fn checked_add_example() {
		// This is valid, as 20 is perfectly within the bounds of u32.
		let add = (10u32).checked_add(10);
		assert_eq!(add, Some(20))
	}

	#[docify::export]
	#[test]
	fn checked_add_handle_error_example() {
		// This is invalid - we are adding something to the max of u32::MAX, which would overflow.
		// Luckily, checked_add just marks this as None!
		let add = u32::MAX.checked_add(10);
		assert_eq!(add, None)
	}

	#[docify::export]
	#[test]
	fn saturated_add_example() {
		// Saturating add simply saturates
		// to the numeric bound of that type if it overflows.
		let add = u32::MAX.saturating_add(10);
		assert_eq!(add, u32::MAX)
	}

	#[docify::export]
	#[test]
	#[cfg_attr(debug_assertions, should_panic(expected = "Defensive failure has been triggered!"))]
	fn saturated_defensive_example() {
		let saturated_defensive = u32::MAX.defensive_saturating_add(10);
		assert_eq!(saturated_defensive, u32::MAX);
	}
}
