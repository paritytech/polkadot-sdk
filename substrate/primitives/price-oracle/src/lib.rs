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

//! Primitives shared by the price oracle node service and the price oracle pallet.
//!
//! Block authors collect signed price reports from the oracle nodes of the network and include
//! them in a block as an inherent. The runtime verifies the reports and aggregates them into
//! on-chain prices.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod inherents;
pub mod market;
pub mod runtime_api;

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_arithmetic::FixedU128;
use sp_runtime::RuntimeAppPublic;

/// The block height a price report is anchored to.
///
/// Reports carry the height of the best block known to the signer at signing time. The runtime
/// uses it to order reports of the same signer and to expire old ones.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct Anchor(pub u32);

/// Identifier of a pair of assets, such as DOT/USD.
///
/// The set of pairs and the meaning of each identifier is defined by the runtime.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct PairId(pub u16);

/// A price with 18 decimal places, in quote asset units per one base asset unit.
pub type Price = FixedU128;

/// A price for a pair.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub struct Quote {
	/// The pair being priced.
	pub pair: PairId,
	/// The price of the pair.
	pub price: Price,
}

/// Context prefixed to a report before signing.
///
/// Keeps report signatures distinct from any other signature made with the same key.
pub const SIGNING_CONTEXT: &[u8; 12] = b"price-report";

/// A set of prices published by an oracle node, anchored to one block height.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct PriceReport {
	/// The block height the report is anchored to.
	pub anchor: Anchor,
	/// Prices for the pairs the node was able to price. Any subset of the pairs known to the
	/// runtime, at most one quote per pair.
	pub quotes: Vec<Quote>,
}

impl PriceReport {
	/// The bytes a signer signs: [`SIGNING_CONTEXT`] followed by the encoded report.
	pub fn signing_payload(&self) -> Vec<u8> {
		let mut payload = SIGNING_CONTEXT.to_vec();
		self.encode_to(&mut payload);
		payload
	}
}

/// A price report together with the key that signed it and the signature.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct SignedPriceReport<Id, Signature> {
	/// The report.
	pub report: PriceReport,
	/// The public key of the signer.
	pub signer: Id,
	/// Signature of `signer` over [`PriceReport::signing_payload`].
	pub signature: Signature,
}

impl<Id: RuntimeAppPublic<Signature = Signature>, Signature> SignedPriceReport<Id, Signature> {
	/// Check that `signature` is a valid signature of `signer` over the report.
	///
	/// Does not check whether the signer is an accepted oracle signer.
	pub fn verify_signature(&self) -> bool {
		self.signer.verify(&self.report.signing_payload(), &self.signature)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_consensus_aura::sr25519::{AuthorityId, AuthorityPair, AuthoritySignature};
	use sp_core::{crypto::Pair as _, sr25519, ByteArray};

	type Signed = SignedPriceReport<AuthorityId, AuthoritySignature>;

	fn report() -> PriceReport {
		PriceReport {
			anchor: Anchor(1_000),
			quotes: vec![
				Quote { pair: PairId(1), price: Price::from_rational(4_206, 1_000) },
				Quote { pair: PairId(3), price: Price::from_rational(9_998, 10_000) },
			],
		}
	}

	fn sign(pair: &sr25519::Pair, report: PriceReport) -> Signed {
		let signature = pair.sign(&report.signing_payload());
		SignedPriceReport {
			report,
			signer: AuthorityId::from(pair.public()),
			signature: AuthoritySignature::from(signature),
		}
	}

	#[test]
	fn signing_payload_is_context_then_encoded_report() {
		let report = report();
		let payload = report.signing_payload();
		assert_eq!(&payload[..SIGNING_CONTEXT.len()], SIGNING_CONTEXT);
		assert_eq!(&payload[SIGNING_CONTEXT.len()..], report.encode());
	}

	#[test]
	fn valid_signature_verifies() {
		let pair = sr25519::Pair::from_seed(&[1; 32]);
		assert!(sign(&pair, report()).verify_signature());
	}

	#[test]
	fn tampered_report_does_not_verify() {
		let pair = sr25519::Pair::from_seed(&[1; 32]);
		let mut signed = sign(&pair, report());
		signed.report.quotes[0].price = Price::from_rational(4_207, 1_000);
		assert!(!signed.verify_signature());

		let mut signed = sign(&pair, report());
		signed.report.anchor = Anchor(1_001);
		assert!(!signed.verify_signature());
	}

	#[test]
	fn signature_of_another_key_does_not_verify() {
		let pair = sr25519::Pair::from_seed(&[1; 32]);
		let other = sr25519::Pair::from_seed(&[2; 32]);
		let mut signed = sign(&pair, report());
		signed.signer = AuthorityId::from(other.public());
		assert!(!signed.verify_signature());
	}

	#[test]
	fn signed_report_codec_round_trip() {
		let pair = sr25519::Pair::from_seed(&[1; 32]);
		let signed = sign(&pair, report());
		let decoded = Signed::decode(&mut &signed.encode()[..]).unwrap();
		assert_eq!(decoded, signed);
		assert!(decoded.verify_signature());
	}

	#[test]
	fn app_key_types_match_raw_sr25519() {
		// The Aura key wraps a raw sr25519 key without changing its bytes.
		let pair = AuthorityPair::from_seed(&[1; 32]);
		let raw = sr25519::Pair::from_seed(&[1; 32]);
		assert_eq!(ByteArray::to_raw_vec(&pair.public()), raw.public().to_raw_vec());
	}
}
