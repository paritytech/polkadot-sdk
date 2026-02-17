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

//! Price-Oracle System, intended to be used with pUSD.
//!
//! ## Overview
//!
//! We will use the same validator set as the Polkadot relay chain to oraclize the price. The
//! pallets in this crate will run on a dedicated parachain. The collators of the parachain are
//! automatically updated in sync-with the relay chain validators. The same validators would have to
//! run collators for this parachain too.
//!
//! ## Naming
//!
//! This pallet is located under `frame/staking-async`, mainly because it needs to leverage teh
//! staking-async's runtimes for testing. Then, to keep the naming consistent, it has to have the
//! prefix `staking-async-` in its name, ergo `pallet-staking-async-price-oracle`. While not
//! optimal, we opt for a consistent naming here.
//!
//! ## Structure
//!
//! This crate is multiple pallets and components that are meant to work together.
//!
//! - [`crate::oracle`]: the pallet through which validators submit their price updates. This pallet
//!   implements a `OneSessionHandler`, allowing it to receive updated about the local session
//!   pallet. This local session pallet is controlled by the next component (`client`), and pretty
//!   much mimics the relay chain validators. Of course, relay validators need to use their stash
//!   key once in the price-oracle parachain to:
//! 	* Associate a session key with their stash key.
//! 	* Set a proxy for future use
//! - [`crate::client`]: pallet that receives XCMs indicating new validator sets from the RC.
//! - [`crate::tally`]: Example tally implementations that can be wired up with
//!   [`crate::oracle::Config::TallyManager`].
//! - [`crate::extension`]: Transaction extensions that should be used in the parachain runtime
//!   containing this system.
//!
//! ## Runtime Restriction
//!
//! The runtime containing this system is a special one, not containing any other pallet. Namely, it
//! doesn't have transaction-payment and balances. This has a few implications:
//!
//! * No balance can be teleported to this runtime.
//! * Yet, the runtime should nonetheless prevent spam transactions from random signed accounts, as
//!   they would all be considered valid in the absence of `transaction-payment` and `balances`.
//!
//! To combat this, this crate provides two lines of defense:
//!
//! * a custom transaction-extension that will invalidate all transactions, at the transaction-pool
//!   level, if they are not originating from our known set of authorities.
//! * all calls in [`crate::oracle`] are marked as `DispatchClass::Operational`, meaning that even
//!   if some spam happens, a portion of the block weight remains free for vote transactions. A
//!   portion of the block weight is also in any case reserved for tallying, which happens at the
//!   end of each block.
//!
//! Other notes:
//! * The said runtime should allocate most of the transaction weights to the operational class.
//! * We may use the global dispatch filter, but this is a rather late defense; the spam
//!   transactions would still occupy space in the transaction-pool, and only upon dispatch they
//!   will be filtered out.
//! * The said runtime should prevent any sort of remote call execution via XCM.
//! * The said runtime should indeed allow XCM messages originating from privileged locations such
//!   as AH, and Collectives to be received and dispatched.
//!
//! ## Future Work / Ideas
//!
//! - [ ] Companion binary to manage the OCW storage + submit transactions
//! - [ ] OCWs should not overlap, add the lock mechanism from EPMB
//! - [ ] OCW-fork: Maybe the OCW should include the block has on top of which it is built, such
//!   that we are sure we won't include a transaction from an OCW instance running on a fork on the
//!   main chain?
//! - [ ] One papi test for quick-ish sanity test
//! - [ ] papi test for validator disabling/swapping (checked manually, see
//!   `TweakValidatorSetOption::UsePreviousKickRandom` in the rc)
//! - [ ] Westend integration
//! 	- [ ] Add the ability to send the price updates to westend AH. Simple.
//! - [ ] Add all the other transactions to manage and have a manager origin.
//! - [x] ZN atm is using the same session key as the stash keys. It should be altered to actually
//!   generate new session keys that are not the same as `derive("Alice")` etc and put them in the
//!   keystore and register them. Alternatively, we can write some scripts that at startup. Without
//!   this, our setup is not realistic
//! - [ ] Randomness audit: Where does the OCW randomness come from? We must be sure that validators
//!   don't all pick the same random endpoint for each block's OCW thread.
//! - [ ] Transaction tracing: can the OCW track the status of a previously submitted transaction?
//!   If yes, it will be useful to prevent new OCW from running.
//! - [ ] More integration tests in this crate (ocw running auto, using real tx-extensions/pool
//!   validation etc)
//! - [ ] More tests in integration-tests crate.
//! - [ ] handle validator disabling via a message from RC
//! - [x] Sanity check: is there any issues with running a parachain with 600 collators? don't think
//!   so!
//! - [ ] Slashing for not providing price updates.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

/// The logging target used everywhere in this pallet.
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

macro_rules! client_log {
	($level:tt, $pattern:expr, $( $args:expr ),*) => {
		log::$level!(
			target: $crate::LOG_TARGET,
			concat!("[#{:?}][client] 🤑 ", $pattern), frame_system::Pallet::<T>::block_number() $(, $args)*
		)
	}
}

#[macro_export]
macro_rules! ocw_log {
	($level:tt, $pattern:expr $(, $values:expr)* $(,)?) => {
		log::$level!(
			target: $crate::LOG_TARGET,
			concat!("[#{:?}] [oracle-ocw] 🤑 ", $pattern), frame_system::Pallet::<T>::block_number() $(, $values)*
		)
	};
}

pub mod client;
pub mod extension;
pub mod oracle;
pub mod tally;
