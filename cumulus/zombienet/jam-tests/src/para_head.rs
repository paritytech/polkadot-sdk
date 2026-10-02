// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The parachain head JAM has accumulated, read straight off a JAM node's RPC.
//!
//! This is the completion signal of the whole pipeline: JAM emits no "accumulated" event, so a
//! para head that moves is the only proof that a work package was guaranteed, reported and
//! accumulated. The read is the collator's own — `serviceValue` at the best block, under the key
//! the parachain service files a para's [`ParaInfo`] at — and yields the head's header hash; the
//! para's collator resolves it to a height.

use crate::rpc::{CollatorRpc, JamRpc};
use anyhow::Context;
use codec::DecodeAll;
use parachain_service_core::{para_info_key, ParaInfo};
use sp_core::H256;
use std::time::Duration;
use tokio::time::{sleep, Instant};

/// A parachain head as JAM has accumulated it: the tip the service believes the chain has reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParaHead {
	/// The block hash, `0x`-prefixed, in the same spelling a collator's RPC uses.
	pub hash: String,
	/// The block's height as the para's collator knows it; `None` while it does not hold it.
	pub number: Option<u64>,
}

impl std::fmt::Display for ParaHead {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self.number {
			Some(number) => write!(formatter, "#{number} {}", self.hash),
			None => write!(formatter, "#? {}", self.hash),
		}
	}
}

/// The hash of the parachain head the service has accumulated for `para`, or `None` while it has
/// none.
///
/// The read is the collator's own: `serviceValue` at the best block, under the key the parachain
/// service files a para's [`ParaInfo`] at. A value that does not decode is an error rather than
/// "no head", because the two mean opposite things to a stall assertion.
pub async fn read_para_head_hash(
	jam: &JamRpc,
	service_id: u32,
	para: u32,
) -> anyhow::Result<Option<String>> {
	let key = para_info_key(para.into());
	let at = jam.best_block_hash().await?;

	let started = std::time::Instant::now();
	let stored = jam.service_value(&at, service_id, &key).await?;
	let elapsed = started.elapsed();

	let hash = stored
		.as_deref()
		.map(decode_para_head_hash)
		.transpose()
		.with_context(|| format!("para {para}'s entry at block {at}"))?;
	log::info!(
		"serviceValue(service {}, para {para}) at block {at}: {} in {elapsed:?}",
		service_id,
		match &hash {
			Some(hash) => format!("head hash {hash}"),
			None => "no entry".to_string(),
		},
	);
	Ok(hash)
}

/// The parachain head the service has accumulated for `para`, with the height `collator` knows
/// it at: JAM stores only the header hash.
pub async fn read_para_head(
	jam: &JamRpc,
	collator: &CollatorRpc,
	service_id: u32,
	para: u32,
) -> anyhow::Result<Option<ParaHead>> {
	let Some(hash) = read_para_head_hash(jam, service_id, para).await? else { return Ok(None) };
	let number = collator.height_of(&hash).await?;
	let head = ParaHead { hash, number };
	log::info!("para {para}'s accumulated head {head}");
	Ok(Some(head))
}

/// Gap between accumulated-head polls; the head cannot advance more than once per JAM slot.
const HEAD_POLL: Duration = Duration::from_secs(3);

/// Wait until JAM has accumulated a head of at least `target` for `para`.
///
/// The read is [`read_para_head`]: the head hash in the service's [`ParaInfo`], resolved to a
/// height by the para's collator. A head the collator does not know yet is not there yet. The
/// collator's own best-block metric is not a faithful stand-in: it holds its slot while a package
/// is overdue, so its height trails the accumulated head and would let the assertion below fail
/// on a healthy run.
pub async fn wait_for_jam_head(
	jam: &JamRpc,
	collator: &CollatorRpc,
	service_id: u32,
	para: u32,
	target: u64,
	budget: Duration,
) -> anyhow::Result<()> {
	let deadline = Instant::now() + budget;
	let mut last = None;
	loop {
		if let Some(head) = read_para_head(jam, collator, service_id, para).await? {
			log::info!("JAM accumulated head for para {para}: {head}");
			if head.number.is_some_and(|number| number >= target) {
				return Ok(());
			}
			last = Some(head);
		}
		anyhow::ensure!(
			Instant::now() < deadline,
			"JAM did not accumulate head #{target} for para {para} within {budget:?}; the last \
			 accumulated head was {last:?}"
		);
		sleep(HEAD_POLL).await;
	}
}

/// Wait until JAM's accumulated head for `para` has not changed for `still_for`.
///
/// Standing still is what a stall looks like from the chain: nothing announces that packages
/// stopped being reported, the head simply stops moving. The core tests use this to confirm a
/// parked core really stopped carrying the para's work; it compares the head hashes
/// [`read_para_head_hash`] reads.
///
/// No head at all (`None`) is unchanged for as long as it stays absent — the caller decides
/// whether that counts as a stall.
pub async fn wait_for_frozen_jam_head(
	jam: &JamRpc,
	service_id: u32,
	para: u32,
	still_for: Duration,
	budget: Duration,
) -> anyhow::Result<()> {
	let deadline = Instant::now() + budget;
	// The last head and the instant it was first seen. Reset the instant whenever the head
	// changes, so the elapsed time measured is only ever time spent on the current head.
	let mut frozen: Option<(Option<String>, Instant)> = None;
	loop {
		let head = read_para_head_hash(jam, service_id, para).await?;
		match &frozen {
			Some((last, since)) if *last == head && since.elapsed() >= still_for => {
				log::info!(
					"JAM's accumulated head for para {para} has stood still at {head:?} for \
					 {still_for:?}"
				);
				return Ok(());
			},
			Some((last, _)) if *last == head => {},
			_ => frozen = Some((head.clone(), Instant::now())),
		}
		anyhow::ensure!(
			Instant::now() < deadline,
			"JAM's accumulated head for para {para} did not stand still for {still_for:?} within \
			 {budget:?}; the last accumulated head was {head:?}"
		);
		sleep(HEAD_POLL).await;
	}
}

/// Read the accumulated head's hash out of a para's stored [`ParaInfo`].
///
/// `head_data` is the header hash of the para's last included block. A value that does not decode
/// is an error rather than "no head": the two mean opposite things to a stall assertion, so a
/// layout that has moved on has to say so loudly.
pub fn decode_para_head_hash(stored: &[u8]) -> anyhow::Result<String> {
	let info = ParaInfo::decode_all(&mut &stored[..])
		.with_context(|| format!("decoding {} bytes as the service's ParaInfo", stored.len()))?;
	let head = info.head_data.into_inner();
	let hash = H256::decode_all(&mut &head[..]).with_context(|| {
		format!("decoding ParaInfo's {} bytes of head_data as a 32-byte header hash", head.len())
	})?;
	Ok(array_bytes::bytes2hex("0x", hash))
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use sp_runtime::traits::BlakeTwo256;

	/// The parachain header type, which is the parachain template runtime's `Header`.
	type ParaHeader = sp_runtime::generic::Header<u32, BlakeTwo256>;

	/// A header of the kind a collator authors.
	fn header(number: u32) -> ParaHeader {
		ParaHeader {
			parent_hash: H256::repeat_byte(0xaa),
			number,
			state_root: H256::repeat_byte(0xbb),
			extrinsics_root: H256::repeat_byte(0xcc),
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

	/// The collator's RPC is handed this string verbatim, and substrate reads a block hash as
	/// `0x` and 32 bytes of hex.
	#[test]
	fn the_accumulated_head_is_the_hash_in_para_infos_head_data() {
		let hash = header(17).hash();
		let head = decode_para_head_hash(&stored_entry(hash.as_bytes().to_vec()))
			.expect("the test entry is a ParaInfo holding a hash; qed");
		assert_eq!(head, array_bytes::bytes2hex("0x", hash));
		assert_eq!(head.len(), 2 + 64);
	}

	#[test]
	fn a_full_header_in_head_data_is_an_error() {
		assert!(decode_para_head_hash(&stored_entry(header(17).encode())).is_err());
	}

	#[test]
	fn a_head_that_is_not_32_bytes_is_an_error() {
		assert!(decode_para_head_hash(&stored_entry(vec![0xff; 8])).is_err());
	}

	#[test]
	fn an_entry_that_is_not_a_para_info_is_an_error() {
		assert!(decode_para_head_hash(&[0xff; 8]).is_err());
	}
}
