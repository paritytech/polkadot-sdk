// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The parachain head JAM has accumulated, read straight off a JAM node's RPC.
//!
//! This is the completion signal of the whole pipeline: JAM emits no "accumulated" event, so a
//! para head that moves is the only proof that a work package was guaranteed, reported and
//! accumulated. The read is the collator's own — `serviceValue` at the best block, under the key
//! the parachain service files a para's [`ParaInfo`] at.

use crate::rpc::JamRpc;
use anyhow::Context;
use codec::DecodeAll;
use parachain_service_core::{para_info_key, ParaInfo};
use sp_runtime::traits::BlakeTwo256;

/// A parachain head as JAM has accumulated it: the tip the service believes the chain has reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParaHead {
	pub number: u64,
	/// The block hash, `0x`-prefixed, in the same spelling a collator's RPC uses.
	pub hash: String,
}

impl std::fmt::Display for ParaHead {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(formatter, "#{} {}", self.number, self.hash)
	}
}

/// The parachain header type, which is the parachain template runtime's `Header`.
type ParaHeader = sp_runtime::generic::Header<u32, BlakeTwo256>;

/// The parachain head the service has accumulated for `para`, or `None` while it has none.
///
/// The read is the collator's own: `serviceValue` at the best block, under the key the parachain
/// service files a para's [`ParaInfo`] at. A value that does not decode is an error rather than
/// "no head", because the two mean opposite things to a stall assertion.
pub async fn read_para_head(
	jam: &JamRpc,
	service_id: u32,
	para: u32,
) -> anyhow::Result<Option<ParaHead>> {
	let key = para_info_key(para.into());
	let at = jam.best_block_hash().await?;

	let started = std::time::Instant::now();
	let stored = jam.service_value(&at, service_id, &key).await?;
	let elapsed = started.elapsed();

	let head = stored
		.as_deref()
		.map(decode_para_head)
		.transpose()
		.with_context(|| format!("para {para}'s entry at block {at}"))?;
	log::info!(
		"serviceValue(service {}, para {para}) at block {at}: {} in {elapsed:?}",
		service_id,
		match &head {
			Some(head) => format!("head {head}"),
			None => "no entry".to_string(),
		},
	);
	Ok(head)
}

/// Read the accumulated head out of a para's stored [`ParaInfo`].
///
/// Both decodes go through the real types — the service's own `ParaInfo`, and the header type the
/// runtime the collators run defines — so no layout is spelled out here to drift out of step with
/// either. A value that does not decode is an error rather than "no head": the two mean opposite
/// things to a stall assertion, so a layout that has moved on has to say so loudly.
fn decode_para_head(stored: &[u8]) -> anyhow::Result<ParaHead> {
	let info = ParaInfo::decode_all(&mut &stored[..])
		.with_context(|| format!("decoding {} bytes as the service's ParaInfo", stored.len()))?;
	let head = info.head_data.into_inner();
	let header = ParaHeader::decode_all(&mut &head[..]).with_context(|| {
		format!("decoding ParaInfo's {} bytes of head_data as a substrate header", head.len())
	})?;

	Ok(ParaHead { number: header.number.into(), hash: array_bytes::bytes2hex("0x", header.hash()) })
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;

	/// A header of the kind a collator files as its para head.
	fn header(number: u32) -> ParaHeader {
		ParaHeader {
			parent_hash: sp_core::H256::repeat_byte(0xaa),
			number,
			state_root: sp_core::H256::repeat_byte(0xbb),
			extrinsics_root: sp_core::H256::repeat_byte(0xcc),
			digest: Default::default(),
		}
	}

	/// The bytes the parachain service files under a para's key, given its `head_data`.
	fn stored_entry(head_data: Vec<u8>) -> Vec<u8> {
		ParaInfo {
			head_data: head_data.try_into().expect("the head fits in HeadData; qed"),
			validation_code: None,
			announced_upgrade: None,
			total_state_balance: 0,
			used_state_balance: 0,
			is_deregistering: false,
		}
		.encode()
	}

	/// The head arrives wrapped in `ParaInfo`, so the two decodes have to compose. Both fields are
	/// asserted because a header read at the wrong offset would still yield *some* number and
	/// *some* hash, and every phase assertion in this suite is a comparison of those.
	#[test]
	fn the_accumulated_head_is_the_header_in_para_infos_head_data() {
		let header = header(17);

		let head = decode_para_head(&stored_entry(header.encode())).expect("the entry decodes");

		assert_eq!(head.number, 17);
		// The collator's RPC is handed this string verbatim, and substrate reads a block hash as
		// `0x` and 32 bytes of hex.
		assert_eq!(head.hash, array_bytes::bytes2hex("0x", header.hash()));
		assert_eq!(head.hash.len(), 2 + 64);
	}

	#[test]
	fn a_head_of_zero_is_a_real_head() {
		// Height zero is a real head — the genesis one — so "nothing accumulated yet" has to come
		// from the para having no entry at all, never from its number, or a stall would read as
		// progress.
		let stored = stored_entry(header(0).encode());
		assert_eq!(decode_para_head(&stored).expect("the entry decodes").number, 0);
	}

	#[test]
	fn an_entry_that_is_not_a_para_info_is_an_error() {
		assert!(decode_para_head(&[0xff; 8]).is_err());
	}

	#[test]
	fn a_head_that_is_not_a_substrate_header_is_an_error() {
		assert!(decode_para_head(&stored_entry(vec![0xff; 8])).is_err());
	}
}
