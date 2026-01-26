// This file is part of Substrate.

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

//! Price-Oracle System
//!
//! Pallets:
//!
//! - Oracle: the pallet through which validators submit their price bumps. This pallet implements a
//!   `OneSessionHandler`, allowing it to receive updated about the local session pallet. This local
//!   session pallet is controlled by the next component (`Rc-client`), and pretty much mimics the
//!   relay chain validators.
//! 	- Of course, relay validators need to use their stash key once in the price-oracle parachain
//!    to:
//! 		- Set a proxy for future use
//! 		- Associate a session key with their stash key.
//! - Rc-client: pallet that receives XCMs indicating new validator sets from the RC. It also acts
//!   as two components for the local session pallet:
//!   - `ShouldEndSession`: It immediately signals the session pallet that it should end the
//!     previous session once it receives the validator set via XCM.
//!   - `SessionManager`: Once session realizes it has to rotate the session, it will call into its
//!     `SessionManager`, which is also implemented by rc-client, to which it gives the new
//!     validator keys.
//!
//! In short, the flow is as follows:
//!
//! 1. block N: `relay_new_validator_set` is received, validators are kept as `ToPlan(v)`.
//! 2. Block N+1: `should_end_session` returns `true`.
//! 3. Block N+1: Session calls its `SessionManager`, `v` is returned in `plan_new_session`
//! 4. Block N+1: `ToPlan(v)` updated to `Planned`.
//! 5. Block N+2: `should_end_session` still returns `true`, forcing tht local session to trigger a
//!    new session again.
//! 6. Block N+2: Session again calls `SessionManager`, nothing is returned in `plan_new_session`,
//!    and session pallet will enact the `v` previously received.
//!
//! This design hinges on the fact that the session pallet always does 3 calls at the same time when
//! interacting with the `SessionManager`:
//!
//! * `end_session(n)`
//! * `start_session(n+1)`
//! * `new_session(n+2)`
//!
//! Every time `new_session` receives some validator set as return value, it is only enacted on the
//! next session rotation.
//!
//! Notes/TODOs:
//! we might want to still retain a periodic session as well, allowing validators to swap keys in
//! case of emergency.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub(crate) const LOG_TARGET: &str = "runtime::price-oracle";

#[macro_export]
macro_rules! log {
	($level:tt, $pattern:expr, $( $args:expr ),*) => {
		log::$level!(
			target: $crate::LOG_TARGET,
			concat!("[#{:?}/#{:?}] 🤑 ", $pattern), Pallet::<T>::local_block_number(), Pallet::<T>::relay_block_number() $(, $args)*
		)
	};
}

#[macro_export]
macro_rules! ocw_log {
	($level:tt, $pattern:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: $crate::LOG_TARGET,
			concat!("[oracle-ocw] 🤑 ", $pattern) $(, $values)*
		)
	};
}

pub mod oracle;
pub mod client;
pub mod extension;
pub mod tally;


