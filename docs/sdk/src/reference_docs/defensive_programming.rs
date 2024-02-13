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
//! to be handled in a graceful manner, which is in the line of the intended, fault-tolerant and deterministic
//! behavior of blockchains.
//!
//! The Polkadot SDK is built to reflect these principles and to facilitate their usage
//! accordingly.
//!
//! ## General Practices
//!
//! When developing within the context of the a runtime, there is *one* golden rule:
//!
//! ***DO NOT PANIC***. There are some exceptions, such as critical operations being actually more
//! dangerous than allowing the runtime to continue functioning (block authoring, consensus, etc).
//!
//! > It's important to make the differentiation between the **runtime** and **node**.  The runtime
//! > refers to the core business logic of a Substrate-based chain, whereas the node refers to the
//! > outer client which deals with telemetry and gossip from other nodes. For more information,
//! > read about Substrate's architecture.
//! > It's also important to note that the behavior of the **node** may differ from that of the
//! > **runtime**, which is also why at times, you may see `unwrap()` or other "non-defensive"
//! > behavior taking place.
//!
//!  General guidelines:
//!
//! - Avoid writing functions that could explicitly panic. Directly using `unwrap()` for a
//!   [`Result`], or common errors such as accessing an out of bounds index on a collection should
//!   not be used. Safer methods to access collection types, i.e., `get()` are available, upon which
//!   defensive handling of the resulting [`Option`] can occur.
//! - It may be acceptable to use `except()`, but only if one is completely certain (and has
//!   performed a check beforehand) that a value won't panic upon unwrapping.  Even this is
//!   discouraged, however, as future changes to that function could then cause that statement to
//!   panic.  It is better to ensure all errors are propagated and handled accordingly in some way.
//! - If a function *can* panic, it usually is prefaced with `unchecked_` to indicate its unsafety.
//! - If you are writing a function that could panic, [be sure to document it!](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#documenting-components)
//! - Carefully handle mathematical operations.  Many seemingly, simplistic operations, such as
//!   **arithmetic** in the runtime, could present a number of issues [(see more later in this
//!   document)](#integer-overflow).
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
//! There are other such examples throughout various pallets, mostly those who are crucial to the
//! blockchain's function.
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
//! as a parameter. For example, even this simple adding function could present one of two outcomes
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
//! Luckily, there are ways to both represent and handle these scenarios depending on our specific
//! use case natively built into Rust, as well as libraries like [`sp_arithmetic`].
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
//! as a single nondeterministic result could cause chaos for consensus along with the
//! aforementioned issues. For more on the specifics of the peculiarities of floating point calculations, [watch this video by the Computerphile](https://www.youtube.com/watch?v=PZRI1IfStY0).
//!
//! The following methods demonstrate different ways one can handle numbers safely natively in Rust,
//! without fear of panic or unexpected behavior from wrapping.
//!
//! ### Checked Arithmetic
//!
//! **Checked operations** utilize an `Option<T>` as a return type. This allows for simple pattern
//! matching to catch any unexpected behavior in the event of an overflow.
//!
//! This is an example of a valid operation:
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", checked_add_example)]
//!
//! This is an example of an invalid operation, in this case, a simulated integer overflow, which
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
//! conventions can be seen from the within the Polkadot SDK, where it can be handled in one of
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
//! `ok_or`. This is a less verbose way of expressing the above.  Which to use often boils down to
//! the developer's preference:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    increase_balance_result
)]
//!
//! ### Saturating Operations
//!
//! Saturating a number limits it to the type's upper or lower bound, no matter the integer would
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
//!
//! Known scenarios that could be fallible should be avoided: i.e., avoiding the possibility of
//! dividing/modulo by zero at any point should be mitigated. One should be, instead, opting for a
//! `checked_*` method in order to introduce safe arithmetic in their code.
//!
//! #### The problem with 'default' wrapped operations
//!
//! **Wrapped operations** cause the overflow to wrap around to either the maximum or minimum of
//! that type. Imagine this in the context of a blockchain, where there are balances, voting
//! counters, nonces for transactions, and other aspects of a blockchain.
//!
//! Some of these mechanisms can be more critical than others. It's for this reason that we may
//! consider some other ways of dealing with runtime arithmetic, such as saturated or checked
//! operations, that won't carry these potential consequences.
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
//! #### Proposals' ID Overwrite
//!
//! The type for counting the number of proposals on-chain is represented by a `u8` number, called
//! `proposals_count`. Every time a new proposal is added to the system, this number increases. With
//! the proposal pallet being high in usage, it has reached `u8::MAX`'s limit of `255`, causing
//! `proposals_count` to go to `0`. Unfortunately, this resulted in new proposals overwriting old
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
//! ## Fixed Point Arithmetic
//!
//! The following code uses types from [`sp_arithmetic`].
//!
//! Fixed point arithmetic solves the aforementioned problems of dealing with the uncertain
//! nature of floating point numbers. Rather than use a radix point (`0.80`), a type which
//! _represents_ a floating point number in base 10, i.e., a **fixed point number**, can be used
//! instead.
//!
//! For use cases which operate within the range of `[0, 1]` types that implement
//! [`PerThing`](sp_arithmetic::PerThing) are used:
//! - **[`Perbill`](sp_arithmetic::Perbill), parts of a billion**
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", perbill_example)]
//! - **[`Percent`](sp_arithmetic::Percent), parts of a hundred**
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", percent_example)]
//!
//! Note that `190 / 400 = 0.475`, and that `Percent` represents it as a _rounded down_, fixed point
//! number (`47`). Unlike primitive types, types that implement
//! [`PerThing`](sp_arithmetic::PerThing) will also not overflow, and are therefore safe to use.
//! They adopt the same behavior that a saturated calculation would provide, meaning that if one is
//! to go over "100%", it wouldn't overflow, but simply stop at the upper or lower bound.
//!
//! For use cases which require precision beyond the range of `[0, 1]`, there are a number of other
//! fixed-point types to use:
//!
//! - [`FixedU64`](sp_arithmetic::FixedU64) and [`FixedI64`](sp_arithmetic::FixedI64)
//! - [`FixedI128`](sp_arithmetic::FixedU128) and [`FixedU128`](sp_arithmetic::FixedI128)
//!
//! Similar to types that implement [`PerThing`](sp_arithmetic::PerThing), these are also
//! fixed-point types, however, they are able to represent larger fractions:
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", fixed_u64)]
//!
//! Let's now explore these types in practice, and how they may be used with pallets to perform
//! safer calculations in the runtime.
//!
//! ### 'PerThing' In Practice
//!
//! [`sp_arithmetic`] contains a trait called [`PerThing`](sp_arithmetic::PerThing), allowing a
//! custom type to be implemented specifically for fixed point arithmetic. While a number of
//! fixed-point types are introduced, let's focus on a few specific examples that implement
//! [`PerThing`](sp_arithmetic::PerThing):
//!
//! - [`Percent`](sp_arithmetic::Percent) - parts of one hundred.
//! - [`Permill`](sp_arithmetic::Permill) - parts of a million.
//! - [`Perbill`](sp_arithmetic::Perbill) - parts of a billion.
//!
//! Each of these can be used to construct and represent ratios within our runtime.
//! You will find types like [`Perbill`](sp_arithmetic::Perbill) being used often in pallet
//! development.  [`pallet_referenda`] is a good example of a pallet which makes good use of fixed
//! point arithmetic.
//!
//! Let's examine the usage of `Perbill` and how exactly we can use it as an alternative to floating
//! point numbers in development with Substrate. For this scenario, let's say we are demonstrating a
//! _voting_ system which depends on reaching a certain threshold, or percentage, before it can be
//! deemed valid.
//!
//! For most applications, `Perbill` gives us a reasonable amount of precision, which
//! is why we're using it here.
//!
//! #### Fixed Point Arithmetic with [`PerThing`](sp_arithmetic::PerThing)
//!
//! As stated, one can also perform mathematics using these types directly. For example, finding the
//! percentage of a particular item:
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", percent_mult)]
//!
//! ### Fixed Point Types in Practice
//!
//! As said earlier, if one needs to exceed the value of one, then
//! [`FixedU64`](sp_arithmetic::FixedU64) (and its signed and `u128` counterparts) can be utilized.
//! Take for example this very rudimentary pricing mechanism, where we wish to calculate the demand
//! / supply to get a price for some on-chain compute:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    fixed_u64_block_computation_example
)]
//!
//! For a much more comprehensive example, be sure to look at the source for [`pallet_broker`].
//!
//! #### Fixed Point Types in Practice
//!
//! Just as with [`PerThing`](sp_arithmetic::PerThing), you can also perform regular mathematical
//! expressions:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    fixed_u64_operation_example
)]
//!
//!
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
	fn percent_mult() {
		let percent = Percent::from_rational(5u32, 100u32); // aka, 5%
		let five_percent_of_100 = percent * 100u32; // 5% of 100 is 5.
		assert_eq!(five_percent_of_100, 5)
	}
	#[docify::export]
	#[test]
	fn perbill_example() {
		let p = Perbill::from_percent(80);
		// 800000000 bil, or a representative of 0.800000000.
		// Precision is in the billions place.
		assert_eq!(p.deconstruct(), 800000000);
	}

	#[docify::export]
	#[test]
	fn percent_example() {
		let percent = Percent::from_rational(190u32, 400u32);
		assert_eq!(percent.deconstruct(), 47);
	}

	#[docify::export]
	#[test]
	fn fixed_u64_block_computation_example() {
		// Calculate a very rudimentary on-chain price from supply / demand
		// Supply: Cores available per block
		// Demand: Cores being ordered per block
		let price = FixedU64::from_rational(5u128, 10u128);

		// 0.5 DOT per core
		assert_eq!(price, FixedU64::from_float(0.5));

		// Now, the story has changed - lots of demand means we buy as many cores as there
		// available.  This also means that price goes up! For the sake of simplicity, we don't care
		// about who gets a core - just about our very simple price model

		// Calculate a very rudimentary on-chain price from supply / demand
		// Supply: Cores available per block
		// Demand: Cores being ordered per block
		let price = FixedU64::from_rational(10u32, 19u32);

		// 1.9 DOT per core
		assert_eq!(price, FixedU64::from_float(1.9));
	}

	#[docify::export]
	#[test]
	fn fixed_u64() {
		// The difference between this and perthings is perthings operates within the relam of [0,
		// 1] In cases where we need > 1, we can used fixed types such as FixedU64

		let rational_1 = FixedU64::from_rational(10, 5); //" 200%" aka 2.
		let rational_2 =
			FixedU64::from_rational_with_rounding(5, 10, sp_arithmetic::Rounding::Down); // "50%" aka 0.50...

		assert_eq!(rational_1, (2u64).into());
		assert_eq!(rational_2.into_perbill(), Perbill::from_float(0.5));
	}

	#[docify::export]
	#[test]
	fn fixed_u64_operation_example() {
		let rational_1 = FixedU64::from_rational(10, 5); // "200%" aka 2.
		let rational_2 = FixedU64::from_rational(8, 5); // "160%" aka 1.6.

		let addition = rational_1 + rational_2;
		let multiplication = rational_1 * rational_2;
		let division = rational_1 / rational_2;
		let subtraction = rational_1 - rational_2;

		assert_eq!(addition, FixedU64::from_float(3.6));
		assert_eq!(multiplication, FixedU64::from_float(3.2));
		assert_eq!(division, FixedU64::from_float(1.25));
		assert_eq!(subtraction, FixedU64::from_float(0.4));
	}

	#[docify::export]
	#[test]
	fn bad_unwrap() {
		let some_result: Result<u32, &str> = Ok(10);
		assert_eq!(some_result.unwrap(), 10);
	}

	#[docify::export]
	#[test]
	fn good_unwrap() {
		let some_result: Result<u32, &str> = Err("Error");
		assert_eq!(some_result.unwrap_or_default(), 0);
		assert_eq!(some_result.unwrap_or(10), 10);
	}

	#[docify::export]
	#[test]
	#[should_panic]
	fn bad_collection_retrieval() {
		let my_list = vec![1, 2, 3, 4, 5];
		// THIS PANICS!
		// Indexing on heap allocated values, i.e., vec, can be unsafe!
		assert_eq!(my_list[5], 6)
	}

	#[docify::export]
	#[test]
	fn good_collection_retrieval() {
		let my_list = vec![1, 2, 3, 4, 5];
		// Rust includes `.get`, returning Option<T> - so lets use that:
		assert_eq!(my_list.get(5), None)
	}

	#[docify::export]
	#[test]
	#[cfg_attr(debug_assertions, should_panic(expected = "Defensive failure has been triggered!"))]
	fn saturated_defensive_example() {
		let saturated_defensive = u32::MAX.defensive_saturating_add(10);
		assert_eq!(saturated_defensive, u32::MAX);
	}
}
