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
	decimal,
	market::{Market, MarketId},
	runtime_api::{PriceOracleApi, PricingState},
	Anchor, PairAggregate, PairId, Price, PriceReport, Quote, Settings, SignedPriceReport,
};
use sp_runtime::{
	traits::{Block as BlockT, SaturatedConversion},
	RuntimeAppPublic,
};
use std::{
	collections::BTreeMap,
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
	pub(crate) const DUPLICATE_REPORT: Rep = Rep::new(-50, "Price oracle: duplicate report");
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
	/// [`Self::window`]), come from an accepted signer, be newer than what `pool` holds of that
	/// signer, and carry a valid signature, checked in that order. `Err` is the reputation
	/// change the sender deserves, and the reason.
	// TODO: consider bounding how far ahead an anchor may be. A signer anchoring far ahead pins
	// its own pool slot and costs every author a filtered report per block.
	pub fn check<Signature>(
		&self,
		sender: &PeerId,
		mut data: &[u8],
		now_ms: u64,
		pool: &ReportPool<Id, Signature>,
	) -> Result<SignedPriceReport<Id, Signature>, (ReputationChange, &'static str)>
	where
		Id: RuntimeAppPublic<Signature = Signature> + Decode + Ord,
		Signature: Decode + Clone,
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
		// A signer sends each report once, with a fresh anchor. A report the pool already holds,
		// or an older one, is a replay: dropped before the signature is verified.
		if pool.holds_at_least(&report.signer, &report.report.anchor) {
			return reject(cost::DUPLICATE_REPORT, "superseded");
		}
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

/// Logs the markets and the pairs when their status changes, not every tick.
///
/// A market is logged when it stops being priced, with the reason, when the reason changes, and
/// when it is priced again. A pair is logged when it stops being priced and when it is priced
/// again. A market that was not fetched keeps its last status.
#[derive(Default)]
pub(crate) struct LogState {
	/// The number of registered markets.
	markets: usize,
	/// The markets priced this tick so far.
	priced: usize,
	/// The markets not priced at the last tick that fetched them, with the reason.
	unpriced_markets: BTreeMap<MarketId, String>,
	/// How each pair was aggregated at the last tick that aggregated it: inputs, quorum, price
	/// and note.
	pairs: BTreeMap<PairId, (u32, u32, Option<Price>, String)>,
}

impl LogState {
	/// A tick starts with the `registered` markets. Those no longer registered are forgotten.
	fn tick_starts(&mut self, registered: &[Market]) {
		self.markets = registered.len();
		self.priced = 0;
		self.unpriced_markets.retain(|id, _| registered.iter().any(|m| m.id == *id));
	}

	/// `market` was priced.
	fn market_priced(&mut self, market: MarketId) {
		self.priced += 1;
		if self.unpriced_markets.remove(&market).is_some() {
			log::info!(target: LOG_TARGET, "Market {market:?} priced again");
		}
	}

	/// `market` was not priced for `reason`.
	fn market_not_priced(&mut self, market: MarketId, reason: String) {
		if self.unpriced_markets.get(&market) == Some(&reason) {
			log::trace!(target: LOG_TARGET, "Market {market:?} still not priced: {reason}");
		} else {
			log::info!(target: LOG_TARGET, "Market {market:?} not priced: {reason}");
			self.unpriced_markets.insert(market, reason);
		}
	}

	/// `pair` was priced at `price` from `inputs` against its `quorum`, or not priced if `None`,
	/// with `note` saying how.
	fn pair(&mut self, pair: PairId, inputs: u32, quorum: u32, price: Option<Price>, note: &str) {
		let last = self.pairs.insert(pair, (inputs, quorum, price, note.into()));
		let was_priced = last.map(|(_, _, price, _)| price.is_some());
		match price {
			Some(price) if was_priced != Some(true) => {
				log::info!(
					target: LOG_TARGET,
					"{pair:?} priced at {}: {note}, {inputs}/{quorum}",
					decimal(price),
				)
			},
			None if was_priced != Some(false) => {
				log::warn!(target: LOG_TARGET, "{pair:?} not priced: {note}, {inputs}/{quorum}")
			},
			_ => {},
		}
	}

	/// The tick at `block` is done pricing, `took` after it started. Logs the tick: the pairs,
	/// the markets priced and the time.
	fn tick_ends(&self, block: u32, took: Duration) {
		let pairs: Vec<String> = self
			.pairs
			.iter()
			.map(|(pair, (inputs, quorum, price, note))| match price {
				Some(price) => format!("{pair:?} {} ({note}, {inputs}/{quorum})", decimal(*price)),
				None => format!("{pair:?} not priced ({note}, {inputs}/{quorum})"),
			})
			.collect();
		log::debug!(
			target: LOG_TARGET,
			"Block {block}: {}; {} of {} markets, tick took {took:?}",
			pairs.join(", "),
			self.priced,
			self.markets,
		);
	}
}

/// Fetch and price the markets, aggregate the prices, and sign the result as a report anchored
/// at `setup`'s block and this node's clock at signing.
///
/// `pricing_state` is what the last tick that reported left for this one, and `log_state` what
/// the markets and the pairs were last logged as.
///
/// Returns the encoded signed report after inserting it into `pool`, or the outcome that stopped
/// the tick, with the states for the next tick. The pricing state is empty unless the tick
/// reported.
pub(crate) async fn run_tick<Block, Client, Id, Signature>(
	client: Arc<Client>,
	fetcher: Fetcher,
	keystore: KeystorePtr,
	pool: ReportPool<Id, Signature>,
	metrics: Metrics,
	setup: TickSetup<Block::Hash, Id>,
	pricing_state: PricingState,
	mut log_state: LogState,
) -> (Result<Vec<u8>, TickOutcome>, PricingState, LogState)
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: PriceOracleApi<Block, Id>,
	Id: RuntimeAppPublic<Signature = Signature> + AppCrypto + Ord + Clone + Encode + Decode,
	Signature: Clone + Encode,
{
	let interval = setup.interval();
	let TickSetup { best_hash, block, signer, markets, .. } = setup;
	// Nothing to do without a signer key or markets. The missing signer is warned about in `run`.
	let Some(signer) = signer else {
		return (Err(TickOutcome::NoSigner), PricingState::default(), log_state);
	};
	if markets.is_empty() {
		log::debug!(target: LOG_TARGET, "No markets registered, not reporting");
		return (Err(TickOutcome::NoMarkets), PricingState::default(), log_state);
	}
	log_state.tick_starts(&markets);

	// Times the rest of the tick, whichever way it ends.
	let _timer = metrics.tick_timer();
	let started = Instant::now();
	let deadline = started + interval.saturating_sub(TICK_MARGIN);

	// Fetch the markets until the deadline, and have the runtime price each one as it arrives.
	let (tx, mut rx) = mpsc::unbounded();
	let fetch = fetcher.fetch_markets(&markets, deadline, tx);
	let parse = async {
		let mut outputs = Vec::new();
		while let Some(MarketResponses { market, responses }) = rx.next().await {
			metrics.on_market_fetched(market, started.elapsed());
			match client.runtime_api().parse(best_hash, market, responses, now_ms()) {
				Ok(Ok(output)) => {
					log_state.market_priced(market);
					metrics.on_market_result(market, "priced");
					outputs.push((market, output));
				},
				Ok(Err(e)) => {
					let reason = e.to_string();
					metrics.on_market_result(market, &format!("parse_{reason}"));
					log_state.market_not_priced(market, reason);
				},
				Err(e) => {
					log::error!(target: LOG_TARGET, "Runtime `parse` failed: {e}");
					metrics.on_market_result(market, "runtime_error");
				},
			}
		}
		outputs
	};
	// Fetching and parsing run together. Fetching ends at the deadline or when every market
	// answered, parsing once fetching ends, with the markets that gave no responses left over.
	let (failures, outputs) = futures::join!(fetch, parse);
	for (market, failure) in failures {
		log::debug!(target: LOG_TARGET, "Market {market:?} not fetched: {failure}");
		metrics.on_market_result(market, failure.label());
	}

	// Have the runtime turn the market prices into pair prices.
	let (aggregates, pricing_state) =
		match client.runtime_api().aggregate(best_hash, outputs, pricing_state) {
			Ok(aggregated) => aggregated,
			Err(e) => {
				log::error!(target: LOG_TARGET, "Runtime `aggregate` failed: {e}");
				return (Err(TickOutcome::RuntimeError), PricingState::default(), log_state);
			},
		};
	// Record every pair, and quote those with a price.
	metrics.reset_pair_market_margins();
	let quotes: Vec<Quote> = aggregates
		.iter()
		.filter_map(|aggregate| {
			let PairAggregate { pair, inputs, quorum, price, .. } = *aggregate;
			metrics.on_pair_market_margin(pair, inputs, quorum);
			log_state.pair(pair, inputs, quorum, price, &aggregate.note());
			price.map(|price| Quote { pair, price })
		})
		.collect();

	log_state.tick_ends(block, started.elapsed());
	if quotes.is_empty() {
		return (Err(TickOutcome::NothingAggregated), PricingState::default(), log_state);
	}

	// Sign the report, pool it for the blocks this node authors, and hand it over for sending.
	let anchor = Anchor::new(block, now_ms());
	let report = PriceReport { anchor, quotes };
	let Some(signed) = signer::sign_report(&keystore, signer, report) else {
		return (Err(TickOutcome::SigningFailed), PricingState::default(), log_state);
	};
	let encoded = signed.encode();
	pool.insert(signed);
	(Ok(encoded), pricing_state, log_state)
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
		check_with(tick, data, &ReportPool::new())
	}

	fn check_with(
		tick: &TickSetup<H256, AuthorityId>,
		data: &[u8],
		pool: &ReportPool<AuthorityId, AuthoritySignature>,
	) -> Result<Signed, ReputationChange> {
		tick.check(&PeerId::random(), data, NOW_MS, pool).map_err(|(cost, _)| cost)
	}

	#[test]
	fn replays_are_rejected_before_the_signature_is_checked() {
		let tick = tick();
		let pool = ReportPool::new();
		pool.insert(signed(1, 100));
		// The same anchor and an older one are replays, a newer one is not.
		assert_eq!(check_with(&tick, &signed(1, 100).encode(), &pool), Err(cost::DUPLICATE_REPORT));
		assert_eq!(check_with(&tick, &signed(1, 95).encode(), &pool), Err(cost::DUPLICATE_REPORT));
		assert!(check_with(&tick, &signed(1, 101).encode(), &pool).is_ok());
		// Another signer is not affected.
		assert!(check_with(&tick, &signed(2, 100).encode(), &pool).is_ok());
		// A replay with a broken signature is still a replay: it costs no verification.
		let mut broken = signed(1, 100).encode();
		let last = broken.len() - 1;
		broken[last] ^= 1;
		assert_eq!(check_with(&tick, &broken, &pool), Err(cost::DUPLICATE_REPORT));
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
