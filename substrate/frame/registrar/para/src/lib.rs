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

//! # User Interface Pallet For Parachain Registrations
//!
//! This pallet exposes the extrinsics that can be used to manage parachain registrations. It
//! communicates over XCM with the `pallet-registrar-relay`

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

/// Used to send an XCM `Transact` to the registrar pallet on the remote relay chain.
pub trait SendToRelay {}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Sends messages to the relay chain.
		type SendToRelay: SendToRelay;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// - TODO: reserve — reserve a ParaId; holds the ParaId deposit.

	// - TODO: register — register code+head for a reserved ParaId; holds the code deposit,
	//   XCM-authorizes on the relay.

	// - TODO: deregister — free the ParaId; releases both deposits.

	// - TODO: swap — swap the slots of two paras. (Do we still need this? leases are deprecated)

	// - TODO: add_lock — add the manager lock.

	// - TODO: remove_lock — remove the manager lock.

	// - TODO: schedule_code_upgrade — schedule a validation-code upgrade (XCM to the relay).

	// - TODO: set_current_head — set the current head data (XCM to the relay).
}
