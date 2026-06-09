// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0
//
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

//! Synthesize Ethereum-shaped artifacts (ERC-20 `Transfer` logs + a stand-in
//! transaction/receipt) from `pallet-assets` `Transferred` events, so that asset
//! transfers issued by plain Substrate extrinsics (not just `eth_transact` calls)
//! are visible through the eth-rpc (`eth_getTransactionReceipt`, `eth_getLogs`,
//! `eth_getBlock*`). See contract-issues#61.
//!
//! Native balance (`pallet-balances`) is intentionally NOT covered here: it has no
//! ERC-20 precompile address on Asset Hub, so there is no real callable token
//! contract to attribute its transfers to. It is a fast-follow blocked on a
//! native-balances precompile.

use alloy_core::sol_types::{SolCall, SolEvent};
use codec::Decode;
use ethereum_standards::IERC20;
use pallet_revive::evm::{
	Bytes, H256, Log, TransactionLegacySigned, TransactionLegacyUnsigned, TransactionSigned, U256,
};
use sp_core::{H160, crypto::AccountId32};
use sp_crypto_hashing::{blake2_128, keccak_256, twox_128};

/// topic0 of an ERC-20 `Transfer` — sourced from the same `IERC20` ABI the assets precompile
/// uses to emit the event, so the two cannot drift.
pub const ERC20_TRANSFER_TOPIC: H256 = H256(IERC20::Transfer::SIGNATURE_HASH.0);

/// The ERC-20 `transfer(address,uint256)` selector, from the same `IERC20` ABI.
const ERC20_TRANSFER_SELECTOR: [u8; 4] = IERC20::transferCall::SELECTOR;

/// Storage location of the foreign-asset `Location -> u32 index` map, in the
/// `pallet-assets-precompiles` index pallet. Fixed across runtimes that wire that pallet.
const FOREIGN_INDEX_PALLET: &str = "AssetsPrecompiles";
const FOREIGN_INDEX_ENTRY: &str = "ForeignAssetIdToAssetIndex";

/// Maps `pallet-assets` instances (by metadata pallet name) to their precompile address prefix.
///
/// Prefixes are a per-runtime choice, not a protocol constant; defaults target Asset Hub and
/// are overridable for chains that pick different ones.
///
/// `u32_instances`: `AssetId` is a `u32`, so the address is computed statelessly.
/// `foreign_instances`: `AssetId` is an XCM `Location`; the event carries the `Location`, not
/// the `u32` index the address needs, so the index is looked up from `FOREIGN_INDEX_ENTRY`
/// (see [`foreign_index_storage_key`]).
#[derive(Clone)]
pub struct AssetTransferConfig {
	/// `(pallet name, address prefix)` for instances whose `AssetId` is a `u32`.
	pub u32_instances: Vec<(&'static str, u16)>,
	/// `(pallet name, address prefix)` for instances whose `AssetId` is a `Location`.
	pub foreign_instances: Vec<(&'static str, u16)>,
}

impl Default for AssetTransferConfig {
	fn default() -> Self {
		Self {
			u32_instances: vec![("Assets", 0x0120), ("PoolAssets", 0x0320)],
			foreign_instances: vec![("ForeignAssets", 0x0220)],
		}
	}
}

impl AssetTransferConfig {
	/// The address prefix for a pallet, if it is a configured u32-id assets instance.
	fn prefix_for(&self, pallet_name: &str) -> Option<u16> {
		self.u32_instances
			.iter()
			.find(|(p, _)| *p == pallet_name)
			.map(|(_, prefix)| *prefix)
	}

	/// The address prefix for a pallet, if it is a configured foreign-assets instance.
	fn foreign_prefix_for(&self, pallet_name: &str) -> Option<u16> {
		self.foreign_instances
			.iter()
			.find(|(p, _)| *p == pallet_name)
			.map(|(_, prefix)| *prefix)
	}
}

/// Decoded shape of `pallet_assets::Event::Transferred` for a `u32`-id instance.
/// Field order mirrors `frame/assets/src/lib.rs` (`asset_id, from, to, amount`).
#[derive(Decode)]
struct AssetsTransferred {
	asset_id: u32,
	from: AccountId32,
	to: AccountId32,
	amount: u128,
}

/// The deterministic precompile/token address for a `u32` asset id under `prefix`.
/// Mirrors `pallet_assets_precompiles::InlineIdConfig` exactly.
pub fn asset_token_address(asset_id: u32, prefix: u16) -> H160 {
	let mut addr = [0u8; 20];
	addr[0..4].copy_from_slice(&asset_id.to_be_bytes());
	addr[16..18].copy_from_slice(&prefix.to_be_bytes());
	H160(addr)
}

/// Left-pad a 20-byte address into a 32-byte EVM word (for indexed-address topics).
fn address_topic(addr: H160) -> H256 {
	let mut word = [0u8; 32];
	word[12..].copy_from_slice(addr.as_bytes());
	H256(word)
}

/// Encode a `u128` as a 32-byte big-endian EVM word.
fn u128_be32(v: u128) -> [u8; 32] {
	let mut word = [0u8; 32];
	word[16..].copy_from_slice(&v.to_be_bytes());
	word
}

/// A single asset transfer extracted from a block event, mapped into EVM terms.
pub struct AssetTransfer {
	pub token: H160,
	pub from: H160,
	pub to: H160,
	pub amount: u128,
}

impl AssetTransfer {
	/// Build the canonical ERC-20 `Transfer` log for this transfer.
	pub fn to_log(
		&self,
		block_number: U256,
		block_hash: H256,
		transaction_hash: H256,
		transaction_index: usize,
		log_index: u32,
	) -> Log {
		Log {
			address: self.token,
			topics: vec![ERC20_TRANSFER_TOPIC, address_topic(self.from), address_topic(self.to)],
			data: Some(Bytes(u128_be32(self.amount).to_vec())),
			block_number,
			block_hash,
			transaction_hash,
			transaction_index: transaction_index.into(),
			log_index: log_index.into(),
			..Default::default()
		}
	}

	/// The ERC-20 `transfer(address,uint256)` calldata, used as the `input` of the
	/// stand-in transaction so `eth_getTransactionByHash` shows a meaningful payload.
	fn erc20_calldata(&self) -> Vec<u8> {
		let mut data = Vec::with_capacity(4 + 32 + 32);
		data.extend_from_slice(&ERC20_TRANSFER_SELECTOR);
		data.extend_from_slice(address_topic(self.to).as_bytes());
		data.extend_from_slice(&u128_be32(self.amount));
		data
	}
}

/// Build the ERC-20 `Transfer` logs for every transfer in one extrinsic, each paired with its
/// block-wide event index (used as the log's `log_index`).
pub fn asset_transfer_logs(
	transfers: &[(AssetTransfer, u32)],
	block_number: U256,
	block_hash: H256,
	transaction_hash: H256,
	transaction_index: usize,
) -> Vec<Log> {
	transfers
		.iter()
		.map(|(transfer, log_index)| {
			transfer.to_log(
				block_number,
				block_hash,
				transaction_hash,
				transaction_index,
				*log_index,
			)
		})
		.collect()
}

/// Try to decode an asset-transfer from a raw event (pallet name, variant name, SCALE
/// field bytes). Returns `None` for anything that is not a configured assets `Transferred`.
///
/// Decoding is dynamic (by name + `Decode` of the field bytes) rather than via generated
/// static types, because the eth-rpc's subxt metadata is generated from the dev runtime,
/// which does not include `pallet-assets`.
pub fn decode_asset_transfer(
	config: &AssetTransferConfig,
	pallet_name: &str,
	variant_name: &str,
	mut field_bytes: &[u8],
) -> Option<AssetTransfer> {
	if variant_name != "Transferred" {
		return None;
	}
	let prefix = config.prefix_for(pallet_name)?;
	let ev = AssetsTransferred::decode(&mut field_bytes).ok()?;
	Some(AssetTransfer {
		token: asset_token_address(ev.asset_id, prefix),
		from: account_to_h160(&ev.from),
		to: account_to_h160(&ev.to),
		amount: ev.amount,
	})
}

/// The parts of a foreign-asset `Transferred` event needed to build an [`AssetTransfer`],
/// once the `Location -> index` lookup has resolved the precompile address.
pub struct ForeignTransferParts {
	/// SCALE-encoded `Location` (the `asset_id` field), used as the storage-map key.
	pub asset_id_key: Vec<u8>,
	pub from: H160,
	pub to: H160,
	pub amount: u128,
}

/// Decode a foreign-asset `Transferred` event without knowing the `Location` type.
///
/// The event's SCALE fields are `[asset_id: Location][from: [u8;32]][to: [u8;32]][amount: u128]`.
/// `from`/`to`/`amount` are fixed-size (32 + 32 + 16 = 80 trailing bytes), so we split those
/// off the end; the remaining prefix is the encoded `Location`, used verbatim as the
/// `ForeignAssetIdToAssetIndex` map key. Returns the instance's address prefix and the parts,
/// or `None` if the pallet is not a configured foreign instance, the variant is not
/// `Transferred`, or the bytes are too short.
pub fn decode_foreign_transfer_parts(
	config: &AssetTransferConfig,
	pallet_name: &str,
	variant_name: &str,
	field_bytes: &[u8],
) -> Option<(u16, ForeignTransferParts)> {
	if variant_name != "Transferred" {
		return None;
	}
	let prefix = config.foreign_prefix_for(pallet_name)?;
	if field_bytes.len() < 80 {
		return None;
	}
	let split = field_bytes.len() - 80;
	let asset_id_key = field_bytes[..split].to_vec();
	let from = account_h160_from_slice(&field_bytes[split..split + 32]);
	let to = account_h160_from_slice(&field_bytes[split + 32..split + 64]);
	let amount = {
		let mut a = [0u8; 16];
		a.copy_from_slice(&field_bytes[split + 64..]);
		u128::from_le_bytes(a)
	};
	Some((prefix, ForeignTransferParts { asset_id_key, from, to, amount }))
}

/// Build the full storage key for the `Blake2_128Concat`
/// `FOREIGN_INDEX_PALLET::FOREIGN_INDEX_ENTRY` map, keyed by the SCALE-encoded asset `Location`.
/// The eth-rpc reads this raw key via `fetch_raw` to resolve a foreign asset's `u32` index.
pub fn foreign_index_storage_key(asset_id_key: &[u8]) -> Vec<u8> {
	let mut key = Vec::with_capacity(16 + 16 + 16 + asset_id_key.len());
	key.extend_from_slice(&twox_128(FOREIGN_INDEX_PALLET.as_bytes()));
	key.extend_from_slice(&twox_128(FOREIGN_INDEX_ENTRY.as_bytes()));
	key.extend_from_slice(&blake2_128(asset_id_key));
	key.extend_from_slice(asset_id_key);
	key
}

/// Decode the `u32` asset index from a raw `ForeignAssetIdToAssetIndex` storage value.
pub fn decode_foreign_index(mut raw: &[u8]) -> Option<u32> {
	u32::decode(&mut raw).ok()
}

/// Build an [`AssetTransfer`] for a foreign asset, given its address prefix and resolved index.
pub fn foreign_asset_transfer(
	prefix: u16,
	parts: ForeignTransferParts,
	asset_index: u32,
) -> AssetTransfer {
	AssetTransfer {
		token: asset_token_address(asset_index, prefix),
		from: parts.from,
		to: parts.to,
		amount: parts.amount,
	}
}

/// `pallet_revive::AccountId32Mapper::to_address` reimplemented over a bare `AccountId32`
/// (the eth-rpc has only the account bytes from the event, not runtime access). Total &
/// stateless, matching `frame/revive/src/address.rs`.
fn account_to_h160(account: &AccountId32) -> H160 {
	let bytes: &[u8; 32] = account.as_ref();
	if bytes[20..] == [0xEE; 12] {
		// eth-derived: strip the 0xEE suffix.
		H160::from_slice(&bytes[..20])
	} else {
		// (ed|sr)25519-derived: hash to avoid truncating the public key.
		let hash = keccak_256(bytes);
		H160::from_slice(&hash[12..])
	}
}

/// [`account_to_h160`] for a 32-byte account slice. Panics if `bytes.len() != 32`.
fn account_h160_from_slice(bytes: &[u8]) -> H160 {
	let mut id = [0u8; 32];
	id.copy_from_slice(bytes);
	account_to_h160(&AccountId32::new(id))
}

/// Build a stand-in legacy transaction for an asset-transfer extrinsic, so the transaction RPC
/// surface (`eth_getTransactionByHash`, full-tx `eth_getBlock*`) returns a coherent object.
pub fn synthetic_transaction(transfer: &AssetTransfer) -> TransactionSigned {
	let unsigned = TransactionLegacyUnsigned {
		to: Some(transfer.token),
		value: U256::zero(),
		input: Bytes(transfer.erc20_calldata()),
		..Default::default()
	};
	// Synthetic tx, never signed; `from` is served from the receipt, so the signature is unused.
	TransactionSigned::TransactionLegacySigned(TransactionLegacySigned {
		transaction_legacy_unsigned: unsigned,
		r: U256::zero(),
		s: U256::zero(),
		v: U256::zero(),
	})
}

/// Deterministic, block-unique transaction hash for a synthetic asset-transfer "tx".
/// Reproducible from the block alone (extraction runs at both index- and query-time):
/// `keccak256(block_hash ++ extrinsic_index_be)`.
pub fn synthetic_tx_hash(block_hash: H256, extrinsic_index: usize) -> H256 {
	let mut buf = Vec::with_capacity(32 + 4);
	buf.extend_from_slice(block_hash.as_bytes());
	buf.extend_from_slice(&(extrinsic_index as u32).to_be_bytes());
	H256(keccak_256(&buf))
}

/// Decode the signer (origin) `AccountId32` from a subxt extrinsic `address_bytes()`.
/// The bytes are a SCALE-encoded `MultiAddress`; we only handle the `Id` (0x00) variant,
/// which is what signed transfer extrinsics use.
pub fn signer_h160_from_address_bytes(address_bytes: Option<&[u8]>) -> Option<H160> {
	let bytes = address_bytes?;
	// MultiAddress::Id == variant 0, followed by 32 account bytes.
	if bytes.len() == 33 && bytes[0] == 0x00 {
		Some(account_h160_from_slice(&bytes[1..]))
	} else if bytes.len() == 32 {
		// Some configs encode the bare AccountId32.
		Some(account_h160_from_slice(bytes))
	} else {
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;

	#[test]
	fn asset_address_matches_precompile_scheme() {
		// Vector from pallet_assets_precompiles tests: asset 1337 (0x539), prefix 0x0120.
		let addr = asset_token_address(1337, 0x0120);
		assert_eq!(
			addr,
			H160([0, 0, 0x05, 0x39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0x20, 0, 0]),
		);
		// Pool assets prefix.
		let pool = asset_token_address(1, 0x0320);
		assert_eq!(
			pool,
			H160([0, 0, 0, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x03, 0x20, 0, 0]),
		);
	}

	#[test]
	fn log_encodes_indexed_addresses_and_amount() {
		let t = AssetTransfer {
			token: H160::from([0x11; 20]),
			from: H160::from([0x22; 20]),
			to: H160::from([0x33; 20]),
			amount: 1_000_000u128,
		};
		let log = t.to_log(U256::from(7), H256::from([0xab; 32]), H256::from([0xcd; 32]), 4, 9);
		assert_eq!(log.address, t.token);
		assert_eq!(log.topics.len(), 3);
		assert_eq!(log.topics[0], ERC20_TRANSFER_TOPIC);
		assert_eq!(&log.topics[1].as_bytes()[12..], &[0x22; 20]);
		assert_eq!(&log.topics[2].as_bytes()[12..], &[0x33; 20]);
		let data = log.data.unwrap().0;
		assert_eq!(data.len(), 32);
		assert_eq!(U256::from_big_endian(&data), U256::from(1_000_000u64));
		assert_eq!(log.transaction_index, U256::from(4));
		assert_eq!(log.log_index, U256::from(9));
	}

	#[test]
	fn decodes_assets_transferred_by_name() {
		let from = AccountId32::new([0xAA; 32]);
		let to = AccountId32::new([0xBB; 32]);
		// Build SCALE field bytes in event field order: asset_id, from, to, amount.
		let mut bytes = Vec::new();
		1337u32.encode_to(&mut bytes);
		from.encode_to(&mut bytes);
		to.encode_to(&mut bytes);
		42u128.encode_to(&mut bytes);

		let cfg = AssetTransferConfig::default();
		let t = decode_asset_transfer(&cfg, "Assets", "Transferred", &bytes).expect("decodes");
		assert_eq!(t.token, asset_token_address(1337, 0x0120));
		assert_eq!(t.amount, 42u128);
		assert_eq!(t.from, account_to_h160(&from));

		// Wrong variant / unconfigured pallet → None.
		assert!(decode_asset_transfer(&cfg, "Assets", "Issued", &bytes).is_none());
		assert!(decode_asset_transfer(&cfg, "ForeignAssets", "Transferred", &bytes).is_none());
	}

	#[test]
	fn foreign_transfer_splits_trailing_fixed_fields() {
		let from = AccountId32::new([0xAA; 32]);
		let to = AccountId32::new([0xBB; 32]);
		// asset_id is a variable-length Location; use arbitrary leading bytes.
		let location = vec![0x01, 0x02, 0x00, 0xCA, 0xFE];
		let mut bytes = Vec::new();
		bytes.extend_from_slice(&location);
		from.encode_to(&mut bytes);
		to.encode_to(&mut bytes);
		7u128.encode_to(&mut bytes);

		let cfg = AssetTransferConfig::default();
		let (prefix, parts) =
			decode_foreign_transfer_parts(&cfg, "ForeignAssets", "Transferred", &bytes)
				.expect("decodes");
		assert_eq!(prefix, 0x0220);
		assert_eq!(parts.asset_id_key, location);
		assert_eq!(parts.amount, 7u128);
		assert_eq!(parts.from, account_to_h160(&from));
		assert_eq!(parts.to, account_to_h160(&to));

		// Resolved index → address uses the foreign prefix.
		let t = foreign_asset_transfer(prefix, parts, 9);
		assert_eq!(t.token, asset_token_address(9, 0x0220));

		// Non-foreign pallet / wrong variant / too-short → None.
		assert!(decode_foreign_transfer_parts(&cfg, "Assets", "Transferred", &bytes).is_none());
		assert!(decode_foreign_transfer_parts(&cfg, "ForeignAssets", "Issued", &bytes).is_none());
		assert!(
			decode_foreign_transfer_parts(&cfg, "ForeignAssets", "Transferred", &[0u8; 10])
				.is_none()
		);
	}

	#[test]
	fn foreign_index_storage_key_layout() {
		let loc = vec![0xDE, 0xAD, 0xBE, 0xEF];
		let key = foreign_index_storage_key(&loc);
		// twox128(pallet) ++ twox128(entry) ++ blake2_128(loc) ++ loc
		assert_eq!(key.len(), 16 + 16 + 16 + loc.len());
		assert_eq!(&key[..16], &twox_128(b"AssetsPrecompiles"));
		assert_eq!(&key[16..32], &twox_128(b"ForeignAssetIdToAssetIndex"));
		assert_eq!(&key[32..48], &blake2_128(&loc));
		assert_eq!(&key[48..], &loc[..]);
	}

	#[test]
	fn decodes_foreign_index_u32() {
		let raw = 12345u32.encode();
		assert_eq!(decode_foreign_index(&raw), Some(12345));
	}

	#[test]
	fn eth_derived_account_strips_suffix() {
		let mut raw = [0u8; 32];
		raw[..20].copy_from_slice(&[0x42; 20]);
		raw[20..].copy_from_slice(&[0xEE; 12]);
		assert_eq!(account_to_h160(&AccountId32::new(raw)), H160::from([0x42; 20]));
	}

	#[test]
	fn synthetic_tx_hash_is_deterministic_and_index_unique() {
		let bh = H256::from([0x01; 32]);
		assert_eq!(synthetic_tx_hash(bh, 3), synthetic_tx_hash(bh, 3));
		assert_ne!(synthetic_tx_hash(bh, 3), synthetic_tx_hash(bh, 4));
	}

	#[test]
	fn synthetic_tx_carries_erc20_calldata() {
		let t = AssetTransfer {
			token: H160::from([0x11; 20]),
			from: H160::from([0x22; 20]),
			to: H160::from([0x33; 20]),
			amount: 5u128,
		};
		let TransactionSigned::TransactionLegacySigned(tx) = synthetic_transaction(&t) else {
			panic!("expected legacy")
		};
		let input = tx.transaction_legacy_unsigned.input.0;
		assert_eq!(&input[..4], &ERC20_TRANSFER_SELECTOR);
		assert_eq!(&input[4 + 12..4 + 32], &[0x33; 20]); // to, left-padded
		assert_eq!(U256::from_big_endian(&input[36..68]), U256::from(5u64));
		assert_eq!(tx.transaction_legacy_unsigned.to, Some(t.token));
	}
}
