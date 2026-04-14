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

//! Substrate primitives for the price oracle system.
//!
//! This crate defines the types and runtime API for the price oracle, which allows relay chain
//! validators to collaboratively determine the on-chain price of DOT/USD through a nudge-based
//! gossip protocol.
//!
//! # Overview
//!
//! Validators periodically query external price APIs, compare the result to the current on-chain
//! price, and sign a [`Nudge`] (Up or Down). These signed nudges are gossipped among validators.
//! When a validator authors a block, it selects a subset of collected nudges and includes them as
//! an inherent. The runtime applies the net nudge direction multiplied by an epsilon to update the
//! on-chain price.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_consensus_babe::{AuthorityId, AuthorityIndex, AuthoritySignature};
use sp_consensus_slots::Slot;
use sp_inherents::{InherentData, InherentIdentifier, IsFatalError};
use sp_runtime::FixedU128;

/// The identifier for the price oracle inherent.
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"prcorcl0";

/// A nudge direction indicating whether the on-chain price should go up or down.
#[derive(
	Debug,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub enum Nudge {
	/// The price should increase.
	Up,
	/// The price should decrease.
	Down,
}

/// A signed nudge from a validator.
///
/// Contains the nudge direction, the slot at which the nudge was produced (for freshness
/// validation), the authority index of the signing validator, and the signature.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct SignedNudge {
	/// The nudge direction.
	pub nudge: Nudge,
	/// The BABE slot at which this nudge was produced.
	pub slot: Slot,
	/// The index of the authority that signed this nudge.
	pub authority_index: AuthorityIndex,
	/// The signature over `(nudge, slot)` using the authority's BABE key.
	pub signature: AuthoritySignature,
}

impl SignedNudge {
	/// Encode the payload that is signed: `(nudge, slot)`.
	pub fn signing_payload(nudge: &Nudge, slot: Slot) -> Vec<u8> {
		(nudge, slot).encode()
	}

	/// Verify the signature against the given authority public key.
	pub fn verify(&self, authority: &AuthorityId) -> bool {
		use sp_core::crypto::ByteArray;
		use sp_runtime::traits::Verify;

		let payload = Self::signing_payload(&self.nudge, self.slot);
		let raw_sig = self.signature.clone();
		// Convert AuthoritySignature (app-wrapped) to raw sr25519::Signature for verification
		let raw_public =
			sp_core::sr25519::Public::from_slice(authority.as_slice()).expect("Valid key; qed");
		let raw_signature = sp_core::sr25519::Signature::from_slice(raw_sig.as_ref())
			.expect("Valid signature bytes; qed");
		raw_signature.verify(payload.as_slice(), &raw_public)
	}
}

/// The inherent data for the price oracle: a list of signed nudges selected by the block author.
pub type PriceOracleInherentData = Vec<SignedNudge>;

/// Errors that can occur while checking the price oracle inherent.
#[derive(Encode, Debug)]
#[cfg_attr(feature = "std", derive(Decode, thiserror::Error))]
pub enum InherentError {
	/// A nudge in the inherent has an invalid signature.
	#[cfg_attr(feature = "std", error("Invalid nudge signature for authority index {0}"))]
	InvalidSignature(AuthorityIndex),
	/// A nudge in the inherent is too old (slot is beyond the validity window).
	#[cfg_attr(feature = "std", error("Nudge from slot {0:?} is too old"))]
	StaleNudge(Slot),
	/// Too few nudges were provided (below the runtime minimum).
	#[cfg_attr(feature = "std", error("Too few nudges: got {0}, need {1}"))]
	TooFewNudges(u32, u32),
}

impl IsFatalError for InherentError {
	fn is_fatal_error(&self) -> bool {
		true
	}
}

impl InherentError {
	/// Try to create an instance from the given identifier and data.
	#[cfg(feature = "std")]
	pub fn try_from(id: &InherentIdentifier, mut data: &[u8]) -> Option<Self> {
		if id == &INHERENT_IDENTIFIER {
			<InherentError as Decode>::decode(&mut data).ok()
		} else {
			None
		}
	}
}

/// Auxiliary trait to extract price oracle inherent data.
pub trait PriceOracleInherentDataExt {
	/// Get the price oracle inherent data.
	fn price_oracle_inherent_data(
		&self,
	) -> Result<Option<PriceOracleInherentData>, sp_inherents::Error>;
}

impl PriceOracleInherentDataExt for InherentData {
	fn price_oracle_inherent_data(
		&self,
	) -> Result<Option<PriceOracleInherentData>, sp_inherents::Error> {
		self.get_data(&INHERENT_IDENTIFIER)
	}
}

sp_api::decl_runtime_apis! {
	/// Runtime API for the price oracle.
	pub trait PriceOracleApi {
		/// Get the current on-chain price (0 if not yet set).
		fn current_price() -> FixedU128;

		/// Get the epsilon value (absolute price change per net nudge).
		fn epsilon() -> FixedU128;

		/// Get the nudge validity window in slots.
		fn nudge_validity() -> u64;

		/// Get the current set of BABE authorities (used for signature verification).
		fn authorities() -> Vec<AuthorityId>;

		/// Get the list of price feed endpoints as `(endpoint_id, url_bytes)` pairs.
		/// The node fetches these URLs and passes raw response bytes to `decode_results`.
		fn endpoint_list() -> Vec<(u8, Vec<u8>)>;

		/// Batch-decode raw HTTP response bodies into prices.
		/// Takes `(endpoint_id, raw_response_bytes)` pairs and returns
		/// `(endpoint_id, Option<price>)` — `None` means the response could not be parsed.
		/// Batched to minimize Wasm boundary crossings.
		fn decode_results(data: Vec<(u8, Vec<u8>)>) -> Vec<(u8, Option<FixedU128>)>;
	}
}

/// Provide price oracle inherent data.
///
/// This is used on the node side to inject the selected nudges into the block's inherent data.
#[cfg(feature = "std")]
pub struct InherentDataProvider {
	nudges: PriceOracleInherentData,
}

#[cfg(feature = "std")]
impl InherentDataProvider {
	/// Create a new provider with the given set of signed nudges.
	pub fn new(nudges: PriceOracleInherentData) -> Self {
		Self { nudges }
	}

	/// Create an empty provider (no nudges to include).
	pub fn empty() -> Self {
		Self { nudges: Vec::new() }
	}
}

#[cfg(feature = "std")]
#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for InherentDataProvider {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(INHERENT_IDENTIFIER, &self.nudges)
	}

	async fn try_handle_error(
		&self,
		identifier: &InherentIdentifier,
		error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		Some(Err(sp_inherents::Error::Application(Box::from(InherentError::try_from(
			identifier, error,
		)?))))
	}
}
