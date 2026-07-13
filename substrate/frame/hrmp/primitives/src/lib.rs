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

//! # HRMP-channel shared primitives
//!
//! Types shared by the two halves of AHM v2 HRMP-channel management — the control-plane pallet on
//! the Coretime chain ([`pallet-hrmp-para`]) and the consensus-plane pallet on the relay chain
//! ([`pallet-hrmp-relay`]). This crate is deliberately free of any FRAME, XCM, or network-specific
//! dependency, so a single version of the wire types serves Westend, Kusama and Polkadot, and so
//! both halves can depend on it without forming a dependency cycle between the pallets.
//!
//! ## Versioning
//!
//! The cross-chain message payloads are the interface seam that is expensive to change once
//! deployed (XCM messages are long-lived on the wire). They are therefore modelled as a versioned
//! wrapper enum whose codec index is the on-wire version tag: add new versions as new variants and
//! **never** renumber or remove an existing one.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use scale_info::TypeInfo;

/// HRMP control-plane messages sent to the relay chain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelay {
	/// Version 1 of the HRMP control-plane messages to the relay chain.
	#[codec(index = 0)]
	V1(MessageToRelayV1),
}

/// Version 1 payloads for [`MessageToRelay`].
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToRelayV1 {
	/// Placeholder so the crate compiles before the control-plane operations land.
	///
	/// Replaced by the real messages: open / accept / close HRMP channel.
	#[codec(index = 0)]
	Placeholder,
}

/// HRMP report messages sent back to the parachain.
///
/// The variant's `#[codec(index)]` is the on-wire version tag.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToPara {
	/// Version 1 of the HRMP report messages to the parachain.
	#[codec(index = 0)]
	V1(MessageToParaV1),
}

/// Version 1 payloads for [`MessageToPara`].
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum MessageToParaV1 {
	/// Placeholder so the crate compiles before the report operations land.
	///
	/// Replaced by the real reports: confirm / fail / refund for a channel operation.
	#[codec(index = 0)]
	Placeholder,
}
