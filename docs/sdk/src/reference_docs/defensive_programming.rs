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

//! `panic!`. Defensive practices allow for these circumstances to be accounted for ahead of time
//!
//!
//!
//!
//!
//! > [Substrate's node
//! > approaches
//!
//!
//!
//!   recommended to be used.
//!   panic.  It is important to ensure all possible errors are propagated and handled effectively.
//! - **Carefully handle mathematical operations.**  Many seemingly, simplistic operations, such as
//!
//!
//!```ignore
//! fn good_pop<T>(v: Vec<T>) -> Option<T> {}
//!
//!
//!
//!
//!
//!
//!
//! (from the `MAX` back to zero).
//!
//!
//! }
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", checked_add_example)]
//!
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    checked_add_handle_error_example
)]
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance)]
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance_match)]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance_result)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", saturated_add_example)]
//!
//!
//! operations:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    saturated_defensive_example
)]
//!
//!
//!
//!
//!
//!
//!
//!
//!
//! this issue.  They simply would've reached the upper, or lower bounds, of the particular type for
//!
//!
//! token balance, destroying the chain's integrity.
//!
//! calculation would've simply limited her balance to the lower bound of u32, as having a negative
//!
//!
//!
//! `proposals_count` to go to 0. Unfortunately, this results in new proposals overwriting old ones,
//!
//! Saturating could've been used - but it also would've 'failed' silently. Using `checked_add` to
//!
//!
//!
//!
//!
//! considered safer. Particularly when it comes to mission-critical components, such as block
//!
//!
//!       "Validator with index {:?} is disabled and should not be attempting to author blocks.",
//! ```
//!
//!
//!
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

//! `panic!`. Defensive practices allow for these circumstances to be accounted for ahead of time
//!
//!
//!
//!
//!
//! > [Substrate's node
//! > approaches
//!
//!
//!
//!   recommended to be used.
//!   panic.  It is important to ensure all possible errors are propagated and handled effectively.
//! - **Carefully handle mathematical operations.**  Many seemingly, simplistic operations, such as
//!
//!
//!```ignore
//! fn good_pop<T>(v: Vec<T>) -> Option<T> {}
//!
//!
//!
//!
//!
//!
//!
//! (from the `MAX` back to zero).
//!
//!
//! }
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", checked_add_example)]
//!
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    checked_add_handle_error_example
)]
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance)]
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance_match)]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance_result)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", saturated_add_example)]
//!
//!
//! operations:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    saturated_defensive_example
)]
//!
//!
//!
//!
//!
//!
//!
//!
//!
//! this issue.  They simply would've reached the upper, or lower bounds, of the particular type for
//!
//!
//! token balance, destroying the chain's integrity.
//!
//! calculation would've simply limited her balance to the lower bound of u32, as having a negative
//!
//!
//!
//! `proposals_count` to go to 0. Unfortunately, this results in new proposals overwriting old ones,
//!
//! Saturating could've been used - but it also would've 'failed' silently. Using `checked_add` to
//!
//!
//!
//!
//!
//! considered safer. Particularly when it comes to mission-critical components, such as block
//!
//!
//!       "Validator with index {:?} is disabled and should not be attempting to author blocks.",
//! ```
//!
//!
//!
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

//! `panic!`. Defensive practices allow for these circumstances to be accounted for ahead of time
//!
//!
//!
//!
//!
//! > [Substrate's node
//! > approaches
//!
//!
//!
//!   recommended to be used.
//!   panic.  It is important to ensure all possible errors are propagated and handled effectively.
//! - **Carefully handle mathematical operations.**  Many seemingly, simplistic operations, such as
//!
//!
//!```ignore
//! fn good_pop<T>(v: Vec<T>) -> Option<T> {}
//!
//!
//!
//!
//!
//!
//!
//! (from the `MAX` back to zero).
//!
//!
//! }
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", checked_add_example)]
//!
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    checked_add_handle_error_example
)]
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance)]
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance_match)]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance_result)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", saturated_add_example)]
//!
//!
//! operations:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    saturated_defensive_example
)]
//!
//!
//!
//!
//!
//!
//!
//!
//!
//! this issue.  They simply would've reached the upper, or lower bounds, of the particular type for
//!
//!
//! token balance, destroying the chain's integrity.
//!
//! calculation would've simply limited her balance to the lower bound of u32, as having a negative
//!
//!
//!
//! `proposals_count` to go to 0. Unfortunately, this results in new proposals overwriting old ones,
//!
//! Saturating could've been used - but it also would've 'failed' silently. Using `checked_add` to
//!
//!
//!
//!
//!
//! considered safer. Particularly when it comes to mission-critical components, such as block
//!
//!
//!       "Validator with index {:?} is disabled and should not be attempting to author blocks.",
//! ```
//!
//!
//!
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

//! `panic!`. Defensive practices allow for these circumstances to be accounted for ahead of time
//!
//!
//!
//!
//!
//! > [Substrate's node
//! > approaches
//!
//!
//!
//!   recommended to be used.
//!   panic.  It is important to ensure all possible errors are propagated and handled effectively.
//! - **Carefully handle mathematical operations.**  Many seemingly, simplistic operations, such as
//!
//!
//!```ignore
//! fn good_pop<T>(v: Vec<T>) -> Option<T> {}
//!
//!
//!
//!
//!
//!
//!
//! (from the `MAX` back to zero).
//!
//!
//! }
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", checked_add_example)]
//!
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    checked_add_handle_error_example
)]
//!
//!
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance)]
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance_match)]
//!
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", increase_balance_result)]
//!
//!
#![doc = docify::embed!("./src/reference_docs/defensive_programming.rs", saturated_add_example)]
//!
//!
//! operations:
#![doc = docify::embed!(
    "./src/reference_docs/defensive_programming.rs",
    saturated_defensive_example
)]
//!
//!
//!
//!
//!
//!
//!
//!
//!
//! this issue.  They simply would've reached the upper, or lower bounds, of the particular type for
//!
//!
//! token balance, destroying the chain's integrity.
//!
//! calculation would've simply limited her balance to the lower bound of u32, as having a negative
//!
//!
//!
//! `proposals_count` to go to 0. Unfortunately, this results in new proposals overwriting old ones,
//!
//! Saturating could've been used - but it also would've 'failed' silently. Using `checked_add` to
//!
//!
//!
//!
//!
//! considered safer. Particularly when it comes to mission-critical components, such as block
//!
//!
//!       "Validator with index {:?} is disabled and should not be attempting to author blocks.",
//! ```
//!
//!
//!
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

// [`DefensiveSaturating`]: frame::traits::DefensiveSaturating
// [`PerThing`]: sp_arithmetic::per_things
// [`here`]: frame::traits::Defensive
// [`pallet_babe`]: pallet_babe
