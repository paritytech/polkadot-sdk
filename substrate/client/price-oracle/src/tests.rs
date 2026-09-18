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

//! Live fetches of every venue constructor through [`Fetcher::fetch_markets`].
//!
//! Hits public exchange APIs. Run with `--nocapture` to see per-venue sizes and prices:
//! ```text
//! cargo test -p sc-price-oracle --lib fetch_and_price -- --nocapture
//! ```

use crate::fetcher::Fetcher;
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
use std::{
	collections::BTreeMap,
	time::{SystemTime, UNIX_EPOCH},
};
use tokio::time::{Duration, Instant};

const PAIR: PairId = PairId(1);

/// Exchange-native symbols of [`PAIR`] on each venue constructor.
struct Listing {
	binance: &'static str,
	okx_spot: &'static str,
	okx_perp: &'static str,
	bybit: &'static str,
	mexc_spot: &'static str,
	mexc_perp: &'static str,
	kucoin_spot: &'static str,
	kucoin_perp: &'static str,
	gate: &'static str,
	bitget: &'static str,
	coinbase: &'static str,
	kraken_perp: &'static str,
}

const LISTING: Listing = Listing {
	binance: "DOTUSDT",
	okx_spot: "DOT-USDT",
	okx_perp: "DOT-USDT-SWAP",
	bybit: "DOTUSDT",
	mexc_spot: "DOTUSDT",
	mexc_perp: "DOT_USDT",
	kucoin_spot: "DOT-USDT",
	kucoin_perp: "DOTUSDTM",
	gate: "DOT_USDT",
	bitget: "DOTUSDT",
	coinbase: "DOT-USD",
	kraken_perp: "PF_DOTUSD",
};

fn now_ms() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn contract(s: &str) -> Price {
	parse_decimal(s).expect("contract size literal")
}

/// Every constructor in [`venues`], using `listing` as the symbol on that exchange.
///
/// In this test example we use DOT markets.
fn markets(pair: PairId, listing: &Listing) -> Vec<(&'static str, StoredMarket)> {
	let mut i = 0u32;
	let add = |name: &'static str, market: Option<StoredMarket>| {
		(name, market.unwrap_or_else(|| panic!("{name} does not fit registry bounds")))
	};
	let mut venue = || {
		let v = VenueId(i);
		i += 1;
		v
	};
	vec![
		add("binance_spot", venues::binance_spot(venue(), pair, listing.binance)),
		add("binance_perp", venues::binance_perp(venue(), pair, listing.binance)),
		add("okx_spot", venues::okx_spot(venue(), pair, listing.okx_spot)),
		add("okx_perp", venues::okx_perp(venue(), pair, listing.okx_perp, contract("1"))),
		add("bybit_spot", venues::bybit_spot(venue(), pair, listing.bybit)),
		add("bybit_perp", venues::bybit_perp(venue(), pair, listing.bybit)),
		add("mexc_spot", venues::mexc_spot(venue(), pair, listing.mexc_spot)),
		add("mexc_perp", venues::mexc_perp(venue(), pair, listing.mexc_perp, contract("0.1"))),
		add("kucoin_spot", venues::kucoin_spot(venue(), pair, listing.kucoin_spot)),
		add("kucoin_perp", venues::kucoin_perp(venue(), pair, listing.kucoin_perp, contract("1"))),
		add("gate_spot", venues::gate_spot(venue(), pair, listing.gate)),
		add("gate_perp", venues::gate_perp(venue(), pair, listing.gate, contract("1"))),
		add("bitget_perp", venues::bitget_perp(venue(), pair, listing.bitget)),
		add("coinbase_spot", venues::coinbase_spot(venue(), pair, listing.coinbase)),
		add("kraken_perp", venues::kraken_perp(venue(), pair, listing.kraken_perp)),
	]
}

#[tokio::test]
async fn fetch_and_price() {
	let fetcher = Fetcher::new().expect("HTTPS client");
	let stored = markets(PAIR, &LISTING);
	let names: BTreeMap<MarketId, &str> = stored
		.iter()
		.enumerate()
		.map(|(i, (name, _))| (MarketId(i as u32), *name))
		.collect();
	let stored_by_id: BTreeMap<MarketId, StoredMarket> = stored
		.into_iter()
		.enumerate()
		.map(|(i, (_, market))| (MarketId(i as u32), market))
		.collect();
	let wire: Vec<_> =
		stored_by_id.iter().map(|(id, market)| market.clone().to_wire(*id)).collect();

	let settings = PairSettings {
		max_spread: Permill::from_percent(1),
		max_trade_age_ms: 5 * 60 * 1_000,
		impact_size: Price::from_u32(10_000),
	};
	let now = now_ms();
	let deadline = Instant::now() + Duration::from_secs(2);
	let (tx, mut rx) = mpsc::unbounded();
	let fetch = fetcher.fetch_markets(&wire, deadline, tx);
	let parse = async {
		let mut prices = Vec::new();
		while let Some(responses) = rx.next().await {
			let id = responses.market;
			let name = names[&id];
			for (tag, body) in &responses.responses {
				println!("{name}  tag {tag:?}  {} bytes", body.len());
			}
			let market = &stored_by_id[&id];
			match price_market(market, &settings, responses.responses, now) {
				Ok(price) => {
					println!("{name}  price {price}");
					prices.push((market.venue, market.pair, price));
				},
				Err(e) => println!("{name}  parse {}", String::from_utf8_lossy(&e.0)),
			}
		}
		prices
	};
	let (failures, prices) = futures::join!(fetch, parse);
	for (id, failure) in &failures {
		println!("{}  fetch {failure:?}", names[id]);
	}

	let quotes = pricing::aggregate(prices.clone(), &[PAIR], |_| Vec::new());
	println!(
		"priced {} of {} markets, {} fetch failures",
		prices.len(),
		wire.len(),
		failures.len(),
	);
	for q in &quotes {
		println!("median {price}", price = q.price);
	}

	assert!(!prices.is_empty(), "no market could be fetched and priced");
}
