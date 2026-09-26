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
//! Node side of the price oracle.
//!
//! Fetches market data from exchanges, prices it through the runtime APIs of
//! `sp-price-oracle`, signs the resulting pair prices as a report, gossips the report to the
//! other oracle nodes, and provides the collected reports to the block author as inherent data.
//! The crate README describes the design.

#![warn(missing_docs)]

pub mod fetcher;
pub mod gossip;
pub mod inherent;
pub mod pool;
pub mod signer;

#[cfg(test)]
mod tests;

pub use gossip::peers_set_config;
pub use inherent::PriceOracleInherentDataProvider;
pub use pool::ReportPool;

use codec::{Decode, Encode};
use fetcher::{Fetcher, MarketResponses};
use futures::{channel::mpsc, future::FusedFuture, FutureExt, StreamExt};
use gossip::{Acceptance, ReportValidator};
use prometheus_endpoint::Registry;
use sc_network::{service::traits::NotificationService, ProtocolName};
use sc_network_gossip::{GossipEngine, Network, Syncing};
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppCrypto;
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_keystore::KeystorePtr;
use sp_price_oracle::{
	market::Market,
	runtime_api::{PriceOracleApi, PriceOracleMarketApi},
	Anchor, PriceReport,
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
const MIN_TICK_INTERVAL: Duration = Duration::from_millis(500);
/// Part of the tick interval reserved for aggregating and signing. Fetching stops this long
/// before the tick interval elapses.
const TICK_MARGIN: Duration = Duration::from_millis(200);

/// Parameters of [`run`].
pub struct Params<Client, Net, SyncService, Id, Signature> {
	/// Client providing the chain head and the runtime APIs.
	pub client: Arc<Client>,
	/// Network service the gossip engine runs on. Peer reputation changes are reported through it.
	pub network: Net,
	/// Sync service the gossip engine consults. Also tells whether the node is major syncing.
	pub sync: SyncService,
	/// Notification service of the gossip protocol, as returned by [`peers_set_config`].
	pub notification_service: Box<dyn NotificationService>,
	/// Name of the gossip protocol, as returned by [`peers_set_config`].
	pub protocol_name: ProtocolName,
	/// Keystore to look the signer key up in and to sign reports with.
	pub keystore: KeystorePtr,
	/// Pool the service inserts reports into. The same pool is given to the inherent data
	/// provider.
	pub pool: ReportPool<Id, Signature>,
	/// Registry to register the metrics of the service with, if metrics are enabled.
	// TODO: register metrics of the service itself (tick duration, markets priced and failed,
	// reports signed and received, pool size). Only the gossip engine's metrics are registered.
	pub prometheus_registry: Option<Registry>,
}

/// The price oracle service.
///
/// Runs until the network shuts down. On every tick, the service reads the accepted signers,
/// the report window, the tick interval and the active markets from the runtime at the best
/// block, fetches and prices the markets, aggregates the prices into pair prices, signs them as
/// a report and gossips the report. Reports received from peers are validated and inserted into
/// the pool by the gossip validator.
///
/// A tick is skipped while the node is major syncing, while the previous tick has not finished,
/// and while the runtime does not implement the price oracle APIs.
pub async fn run<Block, Client, Net, SyncService, Id, Signature>(
	params: Params<Client, Net, SyncService, Id, Signature>,
) where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
	Client::Api: PriceOracleApi<Block, Id> + PriceOracleMarketApi<Block>,
	Net: Network<Block> + Clone + Send + Sync + 'static,
	SyncService: Syncing<Block> + SyncOracle + Clone + Send + 'static,
	Id: RuntimeAppPublic<Signature = Signature>
		+ AppCrypto
		+ Ord
		+ Clone
		+ Encode
		+ Decode
		+ Send
		+ Sync
		+ 'static,
	Signature: Clone + Encode + Decode + Send + Sync + 'static,
{
	let Params {
		client,
		network,
		sync,
		notification_service,
		protocol_name,
		keystore,
		pool,
		prometheus_registry,
	} = params;

	let fetcher = match Fetcher::new() {
		Ok(fetcher) => fetcher,
		Err(e) => {
			log::error!(target: LOG_TARGET, "Cannot create the HTTPS client, price oracle disabled: {e}");
			return;
		},
	};
	let report_peer = {
		let network = network.clone();
		move |peer, change| network.report_peer(peer, change)
	};
	let validator =
		Arc::new(ReportValidator::<Block, Id, Signature>::new(pool.clone(), report_peer));
	let mut gossip_engine = GossipEngine::new(
		network,
		sync.clone(),
		notification_service,
		protocol_name.clone(),
		validator.clone(),
		prometheus_registry.as_ref(),
	);
	let mut incoming = gossip_engine.messages_for(gossip::topic::<Block>());

	let mut interval = MIN_TICK_INTERVAL;
	let mut next_tick = Instant::now();
	let mut tick = futures::future::Fuse::terminated();
	let mut api_missing_logged = false;

	log::info!(target: LOG_TARGET, "Price oracle service started on {protocol_name}");
	loop {
		let timer = tokio::time::sleep_until(next_tick).fuse();
		futures::pin_mut!(timer);
		futures::select! {
			_ = timer => {
				next_tick = Instant::now() + interval;
				if !tick.is_terminated() {
					log::warn!(target: LOG_TARGET, "Previous tick still running, skipping a tick");
					continue;
				}
				if sync.is_major_syncing() {
					log::debug!(target: LOG_TARGET, "Major syncing, skipping a tick");
					continue;
				}
				let Some(setup) = TickSetup::read(
					&*client, &keystore, &validator, &pool, &mut api_missing_logged,
				) else {
					continue;
				};
				if setup.interval != interval {
					log::info!(target: LOG_TARGET, "Tick interval is now {:?}", setup.interval);
					interval = setup.interval;
					next_tick = Instant::now() + interval;
				}
				tick = run_tick(client.clone(), fetcher.clone(), keystore.clone(), pool.clone(), setup)
					.boxed()
					.fuse();
			},
			encoded = &mut tick => {
				if let Some(encoded) = encoded {
					gossip_engine.gossip_message(gossip::topic::<Block>(), encoded, false);
				}
			},
			notification = incoming.next() => {
				// Validated and pooled by the validator; the stream is only observed here.
				if notification.is_none() {
					log::warn!(target: LOG_TARGET, "Gossip topic stream ended, stopping");
					return;
				}
			},
			_ = &mut gossip_engine => {
				log::warn!(target: LOG_TARGET, "Gossip engine ended, stopping");
				return;
			},
		}
	}
}

/// The state a tick starts from, read from the runtime at the best block.
struct TickSetup<Hash, Id> {
	best_hash: Hash,
	anchor: Anchor,
	interval: Duration,
	signer: Option<Id>,
	markets: Vec<Market>,
}

impl<Hash: Copy, Id> TickSetup<Hash, Id> {
	/// Read the state at the best block, update the acceptance rules of `validator` and prune
	/// `pool` accordingly.
	///
	/// Returns `None` if a runtime API call fails, which is the case when the runtime does not
	/// implement the price oracle APIs. The failure is logged once, and again only after the
	/// calls have succeeded in between.
	fn read<Block, Client, Signature>(
		client: &Client,
		keystore: &KeystorePtr,
		validator: &ReportValidator<Block, Id, Signature>,
		pool: &ReportPool<Id, Signature>,
		api_missing_logged: &mut bool,
	) -> Option<Self>
	where
		Block: BlockT<Hash = Hash>,
		Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
		Client::Api: PriceOracleApi<Block, Id> + PriceOracleMarketApi<Block>,
		Id: RuntimeAppPublic + AppCrypto + Ord + Clone + Decode,
		Signature: Clone,
	{
		let info = client.info();
		let best_hash = info.best_hash;
		let anchor = Anchor(info.best_number.saturated_into());
		let api = client.runtime_api();

		// TODO: consider reading as much as possible and failing only after few consecutive
		// failures.
		let read = (|| -> Result<_, sp_api::ApiError> {
			Ok((
				api.signers(best_hash)?,
				api.report_window(best_hash)?,
				api.tick_interval_ms(best_hash)?,
				api.markets(best_hash)?,
			))
		})();
		let (signers, window, interval_ms, markets) = match read {
			Ok(read) => {
				*api_missing_logged = false;
				read
			},
			Err(e) => {
				if !*api_missing_logged {
					log::error!(target: LOG_TARGET, "Runtime does not serve the price oracle APIs: {e}");
					*api_missing_logged = true;
				}
				return None;
			},
		};

		let signer = signer::local_signer(keystore, &signers);
		let acceptance = Acceptance { signers, current: anchor, window };
		// TODO: move to upper level or rename function.
		pool.prune(acceptance.oldest());
		// TODO: move to upper level or rename function.
		validator.set_acceptance(acceptance);

		let interval = Duration::from_millis(interval_ms.into()).max(MIN_TICK_INTERVAL);
		Some(Self { best_hash, anchor, interval, signer, markets })
	}
}

/// Fetch and price the markets, aggregate the prices, and sign the result as a report anchored
/// at `setup.anchor`.
///
/// Returns the encoded signed report after inserting it into `pool`. Returns `None` if the node
/// has no signer key, no markets are registered, no market could be priced, the aggregation
/// yields no quotes, or signing fails.
async fn run_tick<Block, Client, Id, Signature>(
	client: Arc<Client>,
	fetcher: Fetcher,
	keystore: KeystorePtr,
	pool: ReportPool<Id, Signature>,
	setup: TickSetup<Block::Hash, Id>,
) -> Option<Vec<u8>>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: PriceOracleMarketApi<Block>,
	Id: RuntimeAppPublic<Signature = Signature> + AppCrypto + Ord + Clone + Encode,
	Signature: Clone + Encode,
{
	let TickSetup { best_hash, anchor, interval, signer, markets } = setup;
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
