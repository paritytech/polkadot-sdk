// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! A subxt [`Config`] for JAM parachain collators.
//!
//! JAM parachain headers carry an `AdditionalData` digest (SCALE discriminant `3`) that subxt's
//! own digest item does not know. This module mirrors subxt's header and digest types and adds
//! that variant, encoding it byte-identically to `sp_runtime::generic::DigestItem::AdditionalData`
//! so that digests can be re-encoded and decoded as substrate digests (which is what the
//! digest-reading helpers do).

use codec::{Decode, Encode, Error, Input, Output};
use serde::{Deserialize, Serialize};
use zombienet_sdk::subxt::{
	config::{
		polkadot::PolkadotExtrinsicParams,
		substrate::{ConsensusEngineId, DynamicHasher256, NumberOrHex},
		Config, Header,
	},
	utils::H256,
	PolkadotConfig,
};

// SCALE discriminants from `sp_runtime::generic::DigestItemType`.
const OTHER: u8 = 0;
const ADDITIONAL_DATA: u8 = 3;
const CONSENSUS: u8 = 4;
const SEAL: u8 = 5;
const PRE_RUNTIME: u8 = 6;
const RUNTIME_ENVIRONMENT_UPDATED: u8 = 8;

/// Digest item of a JAM parachain header.
///
/// Mirrors `subxt::config::substrate::DigestItem` and adds the `AdditionalData` variant, with the
/// exact SCALE layout of `sp_runtime::generic::DigestItem`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DigestItem {
	/// A pre-runtime digest.
	PreRuntime(ConsensusEngineId, Vec<u8>),
	/// A message from the runtime to the consensus engine.
	Consensus(ConsensusEngineId, Vec<u8>),
	/// A seal.
	Seal(ConsensusEngineId, Vec<u8>),
	/// Some other thing.
	Other(Vec<u8>),
	/// An indication for light clients that the runtime execution environment is updated.
	RuntimeEnvironmentUpdated,
	/// Per-block additional data hash.
	AdditionalData([u8; 32]),
}

impl Encode for DigestItem {
	fn encode_to<W: Output + ?Sized>(&self, dest: &mut W) {
		match self {
			Self::PreRuntime(id, data) => {
				PRE_RUNTIME.encode_to(dest);
				(id, data).encode_to(dest);
			},
			Self::Consensus(id, data) => {
				CONSENSUS.encode_to(dest);
				(id, data).encode_to(dest);
			},
			Self::Seal(id, data) => {
				SEAL.encode_to(dest);
				(id, data).encode_to(dest);
			},
			Self::Other(data) => {
				OTHER.encode_to(dest);
				data.encode_to(dest);
			},
			Self::RuntimeEnvironmentUpdated => RUNTIME_ENVIRONMENT_UPDATED.encode_to(dest),
			Self::AdditionalData(hash) => {
				ADDITIONAL_DATA.encode_to(dest);
				hash.encode_to(dest);
			},
		}
	}
}

impl codec::EncodeLike for DigestItem {}

impl Decode for DigestItem {
	fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
		let tag = u8::decode(input)?;
		match tag {
			PRE_RUNTIME => {
				let (id, data) = <(ConsensusEngineId, Vec<u8>)>::decode(input)?;
				Ok(Self::PreRuntime(id, data))
			},
			CONSENSUS => {
				let (id, data) = <(ConsensusEngineId, Vec<u8>)>::decode(input)?;
				Ok(Self::Consensus(id, data))
			},
			SEAL => {
				let (id, data) = <(ConsensusEngineId, Vec<u8>)>::decode(input)?;
				Ok(Self::Seal(id, data))
			},
			OTHER => Ok(Self::Other(Decode::decode(input)?)),
			RUNTIME_ENVIRONMENT_UPDATED => Ok(Self::RuntimeEnvironmentUpdated),
			ADDITIONAL_DATA => Ok(Self::AdditionalData(Decode::decode(input)?)),
			_ => Err(Error::from("Unknown digest item discriminant")),
		}
	}
}

impl From<DigestItem> for sp_runtime::generic::DigestItem {
	fn from(item: DigestItem) -> Self {
		match item {
			DigestItem::PreRuntime(id, data) => Self::PreRuntime(id, data),
			DigestItem::Consensus(id, data) => Self::Consensus(id, data),
			DigestItem::Seal(id, data) => Self::Seal(id, data),
			DigestItem::Other(data) => Self::Other(data),
			DigestItem::RuntimeEnvironmentUpdated => Self::RuntimeEnvironmentUpdated,
			DigestItem::AdditionalData(hash) => Self::AdditionalData(hash),
		}
	}
}

impl From<sp_runtime::generic::DigestItem> for DigestItem {
	fn from(item: sp_runtime::generic::DigestItem) -> Self {
		match item {
			sp_runtime::generic::DigestItem::PreRuntime(id, data) => Self::PreRuntime(id, data),
			sp_runtime::generic::DigestItem::Consensus(id, data) => Self::Consensus(id, data),
			sp_runtime::generic::DigestItem::Seal(id, data) => Self::Seal(id, data),
			sp_runtime::generic::DigestItem::Other(data) => Self::Other(data),
			sp_runtime::generic::DigestItem::RuntimeEnvironmentUpdated => {
				Self::RuntimeEnvironmentUpdated
			},
			sp_runtime::generic::DigestItem::AdditionalData(hash) => Self::AdditionalData(hash),
		}
	}
}

// Serialize as the same hex blob as `sp_runtime::generic::DigestItem`, which is what
// `chain_getHeader` returns.
impl Serialize for DigestItem {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		sp_runtime::generic::DigestItem::from(self.clone()).serialize(serializer)
	}
}

impl<'de> Deserialize<'de> for DigestItem {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		sp_runtime::generic::DigestItem::deserialize(deserializer).map(Self::from)
	}
}

/// Header digest of a JAM parachain block.
///
/// Mirrors `subxt::config::substrate::Digest` with [`DigestItem`] items.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, Default, Serialize, Deserialize)]
pub struct Digest {
	/// A list of digest items.
	pub logs: Vec<DigestItem>,
}

/// JAM parachain header, mirroring
/// `subxt::config::substrate::SubstrateHeader<u32, DynamicHasher256>`.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JamHeader {
	/// The parent hash.
	pub parent_hash: H256,
	/// The block number.
	#[serde(serialize_with = "serialize_number", deserialize_with = "deserialize_number")]
	#[codec(compact)]
	pub number: u32,
	/// The state trie merkle root.
	pub state_root: H256,
	/// The merkle root of the extrinsics.
	pub extrinsics_root: H256,
	/// The header digest.
	pub digest: Digest,
}

impl Header for JamHeader {
	type Number = u32;
	type Hasher = DynamicHasher256;

	fn number(&self) -> u32 {
		self.number
	}
}

/// subxt configuration for JAM parachain collators.
///
/// Identical to [`PolkadotConfig`] except for the header type, which understands the
/// `AdditionalData` digest.
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum JamConfig {}

impl Config for JamConfig {
	type AccountId = <PolkadotConfig as Config>::AccountId;
	type Address = <PolkadotConfig as Config>::Address;
	type Signature = <PolkadotConfig as Config>::Signature;
	type Hasher = <PolkadotConfig as Config>::Hasher;
	type Header = JamHeader;
	type ExtrinsicParams = PolkadotExtrinsicParams<Self>;
	type AssetId = <PolkadotConfig as Config>::AssetId;
}

fn serialize_number<S: serde::Serializer>(number: &u32, serializer: S) -> Result<S::Ok, S::Error> {
	NumberOrHex::from(*number).serialize(serializer)
}

fn deserialize_number<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
	let number = NumberOrHex::deserialize(deserializer)?;
	u32::try_from(number.into_u256())
		.map_err(|_| serde::de::Error::custom("block number does not fit into u32"))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn substrate_digest() -> sp_runtime::generic::Digest {
		sp_runtime::generic::Digest {
			logs: vec![
				sp_runtime::generic::DigestItem::PreRuntime(*b"aura", vec![1, 2, 3, 4]),
				sp_runtime::generic::DigestItem::AdditionalData([7u8; 32]),
			],
		}
	}

	#[test]
	fn digest_with_additional_data_is_byte_identical_to_substrate() {
		let substrate = substrate_digest();
		let encoded = substrate.encode();

		let decoded =
			Digest::decode(&mut &encoded[..]).expect("JAM digest must decode AdditionalData");
		assert_eq!(decoded.encode(), encoded, "re-encoding must be byte-identical");

		let roundtrip = sp_runtime::generic::Digest::decode(&mut &decoded.encode()[..])
			.expect("substrate digest must decode the JAM bytes");
		assert_eq!(roundtrip, substrate);
	}

	#[test]
	fn digest_item_roundtrips_every_variant() {
		let items = vec![
			DigestItem::PreRuntime(*b"aura", vec![1, 2, 3]),
			DigestItem::Consensus(*b"para", vec![4, 5]),
			DigestItem::Seal(*b"aura", vec![6]),
			DigestItem::Other(vec![7, 8]),
			DigestItem::RuntimeEnvironmentUpdated,
			DigestItem::AdditionalData([9u8; 32]),
		];

		for item in items {
			let encoded = item.encode();
			let substrate: sp_runtime::generic::DigestItem = item.clone().into();
			assert_eq!(encoded, substrate.encode(), "layouts must match for {item:?}");
			assert_eq!(DigestItem::decode(&mut &encoded[..]).unwrap(), item);
		}
	}

	#[test]
	fn digest_deserializes_from_chain_get_header_json() {
		let substrate = substrate_digest();
		let logs: Vec<String> = substrate
			.logs
			.iter()
			.map(|item| {
				let encoded = item.encode();
				let hex: String = encoded.iter().map(|byte| format!("{byte:02x}")).collect();
				format!("\"0x{hex}\"")
			})
			.collect();
		let json = format!(r#"{{"logs":[{}]}}"#, logs.join(","));

		let digest: Digest = serde_json::from_str(&json).expect("digest JSON");
		let expected: Vec<DigestItem> =
			substrate.logs.iter().cloned().map(DigestItem::from).collect();
		assert_eq!(digest.logs, expected);
	}

	#[test]
	fn header_deserializes_hex_number_and_additional_data_log() {
		let additional_data = format!("0x03{}", "07".repeat(32));
		let json = format!(
			r#"{{
				"parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"number": "0x2a",
				"stateRoot": "0x0101010101010101010101010101010101010101010101010101010101010101",
				"extrinsicsRoot": "0x0000000000000000000000000000000000000000000000000000000000000000",
				"digest": {{ "logs": ["{additional_data}"] }}
			}}"#
		);

		let header: JamHeader = serde_json::from_str(&json).expect("header JSON");
		assert_eq!(header.number, 42);
		assert_eq!(header.digest.logs, vec![DigestItem::AdditionalData([7u8; 32])]);

		let encoded = header.encode();
		assert_eq!(JamHeader::decode(&mut &encoded[..]).unwrap(), header);
	}
}
