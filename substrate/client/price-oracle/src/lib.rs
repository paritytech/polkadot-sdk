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
#![doc = include_str!("../README.md")]
#![warn(missing_docs)]

pub mod delivery;
pub mod fetcher;
pub mod inherent;
mod metrics;
pub mod pool;
pub mod signer;
pub mod tick;

pub use delivery::{peers_set_config, ReportRecipients};
pub use inherent::PriceOracleInherentDataProvider;
pub use pool::ReportPool;

use codec::{Decode, Encode};
use delivery::Targets;
use fetcher::Fetcher;
use futures::{future::FusedFuture, FutureExt};
use metrics::Metrics;
use prometheus_endpoint::Registry;
use sc_network::{
	service::traits::{NetworkService, NotificationEvent, NotificationService, ValidationResult},
	ProtocolName,
};
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppCrypto;
use sp_blockchain::HeaderBackend;
use sp_consensus::SyncOracle;
use sp_keystore::KeystorePtr;
use sp_price_oracle::runtime_api::{PriceOracleApi, PricingState};
use sp_runtime::{traits::Block as BlockT, RuntimeAppPublic};
use std::sync::Arc;
use tick::{benefit::GOOD_REPORT, run_tick, LogState, TickOutcome, TickSetup, MIN_TICK_INTERVAL};
use tokio::time::MissedTickBehavior;

pub(crate) const LOG_TARGET: &str = "price-oracle";

/// Parameters of [`run`].
pub struct Params<Client, SyncService, Recipients, Id, Signature> {
	/// Client providing the chain head and the runtime APIs.
	pub client: Arc<Client>,
	/// Network service the protocol runs on. The recipients are reserved and peer reputation
	/// changes are reported through it.
	pub network: Arc<dyn NetworkService>,
	/// Tells whether the node is major syncing.
	pub sync: SyncService,
	/// Notification service of the protocol, as returned by [`peers_set_config`].
	pub notification_service: Box<dyn NotificationService>,
	/// Name of the protocol, as returned by [`peers_set_config`].
	pub protocol_name: ProtocolName,
	/// Where the reports are sent.
	pub recipients: Recipients,
	/// How many of the peers that stopped being recipients stay reserved while this node is a
	/// recipient, the most recent ones.
	///
	/// While this node is a recipient, a peer that is no longer one is still sending its reports
	/// to this node. For example, when the recipients are the authors of the next 4 slots, the
	/// authors of the last 4 slots may be, so `keep` should be 4.
	pub keep: usize,
	/// Keystore to look the signer key up in and to sign reports with.
	pub keystore: KeystorePtr,
	/// Pool the service inserts reports into. The same pool is given to the inherent data
	/// provider.
	pub pool: ReportPool<Id, Signature>,
	/// Registry to register the metrics of the service with, if metrics are enabled.
	pub prometheus_registry: Option<Registry>,
}

/// The price oracle service.
///
/// Runs until the network shuts down. On every tick, the service reads the accepted signers,
/// the report window, the tick interval and the active markets from the runtime at the best
/// block, fetches and prices the markets, aggregates the prices into pair prices, signs them as
/// a report and sends the report to the recipients of the tick. Reports received from peers are
/// validated and inserted into the pool.
///
/// A tick is skipped while the node is major syncing, while the previous tick has not finished,
/// and while the runtime does not implement the price oracle APIs.
pub async fn run<Block, Client, SyncService, Recipients, Id, Signature>(
	params: Params<Client, SyncService, Recipients, Id, Signature>,
) where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block> + Send + Sync + 'static,
	Client::Api: PriceOracleApi<Block, Id>,
	SyncService: SyncOracle + Send + 'static,
	Recipients: ReportRecipients<Block> + 'static,
	Id: RuntimeAppPublic<Signature = Signature>
		+ AppCrypto
		+ Ord
		+ Clone
		+ Encode
		+ Decode
		+ Send
		+ Sync
		+ 'static,
	Signature: Clone + PartialEq + Encode + Decode + Send + Sync + 'static,
{
	let Params {
		client,
		network,
		sync,
		mut notification_service,
		protocol_name,
		recipients,
		keep,
		keystore,
		pool,
		prometheus_registry,
	} = params;

	let metrics = Metrics::register(prometheus_registry.as_ref());

	let fetcher = match Fetcher::new() {
		Ok(fetcher) => fetcher,
		Err(e) => {
			log::error!(
				target: LOG_TARGET,
				"Cannot create the HTTPS client, price oracle disabled: {e}",
			);
			return;
		},
	};
	// The setup of the last tick, whose rules incoming reports are checked against. `None`
	// before the first tick and while the node is syncing: reports are then dropped without
	// judging the sender.
	let mut tick_setup: Option<TickSetup<Block::Hash, Id>> = None;
	let mut targets = Targets::new(network.clone(), protocol_name.clone(), keep);

	let mut ticker = tokio::time::interval(MIN_TICK_INTERVAL);
	ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
	let mut tick = futures::future::Fuse::terminated();
	// What the last tick left for the next one: the pricing state, which a tick that does not
	// report drops, and what the markets and the pairs were last logged as. Both go with the
	// tick and come back with its result.
	let mut pricing_state = PricingState::default();
	let mut log_state = LogState::default();
	let mut api_missing_logged = false;
	let mut no_signer_logged = false;

	log::info!(target: LOG_TARGET, "Price oracle service started on {protocol_name}");
	loop {
		tokio::select! {
			// The interval elapsed: start a tick, unless the previous one is still running or
			// the node is syncing.
			ticked_at = ticker.tick() => {
				if !tick.is_terminated() {
					log::warn!(target: LOG_TARGET, "Previous tick still running, skipping a tick");
					metrics.on_tick(TickOutcome::Overrun);
					continue;
				}
				if sync.is_major_syncing() {
					log::debug!(target: LOG_TARGET, "Major syncing, skipping a tick");
					metrics.on_tick(TickOutcome::Syncing);
					// The rules of the last tick are far behind; do not judge peers by them.
					tick_setup = None;
					continue;
				}
				// Read the settings and the markets from the runtime at the best block. Without
				// them the node reports nothing this tick, but keeps receiving reports under
				// the rules of the last tick it read.
				let Some(setup) = TickSetup::read(&*client, &keystore, &mut api_missing_logged)
				else {
					metrics.on_tick(TickOutcome::ApiUnavailable);
					continue;
				};
				// Warn only once until a signer key is found.
				if setup.has_signer() {
					no_signer_logged = false;
				} else if !no_signer_logged {
					log::warn!(
						target: LOG_TARGET,
						"No accepted signer key in the keystore, not reporting",
					);
					no_signer_logged = true;
				}
				// Drop pooled reports that fell out of the report window.
				pool.prune(&setup.window(tick::now_ms()));
				metrics.on_pool_reports(pool.len());
				// Keep a substream open to the recipients of this tick.
				targets.reserve(recipients.recipients(setup.best_hash).await);
				// Follow a changed tick interval from the next tick on.
				if setup.interval() != ticker.period() {
					let period = setup.interval();
					log::info!(target: LOG_TARGET, "Tick interval is now {period:?}");
					ticker = tokio::time::interval_at(ticked_at + period, period);
					ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
				}
				// Fetch, price, sign and pool in the background; the result arrives in the arm below.
				tick = run_tick(
					client.clone(),
					fetcher.clone(),
					keystore.clone(),
					pool.clone(),
					metrics.clone(),
					setup.clone(),
					// Moved, not cloned: the tick owns the states until it returns them below.
					std::mem::take(&mut pricing_state),
					std::mem::take(&mut log_state),
				)
				.boxed()
				.fuse();
				// Check incoming reports against the rules of this tick.
				tick_setup = Some(setup);
			},
			// The tick finished: send its report, if it produced one.
			(result, pricing, log) = &mut tick, if !tick.is_terminated() => {
				// Take the states back for the next tick.
				pricing_state = pricing;
				log_state = log;
				// A tick that produced no report is done.
				let encoded = match result {
					Ok(encoded) => encoded,
					Err(outcome) => {
						metrics.on_tick(outcome);
						continue;
					},
				};
				// Send the report to every recipient reachable now.
				let mut sent = 0;
				for peer in targets.reachable() {
					match notification_service.send_async_notification(peer, encoded.clone()).await {
						Ok(()) => sent += 1,
						Err(e) => log::debug!(target: LOG_TARGET, "Cannot send the report to {peer}: {e}"),
					}
				}
				if sent < targets.len() {
					log::debug!(target: LOG_TARGET, "Sent the report to {sent} of {} peers", targets.len());
				} else {
					log::trace!(target: LOG_TARGET, "Sent the report to all {sent} peers");
				}
				metrics.on_tick(if sent > 0 { TickOutcome::Reported } else { TickOutcome::NotSent });
			},
			// The network delivered a report from a peer, or a substream opened or closed.
			event = notification_service.next_event() => {
				use NotificationEvent::*;
				match event {
					Some(NotificationReceived { peer, notification }) => {
						let Some(setup) = &tick_setup else {
							log::trace!(target: LOG_TARGET, "No tick setup yet, dropping a report from {peer}");
							metrics.on_received_report("no_tick_setup");
							continue;
						};
						match setup.check(&peer, &notification, tick::now_ms()) {
							Ok(report) =>
								if pool.insert(report) {
									network.report_peer(peer, GOOD_REPORT);
									metrics.on_received_report("accepted");
								} else {
									log::trace!(target: LOG_TARGET, "Superseded report from {peer}");
									metrics.on_received_report("superseded");
								},
							Err((cost, reason)) => {
								network.report_peer(peer, cost);
								metrics.on_received_report(reason);
							},
						}
					},
					Some(NotificationStreamOpened { peer, direction, .. }) => {
						log::debug!(target: LOG_TARGET, "Substream with {peer} opened, {direction:?}");
						targets.opened(peer);
					},
					Some(NotificationStreamClosed { peer }) => {
						log::debug!(target: LOG_TARGET, "Substream with {peer} closed");
						targets.closed(&peer);
					},
					Some(ValidateInboundSubstream { result_tx, .. }) => {
						// TODO: any peer can take the inbound slots signers need. Accept only peers
						// whose authority discovery keys are in the collator set: look them up with
						// `get_authority_ids_by_peer_id` and answer `result_tx` after the lookup. A
						// collator not discovered yet would be refused until the next lookup.
						let _ = result_tx.send(ValidationResult::Accept);
					},
					None => {
						log::warn!(target: LOG_TARGET, "Notification stream ended, stopping");
						// TODO: can we handle this without stopping the service?
						return;
					},
				}
			},
		}
	}
}
