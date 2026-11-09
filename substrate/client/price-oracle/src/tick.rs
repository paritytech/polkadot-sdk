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
	metrics::Metrics,
	pool::{ReportPool, Window},
	signer, LOG_TARGET,
};
use codec::{Decode, DecodeAll, Encode};
use futures::{channel::mpsc, StreamExt};
use sc_network::{PeerId, ReputationChange};
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppCrypto;
use sp_blockchain::HeaderBackend;
use sp_keystore::KeystorePtr;
use sp_price_oracle::{
	market::Market, runtime_api::PriceOracleApi, Anchor, PairAggregate, PriceReport, Quote,
	Settings, SignedPriceReport,
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
	/// The number of `best_hash`, the block a report signed at this tick is anchored to.
	block: u32,
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
		let block = info.best_number.saturated_into();
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
		Some(Self { best_hash, block, settings, signer, markets })
	}

	/// Whether the keystore holds one of the accepted signer keys.
	pub fn has_signer(&self) -> bool {
		self.signer.is_some()
	}

	/// The tick interval in force, raised to [`MIN_TICK_INTERVAL`] if shorter.
	pub fn interval(&self) -> Duration {
		Duration::from_millis(self.settings.tick_interval_ms.into()).max(MIN_TICK_INTERVAL)
	}

	/// The anchors a report may have to count at this tick, with this node's clock at `now_ms`.
	///
	/// The block must be within `report_window` of this tick's block, and the timestamp within
	/// `report_window_ms` of `now_ms`. This node's clock is used rather than the chain's time,
	/// which lags behind real time, and freezes when the relay chain stalls. A block or a
	/// timestamp ahead is accepted: a peer may be ahead of this node, and the runtime rejects
	/// blocks ahead of the block including them.
	pub fn window(&self, now_ms: u64) -> Window {
		Window {
			oldest_block: self.block.saturating_sub(self.settings.report_window),
			newest_block: None,
			oldest_timestamp_ms: now_ms.saturating_sub(self.settings.report_window_ms),
		}
	}

	/// Check a report `sender` sent against the rules of this tick, with this node's clock at
	/// `now_ms`: it must decode as a [`SignedPriceReport`], be within the window (see
	/// [`Self::window`]), come from an accepted signer, and carry a valid signature, checked in
	/// that order. `Err` is the reputation change the sender deserves, and the reason.
	// TODO: consider bounding how far ahead an anchor may be. A signer anchoring far ahead pins
	// its own pool slot and costs every author a filtered report per block.
	pub fn check<Signature>(
		&self,
		sender: &PeerId,
		mut data: &[u8],
		now_ms: u64,
	) -> Result<SignedPriceReport<Id, Signature>, (ReputationChange, &'static str)>
	where
		Id: RuntimeAppPublic<Signature = Signature> + Decode + PartialEq,
		Signature: Decode,
	{
		let reject = |cost: ReputationChange, reason: &'static str| {
			log::debug!(target: LOG_TARGET, "Rejecting report from {sender}: {reason}");
			Err((cost, reason))
		};
		let Ok(report) = SignedPriceReport::<Id, Signature>::decode_all(&mut data) else {
			return reject(cost::MALFORMED, "malformed");
		};
		if !self.window(now_ms).contains(&report.report.anchor) {
			return reject(cost::STALE_REPORT, "stale");
		}
		if !self.settings.signers.contains(&report.signer) {
			return reject(cost::UNKNOWN_SIGNER, "unknown_signer");
		}
		// TODO: drop a report the pool already holds a same or newer anchor for before verifying
		// it. A peer replaying a real report makes this node verify signatures without penalty.
		if !report.verify_signature() {
			return reject(cost::BAD_SIGNATURE, "bad_signature");
		}
		Ok(report)
	}
}

/// How a tick ended.
pub(crate) enum TickOutcome {
	/// A report was signed and queued for at least one peer. Delivery is not confirmed.
	Reported,
	/// A report was signed, but could not be queued for any peer.
	NotSent,
	/// The previous tick was still running.
	Overrun,
	/// The node was major syncing.
	Syncing,
	/// The settings or the markets could not be read from the runtime.
	ApiUnavailable,
	/// The keystore holds none of the signer keys.
	NoSigner,
	/// No markets are registered.
	NoMarkets,
	/// The `aggregate` runtime call failed.
	RuntimeError,
	/// No pair reached its quorum.
	NothingAggregated,
	/// The keystore did not sign the report.
	SigningFailed,
}

impl TickOutcome {
	/// The outcome label of the tick metric.
	pub(crate) fn label(&self) -> &'static str {
		match self {
			Self::Reported => "reported",
			Self::NotSent => "not_sent",
			Self::Overrun => "overrun",
			Self::Syncing => "syncing",
			Self::ApiUnavailable => "api_unavailable",
			Self::NoSigner => "no_signer",
			Self::NoMarkets => "no_markets",
			Self::RuntimeError => "runtime_error",
			Self::NothingAggregated => "nothing_aggregated",
			Self::SigningFailed => "signing_failed",
		}
	}
}

/// Fetch and price the markets, aggregate the prices, and sign the result as a report anchored
/// at `setup`'s block and this node's clock at signing.
///
/// Returns the encoded signed report after inserting it into `pool`, or the outcome that
/// stopped the tick.
pub(crate) async fn run_tick<Block, Client, Id, Signature>(
	client: Arc<Client>,
	fetcher: Fetcher,
	keystore: KeystorePtr,
	pool: ReportPool<Id, Signature>,
	metrics: Metrics,
	setup: TickSetup<Block::Hash, Id>,
) -> Result<Vec<u8>, TickOutcome>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: PriceOracleApi<Block, Id>,
	Id: RuntimeAppPublic<Signature = Signature> + AppCrypto + Ord + Clone + Encode + Decode,
	Signature: Clone + Encode,
{
	let interval = setup.interval();
	let TickSetup { best_hash, block, signer, markets, .. } = setup;
	// The missing signer is warned about in `run`.
	let signer = signer.ok_or(TickOutcome::NoSigner)?;
	if markets.is_empty() {
		log::debug!(target: LOG_TARGET, "No markets registered, not reporting");
		return Err(TickOutcome::NoMarkets);
	}

	// Times the rest of the tick, whichever way it ends.
	let _timer = metrics.tick_timer();
	let started = Instant::now();
	let deadline = started + interval.saturating_sub(TICK_MARGIN);
	let (tx, mut rx) = mpsc::unbounded();
	let fetch = fetcher.fetch_markets(&markets, deadline, tx);
	let parse = async {
		let mut prices = Vec::new();
		while let Some(MarketResponses { market, responses }) = rx.next().await {
			metrics.on_market_fetched(market, started.elapsed());
			match client.runtime_api().parse(best_hash, market, responses, now_ms()) {
				Ok(Ok(price)) => {
					metrics.on_market_result(market, "priced");
					prices.push((market, price));
				},
				Ok(Err(e)) => {
					log::info!(
						target: LOG_TARGET,
						"Market {market:?} not priced: {}",
						String::from_utf8_lossy(&e.0),
					);
					metrics.on_market_result(market, "parse");
				},
				Err(e) => {
					log::error!(target: LOG_TARGET, "Runtime `parse` failed: {e}");
					metrics.on_market_result(market, "runtime_error");
				},
			}
		}
		prices
	};
	let (failures, prices) = futures::join!(fetch, parse);
	for (market, failure) in failures {
		log::debug!(target: LOG_TARGET, "Market {market:?} not fetched: {failure}");
		metrics.on_market_result(market, failure.label());
	}

	let priced = prices.len();
	let aggregates = match client.runtime_api().aggregate(best_hash, prices) {
		Ok(aggregates) => aggregates,
		Err(e) => {
			log::error!(target: LOG_TARGET, "Runtime `aggregate` failed: {e}");
			return Err(TickOutcome::RuntimeError);
		},
	};
	metrics.reset_pair_vote_margins();
	let quotes: Vec<Quote> = aggregates
		.into_iter()
		.filter_map(|PairAggregate { pair, votes, quorum, price }| {
			metrics.on_pair_vote_margin(pair, votes, quorum);
			if votes < quorum {
				log::warn!(target: LOG_TARGET, "{pair:?} below quorum: {votes} of {quorum} votes");
			} else if votes == quorum {
				// One failing market away from being unpriced.
				log::debug!(target: LOG_TARGET, "{pair:?} at quorum: {votes} of {quorum} votes");
			}
			Some(Quote { pair, price: price? })
		})
		.collect();
	if quotes.is_empty() {
		log::debug!(target: LOG_TARGET, "Nothing aggregated, not reporting");
		return Err(TickOutcome::NothingAggregated);
	}
	log::debug!(target: LOG_TARGET, "Aggregated quotes: {quotes:?}");

	let anchor = Anchor::new(block, now_ms());
	let report = PriceReport { anchor, quotes };
	let signed =
		signer::sign_report(&keystore, signer, report).ok_or(TickOutcome::SigningFailed)?;
	log::info!(
		target: LOG_TARGET,
		"Reporting {} pair prices from {priced} of {} markets at anchor {anchor:?}, tick took {:?}",
		signed.report.quotes.len(),
		markets.len(),
		started.elapsed(),
	);
	let encoded = signed.encode();
	pool.insert(signed);
	Ok(encoded)
}

/// Current Unix time in milliseconds.
pub(crate) fn now_ms() -> u64 {
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
	/// This node's clock in the tests.
	const NOW_MS: u64 = 1_800_000_000_000;

	fn signed_at(seed: u8, block: u32, timestamp_ms: u64) -> Signed {
		let report = PriceReport { anchor: Anchor::new(block, timestamp_ms), quotes: vec![] };
		let signature = pair(seed).sign(&report.signing_payload()).into();
		Signed { report, signer: id(seed), signature }
	}
	fn signed(seed: u8, block: u32) -> Signed {
		signed_at(seed, block, NOW_MS)
	}

	/// A tick accepting signers 1 and 2, at block 100 with windows of 10 blocks and 30 s.
	fn tick() -> TickSetup<H256, AuthorityId> {
		TickSetup {
			best_hash: H256::zero(),
			block: 100,
			settings: Settings {
				signers: vec![id(1), id(2)],
				report_window: 10,
				report_window_ms: 30_000,
				tick_interval_ms: 2_000,
			},
			signer: None,
			markets: vec![],
		}
	}

	fn check(tick: &TickSetup<H256, AuthorityId>, data: &[u8]) -> Result<Signed, ReputationChange> {
		tick.check(&PeerId::random(), data, NOW_MS).map_err(|(cost, _)| cost)
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
		// Oldest accepted: block 90, 30 s ago. No upper bound on either.
		assert_eq!(
			tick.window(NOW_MS),
			Window { oldest_block: 90, newest_block: None, oldest_timestamp_ms: NOW_MS - 30_000 }
		);
		assert!(check(&tick, &signed(1, 90).encode()).is_ok());
		assert!(check(&tick, &signed(2, 1_000).encode()).is_ok());
		assert!(check(&tick, &signed_at(1, 100, NOW_MS - 30_000).encode()).is_ok());
		assert!(check(&tick, &signed_at(2, 100, NOW_MS + 60_000).encode()).is_ok());
		assert_eq!(check(&tick, &signed(1, 89).encode()), Err(cost::STALE_REPORT));
		assert_eq!(
			check(&tick, &signed_at(1, 100, NOW_MS - 30_001).encode()),
			Err(cost::STALE_REPORT)
		);
	}

	#[test]
	fn unknown_signer_and_bad_signature_are_rejected() {
		let tick = tick();
		assert_eq!(check(&tick, &signed(3, 100).encode()), Err(cost::UNKNOWN_SIGNER));
		let mut forged = signed(1, 100);
		forged.report.anchor = Anchor::new(99, NOW_MS);
		assert_eq!(check(&tick, &forged.encode()), Err(cost::BAD_SIGNATURE));
		assert_eq!(check(&tick, b"garbage"), Err(cost::MALFORMED));
		// A valid report with trailing bytes is malformed too.
		let mut padded = signed(1, 100).encode();
		padded.push(0);
		assert_eq!(check(&tick, &padded), Err(cost::MALFORMED));
	}
}
