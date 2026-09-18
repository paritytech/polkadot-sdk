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

//! Fetch and price the markets of [`pallet_price_oracle::venues`] through the node [`Fetcher`].

use crate::{
	fetcher::{Fetcher, MarketResponses},
	now_ms, LOG_TARGET,
};
use futures::{channel::mpsc, StreamExt};
use pallet_price_oracle::{
	price_market,
	pricing::{self, parse_decimal, PairSettings},
	registry::StoredMarket,
	venues,
};
use sp_price_oracle::{
	market::{MarketId, VenueId},
	PairId, Price,
};
use sp_runtime::Permill;
use tokio::time::{Duration, Instant};

const PAIR: PairId = PairId(1);

fn p(s: &str) -> Price {
	parse_decimal(s).unwrap()
}

fn settings() -> PairSettings {
	PairSettings {
		max_spread: Permill::from_percent(1),
		max_trade_age_ms: 5 * 60 * 1_000,
		impact_size: p("10000"),
	}
}

/// One market per constructor in [`venues`], tagged with the constructor name.
fn markets() -> Vec<(&'static str, StoredMarket)> {
	[
		("binance_spot", venues::binance_spot(VenueId(0), PAIR, "DOTUSDT")),
		("binance_perp", venues::binance_perp(VenueId(1), PAIR, "DOTUSDT")),
		("okx_spot", venues::okx_spot(VenueId(2), PAIR, "DOT-USDT")),
		("okx_perp", venues::okx_perp(VenueId(3), PAIR, "DOT-USDT-SWAP", p("1"))),
		("bybit_spot", venues::bybit_spot(VenueId(4), PAIR, "DOTUSDT")),
		("bybit_perp", venues::bybit_perp(VenueId(5), PAIR, "DOTUSDT")),
		("mexc_spot", venues::mexc_spot(VenueId(6), PAIR, "DOTUSDT")),
		("mexc_perp", venues::mexc_perp(VenueId(7), PAIR, "DOT_USDT", p("0.1"))),
		("kucoin_spot", venues::kucoin_spot(VenueId(8), PAIR, "DOT-USDT")),
		("kucoin_perp", venues::kucoin_perp(VenueId(9), PAIR, "DOTUSDTM", p("1"))),
		("gate_spot", venues::gate_spot(VenueId(10), PAIR, "DOT_USDT")),
		("gate_perp", venues::gate_perp(VenueId(11), PAIR, "DOT_USDT", p("1"))),
		("bitget_perp", venues::bitget_perp(VenueId(12), PAIR, "DOTUSDT")),
		("coinbase_spot", venues::coinbase_spot(VenueId(13), PAIR, "DOT-USD")),
		("kraken_perp", venues::kraken_perp(VenueId(14), PAIR, "PF_DOTUSD")),
	]
	.into_iter()
	.map(|(name, market)| (name, market.unwrap()))
	.collect()
}

fn name_of(stored: &[(&'static str, StoredMarket)], id: MarketId) -> &'static str {
	stored[id.0 as usize].0
}

/// Price a completed fetch. Logs response sizes and whether the market priced.
fn price_fetched(
	stored: &[(&'static str, StoredMarket)],
	fetched: MarketResponses,
) -> Option<(VenueId, PairId, Price)> {
	let (name, market) = &stored[fetched.market.0 as usize];
	fetched.responses.iter().for_each(|(tag, body)| {
		log::debug!(target: LOG_TARGET, "{name} query {tag:?}: {} bytes", body.len());
	});
	match price_market(market, &settings(), fetched.responses, now_ms()) {
		Ok(price) => {
			log::debug!(target: LOG_TARGET, "{name} priced at {price}");
			Some((market.venue, market.pair, price))
		},
		Err(e) => {
			log::debug!(
				target: LOG_TARGET,
				"{name} not priced: {}",
				String::from_utf8_lossy(&e.0),
			);
			None
		},
	}
}

#[tokio::test]
async fn venues_are_fetched_and_priced() {
	sp_tracing::init_for_tests();
	let fetcher = Fetcher::new().unwrap();
	let stored = markets();
	let wire: Vec<_> = stored
		.iter()
		.enumerate()
		.map(|(i, (_, market))| market.clone().to_wire(MarketId(i as u32)))
		.collect();

	let deadline = Instant::now() + Duration::from_secs(2);
	let (tx, rx) = mpsc::unbounded();
	let fetch = fetcher.fetch_markets(&wire, deadline, tx);
	let parse = async {
		let fetched = rx.collect::<Vec<_>>().await;
		fetched.into_iter().filter_map(|r| price_fetched(&stored, r)).collect::<Vec<_>>()
	};
	let (failures, prices) = futures::join!(fetch, parse);
	failures.iter().for_each(|(id, failure)| {
		log::debug!(target: LOG_TARGET, "{} not fetched: {failure:?}", name_of(&stored, *id));
	});
	log::debug!(target: LOG_TARGET, "Priced {} of {} markets", prices.len(), wire.len());

	let quotes = pricing::aggregate(prices.clone(), &[PAIR], |_| Vec::new());
	quotes.iter().for_each(|q| {
		log::info!(target: LOG_TARGET, "aggregated price {}", q.price);
	});

	assert!(!prices.is_empty(), "no market could be fetched and priced");
}
