// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! As our runtime should _never_ panic, we should carefully handle [`Result`]/[`Option`]
//! types, eliminating the possibility of integer overflows, converting between number types, or
//! even replacing floating point usage with fixed point arithmetic to mitigate issues that come
//! with floating point calculations.
//!
//! Intentional and predictable design should be our first and foremost
//! priority for ensuring a well running, safely designed system.
//!
//! ## Defensive Programming
//!
//! [Defensive programming](https://en.wikipedia.org/wiki/Defensive_programming) is a design paradigm that enables a particular program to continue
//! running despite unexpected behavior, input or events which may arise in runtime. Normally,
//! unforeseen circumstances may cause the program to stop or, in the Rust context, `panic!`.
//! Defensive practices allow for these circumstances to be accounted for ahead of time and for them
//! to be handled in a graceful manner, which is in the line of the intended, fault-tolerant and
//! deterministic behavior of blockchains.
//!
//! The Polkadot SDK is built to reflect these principles and to facilitate their usage
//! accordingly.
//!
//! ## General Practices
//!
//! When developing within the context of the a runtime, there is *one* golden rule:
//!
//! ***DO NOT PANIC***. There are some exceptions, which will be covered later on in this doc.
//!
//! > It's important to make the differentiation between the **runtime** and **node**.  The runtime
//! > refers to the core business logic of a Substrate-based chain, whereas the node refers to the
//! > outer client which deals with telemetry and gossip from other nodes. For more information,
//! > read about Substrate's architecture.
//! > It's also important to note that the criticality of the **node** is slightly lesser than that
//! > of the
//! > **runtime**, which is why in a few places of the node's code repository, you may see
//! > `unwrap()` or other "non-defensive"
//! > code instances.
//!
//!  General guidelines:
//!
//! - Avoid writing functions that could explicitly panic. Directly using `unwrap()` on a
//!   [`Result`], or  accessing an out-of-bounds index on a collection, should be avoided. Safer
//!   methods to access collection types, i.e., `get()` which allow defensive handling of the
//!   resulting [`Option`] are recommended to be used.
//! - It may be acceptable to use `except()`, but only if one is completely certain (and has
//!   performed a check beforehand) that a value won't panic upon unwrapping.  Even this is
//!   discouraged, however, as future changes to that function could then cause that statement to
//!   panic.  It is important to ensure all possible errors are propagated and handled effectively.
//! - If a function *can* panic, it usually is prefaced with `unchecked_` to indicate its unsafety.
//! - If you are writing a function that could panic, [be sure to document it!](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#documenting-components)
//! - Carefully handle mathematical operations.  Many seemingly, simplistic operations, such as
//!   **arithmetic** in the runtime, could present a number of issues [(see more later in this
//!   document)](#integer-overflow). Use checked arithmetic wherever possible.
//!
//! ### Examples of when to `panic!`
//!
//! As you traverse through the codebase (particularly in `substrate/frame`, where the majority of
//! runtime code lives), you may notice that there occurrences where `panic!` is used explicitly.
//! This is used when the runtime should stall, rather than keep running, as that is considered
//! safer. Particularly when it comes to mission critical components, such as block authoring,
//! consensus, or other protocol-level dependencies, the unauthorized nature of a node may actually
//! cause harm to the network, and thus stalling would be the better option.
//!
//! Take the example of the BABE pallet ([`pallet_babe`]), which doesn't allow for a validator to
//! participate if it is disabled (see: [frame::traits::DisabledValidators]):
//!
//! ```rust
//! if T::DisabledValidators::is_disabled(authority_index) {
//! 	panic!(
//! 		"Validator with index {:?} is disabled and should not be attempting to author blocks.",
//! 		authority_index,
//! 	);
//! }
//! ```
//!
//! There are other such examples in various pallets, mostly those that are crucial to the
//! blockchain's functionality.
//!
//! ### Defensive Traits
//!
//! The [`Defensive`](frame::traits::Defensive) trait provides a number of functions, all of which
//! provide an alternative to 'vanilla' Rust functions, e.g.,:
//!
//! - [`defensive_unwrap_or()`](frame::traits::Defensive::defensive_unwrap_or)
//! - [`defensive_ok_or()`](frame::traits::DefensiveOption::defensive_ok_or)
//!
//! The [`Defensive`](frame::traits::Defensive) trait and its companions,
//! [`DefensiveOption`](frame::traits::DefensiveOption),
//! [`DefensiveResult`](frame::traits::DefensiveResult) can be used to defensively unwrap
//! and handle values.  This can be used in place of
//! an `expect`, and again, only if the developer is sure about the unwrap in the first place.
//!
//! Here is a full list of all defensive types:
//!
//! - [`DefensiveOption`](frame::traits::DefensiveOption)
//! - [`DefensiveResult`](frame::traits::DefensiveResult)
//! - [`DefensiveMax`](frame::traits::DefensiveMax)
//! - [`DefensiveSaturating`](frame::traits::DefensiveSaturating)
//! - [`DefensiveTruncateFrom`](frame::traits::DefensiveTruncateFrom)
//!
//! All of which can be used by importing
//! [`frame::traits::defensive_prelude`](frame::traits::defensive_prelude), which imports all
//! defensive traits at once.
//!
//! Defensive methods use [`debug_assertions`](https://doc.rust-lang.org/reference/conditional-compilation.html#debug_assertions), which panic in development, but in
//! production/release, they will merely log an error (i.e., `log::error`).
//!
//! ## Integer Overflow
//!
//! The Rust compiler prevents any sort of static overflow from happening at compile time.  
//! The compiler panics in **debug** mode in the event of an integer overflow. In
//! **release** mode, it resorts to silently _wrapping_ the overflowed amount in a modular fashion
//! (from the `MAX` back to zero).
//!
//! In the context of runtime development, we don't always have control over what is being supplied
//! as a parameter. For example, even this simple add function could present one of two outcomes
//! depending on whether it is in **release** or **debug** mode:
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", naive_add)]
//!
//! If we passed in overflow-able values at runtime, this could actually panic (or wrap, if in
//! release).
//!
//! ```ignore
//! naive_add(250u8, 10u8); // In debug mode, this would panic. In release, this would return 4.
//! ```
//!
//! It is the _silent_ portion of this behavior that presents a real issue. Such behavior should be
//! made obvious, especially in the context of blockchain development, where unsafe arithmetic could
//! produce unexpected consequences like a user balance over or underflowing.
//!
//! Fortunately, there are ways to both represent and handle these scenarios depending on our
//! specific use case natively built into Rust, as well as libraries like [`sp_arithmetic`].
//!
//! ## Infallible Arithmetic
//!
//! Both Rust and Substrate provide safe ways to deal with numbers and alternatives to floating
//! point arithmetic.
//!
//! A developer should use fixed-point instead of floating-point arithmetic to mitigate the
//! potential for inaccuracy, rounding errors, or other unexpected behavior.
//!
//! Using floating point number types in the runtime should be avoided,
//! as a single non-deterministic result could cause chaos for blockchain consensus along with the
//! aforementioned issues. For more on the specifics of the peculiarities of floating point calculations, [watch this video by the Computerphile](https://www.youtube.com/watch?v=PZRI1IfStY0).
//!
//! The following methods demonstrate different ways one can handle numbers natively in Rust in a
//! safe manner, without fear of panic or unexpected behavior from wrapping.
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
//! Typically, if you aren't sure about which operation to use for runtime math, **checked**
//! operations are a safe bet, as it presents two, predictable (and _erroring_) outcomes that can be
//! handled accordingly (`Some` and `None`).
//!
//! In a practical context, the resulting [`Option`] should be handled accordingly. The following
//! conventions can be seen within the Polkadot SDK, where it is handled in
//! two ways:
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
//! Optionally, `match` may also be directly used in a more concise manner:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    increase_balance_match
)]
//!
//! This is generally a useful convention for handling not only checked types, but most types that
//! return `Option<T>`.
//!
//! #### Handling via Result - Less Verbose
//!
//! In the Polkadot SDK codebase, you may see checked operations being handled as a [`Result`] via
//! `ok_or`. This is a less verbose way of expressing the above. This usage often boils down to
//! the developer's preference:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    increase_balance_result
)]
//!
//! ### Saturating Operations
//!
//! Saturating a number limits it to the type's upper or lower bound, even if the integer were to
//! overflow in runtime. For example, adding to `u32::MAX` would simply limit itself to `u32::MAX`:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    saturated_add_example
)]
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
//! Known scenarios that could be fallible should be avoided: i.e., avoiding the possibility of
//! dividing/modulo by zero at any point should be mitigated. One should be opting for a
//! `checked_*` method to introduce safe arithmetic in their code.
//!
//! #### The problem with 'default' wrapped operations
//!
//! **Wrapped operations** cause the overflow to wrap around to either the maximum or minimum of
//! that type. Imagine this in the context of a blockchain, where there are account balances, voting
//! counters, nonces for transactions, and other aspects of a blockchain.
//!
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
//! Alice's balance has reached `0` after a transfer to Bob. Suddenly, she has been slashed on
//! `EduChain`, causing her balance to reach near the limit of `u32::MAX` - a very large amount - as
//! _wrapped operations_ can go both ways. Alice can now successfully vote using her new,
//! overpowered token balance, destroying the integrity of the chain.
//!
//! <details>
//!   <summary><b>Solution: Saturating</b></summary>
//!   For Alice's balance problem, using `saturated_sub` could've mitigated this issue.  As debt or
//!   having a negative balance is not a concept within blockchains, a saturating calculation
//! would've simply limited her balance to the lower bound of u32.
//!
//!   In other words: Alice's balance would've stayed at "0", even after being slashed.
//!
//!   This is also an example that while one system may work in isolation, shared interfaces, such
//!   as the notion of balances, are often shared across multiple pallets - meaning these small
//!   changes can make a big difference in outcome. </details>
//!
//! #### Proposal ID Overwrite
//!
//! The type for counting the number of proposals on-chain is represented by a `u8` number, called
//! `proposals_count`. Every time a new proposal is added to the system, this number increases. With
//! the proposal pallet being high in usage, it has reached `u8::MAX`'s limit of `255`, causing
//! `proposals_count` to go to `0`. Unfortunately, this results in new proposals overwriting old
//! ones, effectively erasing any notion of past proposals!
//!
//! <details>
//!  <summary><b>Solution: Checked</b></summary>
//! For the proposal IDs, proper handling via `checked` math would've been much more suitable,
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
//! runtime. Of course, it may be that using unchecked math is perfectly fine under some scenarios -
//! such as certain balance being never realistically attainable, or a number type being so large
//! that it could never realistically overflow unless one sent thousands of transactions to the
//! network.
//!
//! ### Decision Chart: When to use which?
#![doc = simple_mermaid::mermaid!("../../../mermaid/integer_operation_decision.mmd")]
//! ## Other Resources
//!
//! - [PBA Book - FRAME Tips & Tricks](https://polkadot-blockchain-academy.github.io/pba-book/substrate/tips-tricks/page.html?highlight=perthing#substrate-and-frame-tips-and-tricks)

#[cfg(test)]
mod tests {
	enum BlockchainError {
		Overflow,
	}

	type Address = ();

	struct Runtime;

	impl Runtime {
		fn get_balance(account: Address) -> u64 {
			0
		}
	}

	#[docify::export]
	#[test]
	fn naive_add(x: u8, y: u8) -> u8 {
		x + y
	}

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
	fn increase_balance(account: Address, amount: u64) -> Result<(), BlockchainError> {
		// Get a user's current balance
		let balance = Runtime::get_balance(account)?;
		// SAFELY increase the balance by some amount
		if let Some(new_balance) = balance.checked_add(amount) {
			Runtime::set_balance(account, new_balance);
			return Ok(());
		} else {
			return Err(BlockchainError::Overflow);
		}
	}

	#[docify::export]
	#[test]
	fn increase_balance_match(account: Address, amount: u64) -> Result<(), BlockchainError> {
		// Get a user's current balance
		let balance = Runtime::get_balance(account)?;
		// SAFELY increase the balance by some amount
		let new_balance = match balance.checked_add(amount) {
			Some(balance) => balance,
			None => {
				return Err(BlockchainError::Overflow);
			},
		};
		Runtime::set_balance(account, new_balance);
		Ok(())
	}

	#[docify::export]
	#[test]
	fn increase_balance_result(account: Address, amount: u64) -> Result<(), BlockchainError> {
		// Get a user's current balance
		let balance = Runtime::get_balance(account)?;
		// SAFELY increase the balance by some amount - this time, by using `ok_or`
		let new_balance = balance.checked_add(amount).ok_or_else(|| BlockchainError::Overflow)?;
		Runtime::set_balance(account, new_balance);
		Ok(())
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
