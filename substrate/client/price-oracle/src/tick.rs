// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! One tick of the oracle: what it reads from the runtime, the rules it sets for incoming
//! reports, and the report it produces.

use crate::{
	fetcher::{Fetcher, MarketResponses},
	pool::ReportPool,
	signer,
};
use codec::{Decode, DecodeAll, Encode};
use futures::{channel::mpsc, StreamExt};
use sc_network::{PeerId, ReputationChange};
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppCrypto;
use sp_blockchain::HeaderBackend;
use sp_keystore::KeystorePtr;
use sp_price_oracle::{
	market::Market, runtime_api::PriceOracleApi, Anchor, PriceReport, Settings, SignedPriceReport,
};
use sp_runtime::{
	traits::{Block as BlockT, SaturatedConversion},
	RuntimeAppPublic,
};
use std::{
	sync::Arc,
	time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::Instant;

const LOG_TARGET: &str = "price-oracle";
/// Lower bound on the tick interval. A shorter interval configured in the runtime is raised to
/// this value.
pub const MIN_TICK_INTERVAL: Duration = Duration::from_secs(1);
/// Part of the tick interval reserved for aggregating and signing. Fetching stops this long
/// before the tick interval elapses.
const TICK_MARGIN: Duration = Duration::from_millis(200);

/// Reputation changes applied to peers for the reports they send.
pub(crate) mod cost {
	use sc_network::ReputationChange as Rep;
	pub(crate) const MALFORMED: Rep = Rep::new(-500, "Price oracle: undecodable report");
	pub(crate) const STALE_REPORT: Rep = Rep::new(-50, "Price oracle: stale report");
	pub(crate) const UNKNOWN_SIGNER: Rep = Rep::new(-150, "Price oracle: unknown signer");
	pub(crate) const BAD_SIGNATURE: Rep = Rep::new(-100, "Price oracle: bad signature");
}

pub(crate) mod benefit {
	use sc_network::ReputationChange as Rep;
	pub(crate) const GOOD_REPORT: Rep = Rep::new(100, "Price oracle: valid report");
}

/// The state a tick starts from, read from the runtime at the best block.
#[derive(Clone)]
pub struct TickSetup<Hash, Id> {
	/// The best block the state was read at.
	pub best_hash: Hash,
	/// The anchor of a report signed at this tick: the number of `best_hash`.
	anchor: Anchor,
	/// The signers, the report window and the tick interval in force.
	settings: Settings<Id>,
	/// The local signer key, if the keystore holds one of the accepted signers.
	signer: Option<Id>,
	/// The active markets to fetch and price.
	markets: Vec<Market>,
}

impl<Hash: Copy, Id: Clone> TickSetup<Hash, Id> {
	/// Read the state at the best block.
	///
	/// Returns `None` if a runtime API call fails, which is the case when the runtime does not
	/// implement the price oracle API. The failure is logged once, and again only after the
	/// calls have succeeded in between.
	pub fn read<Block, Client>(
		client: &Client,
		keystore: &KeystorePtr,
		api_missing_logged: &mut bool,
	) -> Option<Self>
	where
		Block: BlockT<Hash = Hash>,
		Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
		Client::Api: PriceOracleApi<Block, Id>,
		Id: RuntimeAppPublic + AppCrypto + Ord + Clone + Decode,
	{
		let info = client.info();
		let best_hash = info.best_hash;
		let anchor = Anchor(info.best_number.saturated_into());
		let api = client.runtime_api();

		let (settings, markets) = match (api.settings(best_hash), api.markets(best_hash)) {
			(Ok(settings), Ok(markets)) => (settings, markets),
			(Err(e), _) | (_, Err(e)) => {
				if !*api_missing_logged {
					log::error!(target: LOG_TARGET, "Runtime does not serve the price oracle API: {e}");
					*api_missing_logged = true;
				}
				return None;
			},
		};
		*api_missing_logged = false;

		let signer = signer::local_signer(keystore, &settings.signers);
		Some(Self { best_hash, anchor, settings, signer, markets })
	}

	/// The tick interval in force, raised to [`MIN_TICK_INTERVAL`] if shorter.
	pub fn interval(&self) -> Duration {
		Duration::from_millis(self.settings.tick_interval_ms.into()).max(MIN_TICK_INTERVAL)
	}

	/// The oldest anchor a report may have to be accepted at this tick.
	pub fn oldest(&self) -> Anchor {
		Anchor(self.anchor.0.saturating_sub(self.settings.report_window))
	}

	/// Check a report `sender` sent against the rules of this tick: it must decode as a
	/// [`SignedPriceReport`], be anchored within the report window, come from an accepted
	/// signer, and carry a valid signature, checked in that order. `Err` is the reputation
	/// change the sender deserves.
	///
	/// Reports anchored ahead of this tick are accepted: a peer may be ahead of this node, and
	/// the runtime rejects anchors ahead of the block including them.
	// TODO: consider bounding how far ahead an anchor may be. A signer anchoring far ahead pins
	// its own pool slot and costs every author a filtered report per block.
	pub fn check<Signature>(
		&self,
		sender: &PeerId,
		mut data: &[u8],
	) -> Result<SignedPriceReport<Id, Signature>, ReputationChange>
	where
		Id: RuntimeAppPublic<Signature = Signature> + Decode + PartialEq,
		Signature: Decode,
	{
		let reject = |cost: ReputationChange, what: &str| {
			log::debug!(target: LOG_TARGET, "Rejecting report from {sender}: {what}");
			Err(cost)
		};
		let Ok(report) = SignedPriceReport::<Id, Signature>::decode_all(&mut data) else {
			return reject(cost::MALFORMED, "undecodable");
		};
		if report.report.anchor < self.oldest() {
			return reject(cost::STALE_REPORT, "stale");
		}
		if !self.settings.signers.contains(&report.signer) {
			return reject(cost::UNKNOWN_SIGNER, "unknown signer");
		}
		if !report.verify_signature() {
			return reject(cost::BAD_SIGNATURE, "bad signature");
		}
		Ok(report)
	}
}

/// Fetch and price the markets, aggregate the prices, and sign the result as a report anchored
/// at `setup.anchor`.
///
/// Returns the encoded signed report after inserting it into `pool`. Returns `None` if the node
/// has no signer key, no markets are registered, no market could be priced, the aggregation
/// yields no quotes, or signing fails.
pub async fn run_tick<Block, Client, Id, Signature>(
	client: Arc<Client>,
	fetcher: Fetcher,
	keystore: KeystorePtr,
	pool: ReportPool<Id, Signature>,
	setup: TickSetup<Block::Hash, Id>,
) -> Option<Vec<u8>>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: PriceOracleApi<Block, Id>,
	Id: RuntimeAppPublic<Signature = Signature> + AppCrypto + Ord + Clone + Encode + Decode,
	Signature: Clone + Encode,
{
	let interval = setup.interval();
	let TickSetup { best_hash, anchor, signer, markets, .. } = setup;
	let Some(signer) = signer else {
		log::debug!(target: LOG_TARGET, "No signer key in the keystore, not reporting");
		return None;
	};
	if markets.is_empty() {
		log::debug!(target: LOG_TARGET, "No markets registered, not reporting");
		return None;
	}

	let deadline = Instant::now() + interval.saturating_sub(TICK_MARGIN);
	let (tx, mut rx) = mpsc::unbounded();
	let fetch = fetcher.fetch_markets(&markets, deadline, tx);
	let parse = async {
		let mut prices = Vec::new();
		while let Some(MarketResponses { market, responses }) = rx.next().await {
			match client.runtime_api().parse(best_hash, market, responses, now_ms()) {
				Ok(Ok(price)) => prices.push((market, price)),
				Ok(Err(e)) => log::debug!(
					target: LOG_TARGET,
					"Market {market:?} not priced: {}",
					String::from_utf8_lossy(&e.0),
				),
				Err(e) => log::warn!(target: LOG_TARGET, "Runtime `parse` failed: {e}"),
			}
		}
		prices
	};
	let (failures, prices) = futures::join!(fetch, parse);
	for (market, failure) in failures {
		log::debug!(target: LOG_TARGET, "Market {market:?} not fetched: {failure:?}");
	}
	log::debug!(target: LOG_TARGET, "Priced {} of {} markets", prices.len(), markets.len());
	if prices.is_empty() {
		return None;
	}

	let quotes = match client.runtime_api().aggregate(best_hash, prices) {
		Ok(quotes) => quotes,
		Err(e) => {
			log::warn!(target: LOG_TARGET, "Runtime `aggregate` failed: {e}");
			return None;
		},
	};
	if quotes.is_empty() {
		log::debug!(target: LOG_TARGET, "Nothing aggregated, not reporting");
		return None;
	}

	let report = PriceReport { anchor, quotes };
	let signed = signer::sign_report(&keystore, signer, report)?;
	log::info!(
		target: LOG_TARGET,
		"Reporting {} pair prices at anchor {anchor:?}",
		signed.report.quotes.len(),
	);
	let encoded = signed.encode();
	pool.insert(signed);
	Some(encoded)
}

/// Current Unix time in milliseconds.
fn now_ms() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_consensus_aura::sr25519::{AuthorityId, AuthoritySignature};
	use sp_core::{crypto::Pair as _, sr25519, H256};

	type Signed = SignedPriceReport<AuthorityId, AuthoritySignature>;

	fn pair(seed: u8) -> sr25519::Pair {
		sr25519::Pair::from_seed(&[seed; 32])
	}
	fn id(seed: u8) -> AuthorityId {
		pair(seed).public().into()
	}
	fn signed(seed: u8, anchor: u32) -> Signed {
		let report = PriceReport { anchor: Anchor(anchor), quotes: vec![] };
		let signature = pair(seed).sign(&report.signing_payload()).into();
		Signed { report, signer: id(seed), signature }
	}

	/// A tick accepting signers 1 and 2, at anchor 100 with a window of 10.
	fn tick() -> TickSetup<H256, AuthorityId> {
		TickSetup {
			best_hash: H256::zero(),
			anchor: Anchor(100),
			settings: Settings {
				signers: vec![id(1), id(2)],
				report_window: 10,
				tick_interval_ms: 2_000,
			},
			signer: None,
			markets: vec![],
		}
	}

	fn check(tick: &TickSetup<H256, AuthorityId>, data: &[u8]) -> Result<Signed, ReputationChange> {
		tick.check(&PeerId::random(), data)
	}

	#[test]
	fn valid_report_is_returned() {
		let tick = tick();
		let report = signed(1, 99);
		assert_eq!(check(&tick, &report.encode()), Ok(report));
	}

	#[test]
	fn stale_reports_are_rejected_and_ahead_ones_accepted() {
		let tick = tick();
		// Oldest accepted: 90. No upper bound.
		assert_eq!(tick.oldest(), Anchor(90));
		assert!(check(&tick, &signed(1, 90).encode()).is_ok());
		assert!(check(&tick, &signed(2, 1_000).encode()).is_ok());
		assert_eq!(check(&tick, &signed(1, 89).encode()), Err(cost::STALE_REPORT));
	}

	#[test]
	fn unknown_signer_and_bad_signature_are_rejected() {
		let tick = tick();
		assert_eq!(check(&tick, &signed(3, 100).encode()), Err(cost::UNKNOWN_SIGNER));
		let mut forged = signed(1, 100);
		forged.report.anchor = Anchor(99);
		assert_eq!(check(&tick, &forged.encode()), Err(cost::BAD_SIGNATURE));
		assert_eq!(check(&tick, b"garbage"), Err(cost::MALFORMED));
		// A valid report with trailing bytes is malformed too.
		let mut padded = signed(1, 100).encode();
		padded.push(0);
		assert_eq!(check(&tick, &padded), Err(cost::MALFORMED));
	}
}
