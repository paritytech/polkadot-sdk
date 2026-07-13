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

//! # Relay-chain HRMP pallet
//!
//! Relay half of HRMP channel management. Runs on the relay chain, applying channel operations
//! received from a parachain ([`pallet-hrmp-para`]) to the relay's legacy `hrmp` routing table and
//! reporting back.
//!
//! No user-facing extrinsics — this half is driven entirely by XCM `Transact` from the parachain
//! half.
//!
//! Bare scaffold — the config, storage, calls and logic land later. Kept
//! abstract (no network-specific dependency) so the same pallet serves Westend, Kusama and
//! Polkadot.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

/// Used to send an XCM `Transact` to the HRMP pallet on the remote parachain.
pub trait SendToPara {}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Sends messages to the parachain.
		type SendToPara: SendToPara;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	// No user-facing extrinsics on the relay side — all user ops live on the parachain half
	// ([`pallet-hrmp-para`]) and arrive here as XCM `Transact` to drive the relay's `hrmp` routing.
}
