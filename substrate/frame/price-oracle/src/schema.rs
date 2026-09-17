// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

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
//! How to read the responses of a market's queries.
//!
//! A [`ResponseSchema`] describes where in a JSON document the data sits. The interpreter turns a
//! response into an [`OrderBook`] or the latest trade time, rejecting the whole response on
//! any malformed element rather than returning partial data.

use crate::pricing::parse_decimal;
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{traits::ConstU32, BoundedVec};
use scale_info::TypeInfo;
use serde_json::Value;
use sp_price_oracle::Price;
use sp_runtime::traits::Zero;

pub type MaxKey = ConstU32<32>;
pub type MaxPathDepth = ConstU32<8>;

/// One step into a JSON document.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub enum PathStep {
	/// The value under this key of an object.
	Key(BoundedVec<u8, MaxKey>),
	/// The element at this index of an array.
	Index(u32),
}

/// A path into a JSON document, from the root.
pub type Path = BoundedVec<PathStep, MaxPathDepth>;

/// Where the price and amount of one order book level sit.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub enum LevelLayout {
	/// A level is an array; price and amount are at these positions.
	Array { price: u32, amount: u32 },
	/// A level is an object; price and amount are under these keys.
	Object { price: BoundedVec<u8, MaxKey>, amount: BoundedVec<u8, MaxKey> },
}

/// How a trade's timestamp is encoded.
#[derive(
	Clone,
	Copy,
	PartialEq,
	Eq,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub enum TimeFormat {
	/// Unix time in seconds, possibly fractional.
	Seconds,
	/// Unix time in milliseconds.
	Millis,
	/// Unix time in nanoseconds.
	Nanos,
	/// ISO 8601 UTC, e.g. `2026-08-14T12:34:56.789Z`.
	Iso8601,
}

/// How to read a response.
#[derive(
	Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
)]
pub enum ResponseSchema {
	/// An order book.
	OrderBook {
		/// Path to the array of bid levels.
		bids: Path,
		/// Path to the array of ask levels.
		asks: Path,
		/// Layout of one level.
		layout: LevelLayout,
	},
	/// A list of recent trades.
	Trades {
		/// Path to the array of trades.
		trades: Path,
		/// Path from one trade to its timestamp.
		time: Path,
		/// Encoding of the timestamp.
		format: TimeFormat,
	},
}

/// One level of an order book: price and amount, both positive.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Level {
	pub price: Price,
	pub amount: Price,
}

/// An order book with bids sorted best (highest) first and asks best (lowest) first.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct OrderBook {
	pub bids: Vec<Level>,
	pub asks: Vec<Level>,
}

/// What a response was read into.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Parsed {
	OrderBook(OrderBook),
	/// Unix time of the latest trade, in milliseconds.
	LatestTradeMs(u64),
}

/// Why a response could not be read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResponseSchemaError {
	NotJson,
	PathNotFound,
	NotAnArray,
	MalformedLevel,
	EmptySide,
	MalformedTime,
	NoTrades,
}

impl ResponseSchema {
	/// Read `body` according to this schema.
	pub fn read(&self, body: &[u8]) -> Result<Parsed, ResponseSchemaError> {
		let doc: Value = serde_json::from_slice(body).map_err(|_| ResponseSchemaError::NotJson)?;
		match self {
			ResponseSchema::OrderBook { bids, asks, layout } => {
				let mut bids = read_levels(follow(&doc, bids)?, layout)?;
				let mut asks = read_levels(follow(&doc, asks)?, layout)?;
				bids.sort_unstable_by_key(|level| core::cmp::Reverse(level.price));
				asks.sort_unstable_by_key(|level| level.price);
				Ok(Parsed::OrderBook(OrderBook { bids, asks }))
			},
			ResponseSchema::Trades { trades, time, format } => {
				let rows =
					follow(&doc, trades)?.as_array().ok_or(ResponseSchemaError::NotAnArray)?;
				let mut latest = None;
				for row in rows {
					let ms = read_time(
						follow(row, time).map_err(|_| ResponseSchemaError::MalformedTime)?,
						*format,
					)?;
					latest = Some(latest.map_or(ms, |n: u64| n.max(ms)));
				}
				latest.map(Parsed::LatestTradeMs).ok_or(ResponseSchemaError::NoTrades)
			},
		}
	}
}

/// Follow `path` from `value`.
fn follow<'a>(mut value: &'a Value, path: &Path) -> Result<&'a Value, ResponseSchemaError> {
	for step in path.iter() {
		value = match step {
			PathStep::Key(key) => {
				let key =
					core::str::from_utf8(key).map_err(|_| ResponseSchemaError::PathNotFound)?;
				value.get(key)
			},
			PathStep::Index(i) => value.get(*i as usize),
		}
		.ok_or(ResponseSchemaError::PathNotFound)?;
	}
	Ok(value)
}

/// Read all levels of one side. Any malformed level fails the side.
fn read_levels(side: &Value, layout: &LevelLayout) -> Result<Vec<Level>, ResponseSchemaError> {
	let rows = side.as_array().ok_or(ResponseSchemaError::NotAnArray)?;
	if rows.is_empty() {
		return Err(ResponseSchemaError::EmptySide);
	}
	rows.iter()
		.map(|row| {
			let (price, amount) = match layout {
				LevelLayout::Array { price, amount } => {
					(row.get(*price as usize), row.get(*amount as usize))
				},
				LevelLayout::Object { price, amount } => (
					core::str::from_utf8(price).ok().and_then(|k| row.get(k)),
					core::str::from_utf8(amount).ok().and_then(|k| row.get(k)),
				),
			};
			let price = read_number(price.ok_or(ResponseSchemaError::MalformedLevel)?)?;
			let amount = read_number(amount.ok_or(ResponseSchemaError::MalformedLevel)?)?;
			if price.is_zero() || amount.is_zero() {
				return Err(ResponseSchemaError::MalformedLevel);
			}
			Ok(Level { price, amount })
		})
		.collect()
}

/// A non-negative decimal given as a JSON number or as a string.
fn read_number(value: &Value) -> Result<Price, ResponseSchemaError> {
	let text = match value {
		Value::Number(n) => n.as_str(),
		Value::String(s) => s.as_str(),
		_ => return Err(ResponseSchemaError::MalformedLevel),
	};
	parse_decimal(text).ok_or(ResponseSchemaError::MalformedLevel)
}

/// A timestamp in `format`, as Unix milliseconds.
fn read_time(value: &Value, format: TimeFormat) -> Result<u64, ResponseSchemaError> {
	let text = match value {
		Value::Number(n) => n.as_str(),
		Value::String(s) => s.as_str(),
		_ => return Err(ResponseSchemaError::MalformedTime),
	};
	if let TimeFormat::Iso8601 = format {
		return iso8601_ms(text).ok_or(ResponseSchemaError::MalformedTime);
	}
	// Numeric formats: scale to milliseconds through the decimal parser (18 places of precision).
	let value = parse_decimal(text).ok_or(ResponseSchemaError::MalformedTime)?.into_inner();
	const ONE: u128 = 1_000_000_000_000_000_000;
	let ms = match format {
		TimeFormat::Seconds => value / (ONE / 1_000),
		TimeFormat::Millis => value / ONE,
		TimeFormat::Nanos => value / (ONE * 1_000_000),
		TimeFormat::Iso8601 => unreachable!("handled above"),
	};
	u64::try_from(ms).map_err(|_| ResponseSchemaError::MalformedTime)
}

/// Unix milliseconds of an ISO 8601 UTC timestamp `YYYY-MM-DDThh:mm:ss[.fff...]Z`.
fn iso8601_ms(s: &str) -> Option<u64> {
	let b = s.as_bytes();
	if b.len() < 20 ||
		b[4] != b'-' ||
		b[7] != b'-' ||
		b[10] != b'T' ||
		b[13] != b':' ||
		b[16] != b':'
	{
		return None;
	}
	let num = |r: core::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
	let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
	let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
	if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
		return None;
	}
	// Optional fraction, then a mandatory 'Z'.
	let mut i = 19;
	let mut millis = 0u64;
	if b.get(i) == Some(&b'.') {
		i += 1;
		let start = i;
		while i < b.len() && b[i].is_ascii_digit() {
			i += 1;
		}
		let frac = &s[start..i];
		if frac.is_empty() {
			return None;
		}
		let first3 = &frac[..frac.len().min(3)];
		millis = first3.parse::<u64>().ok()? * 10u64.pow(3 - first3.len() as u32);
	}
	if b.get(i) != Some(&b'Z') || i + 1 != b.len() {
		return None;
	}
	// Days from civil date (Howard Hinnant's algorithm).
	let yy = if mo <= 2 { y - 1 } else { y };
	let era = if yy >= 0 { yy } else { yy - 399 } / 400;
	let yoe = yy - era * 400;
	let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
	let days = era * 146_097 + (yoe * 365 + yoe / 4 - yoe / 100 + doy) - 719_468;
	let secs = days * 86_400 + h * 3_600 + mi * 60 + sec;
	u64::try_from(secs).ok()?.checked_mul(1_000)?.checked_add(millis)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::FixedPointNumber;

	fn key(k: &str) -> PathStep {
		PathStep::Key(k.as_bytes().to_vec().try_into().unwrap())
	}
	fn path(steps: Vec<PathStep>) -> Path {
		steps.try_into().unwrap()
	}
	fn p(s: &str) -> Price {
		parse_decimal(s).unwrap()
	}

	fn binance_book() -> ResponseSchema {
		ResponseSchema::OrderBook {
			bids: path(vec![key("bids")]),
			asks: path(vec![key("asks")]),
			layout: LevelLayout::Array { price: 0, amount: 1 },
		}
	}

	#[test]
	fn reads_and_sorts_an_order_book() {
		let body = br#"{"bids":[["3.99","25"],["4.00","10"]],"asks":[["4.02","35"],["4.01",8]]}"#;
		let Parsed::OrderBook(book) = binance_book().read(body).unwrap() else { panic!() };
		assert_eq!(book.bids[0], Level { price: p("4.00"), amount: p("10") });
		assert_eq!(book.bids[1].price, p("3.99"));
		assert_eq!(book.asks[0], Level { price: p("4.01"), amount: p("8") });
	}

	#[test]
	fn nested_path_and_object_levels() {
		let schema = ResponseSchema::OrderBook {
			bids: path(vec![key("result"), key("XDOTUSD"), key("bids")]),
			asks: path(vec![key("result"), key("XDOTUSD"), key("asks")]),
			layout: LevelLayout::Object {
				price: b"p".to_vec().try_into().unwrap(),
				amount: b"q".to_vec().try_into().unwrap(),
			},
		};
		let body =
			br#"{"result":{"XDOTUSD":{"bids":[{"p":"4","q":"1","t":1}],"asks":[{"p":"5","q":"2"}]}}}"#;
		let Parsed::OrderBook(book) = schema.read(body).unwrap() else { panic!() };
		assert_eq!(book.bids[0].price, p("4"));
		assert_eq!(book.asks[0].amount, p("2"));
	}

	#[test]
	fn malformed_level_fails_the_whole_book() {
		assert_eq!(
			binance_book().read(br#"{"bids":[["4.00","10"],["oops"]],"asks":[["4.01","1"]]}"#),
			Err(ResponseSchemaError::MalformedLevel)
		);
		assert_eq!(
			binance_book().read(br#"{"bids":[["0","10"]],"asks":[["4.01","1"]]}"#),
			Err(ResponseSchemaError::MalformedLevel)
		);
		assert_eq!(
			binance_book().read(br#"{"bids":[],"asks":[["4.01","1"]]}"#),
			Err(ResponseSchemaError::EmptySide)
		);
		assert_eq!(binance_book().read(br#"{"asks":[]}"#), Err(ResponseSchemaError::PathNotFound));
		assert_eq!(binance_book().read(b"not json"), Err(ResponseSchemaError::NotJson));
	}

	#[test]
	fn latest_trade_in_each_time_format() {
		let read = |format, body: &[u8], time: Vec<PathStep>| {
			ResponseSchema::Trades { trades: path(vec![]), time: path(time), format }.read(body)
		};
		assert_eq!(
			read(
				TimeFormat::Millis,
				br#"[{"time":100},{"time":300},{"time":200}]"#,
				vec![key("time")]
			),
			Ok(Parsed::LatestTradeMs(300))
		);
		assert_eq!(
			read(TimeFormat::Seconds, br#"[{"t":"1786708800.5"}]"#, vec![key("t")]),
			Ok(Parsed::LatestTradeMs(1_786_708_800_500))
		);
		assert_eq!(
			read(TimeFormat::Nanos, br#"[["DOT",1786708800500000000]]"#, vec![PathStep::Index(1)]),
			Ok(Parsed::LatestTradeMs(1_786_708_800_500))
		);
		assert_eq!(
			read(
				TimeFormat::Iso8601,
				br#"[{"time":"2026-08-14T12:00:00.500Z"}]"#,
				vec![key("time")]
			),
			Ok(Parsed::LatestTradeMs(1_786_708_800_500))
		);
		assert_eq!(
			read(TimeFormat::Millis, br#"[]"#, vec![key("time")]),
			Err(ResponseSchemaError::NoTrades)
		);
		assert_eq!(
			read(TimeFormat::Millis, br#"[{"time":1},{"x":2}]"#, vec![key("time")]),
			Err(ResponseSchemaError::MalformedTime)
		);
	}

	#[test]
	fn iso8601_edge_cases() {
		assert_eq!(iso8601_ms("1970-01-01T00:00:00Z"), Some(0));
		assert_eq!(iso8601_ms("2026-08-14T12:00:00.5Z"), Some(1_786_708_800_500));
		assert_eq!(iso8601_ms("2026-08-14T12:00:00.123456Z"), Some(1_786_708_800_123));
		assert_eq!(iso8601_ms("2026-08-14T12:00:00"), None);
		assert_eq!(iso8601_ms("2026-13-14T12:00:00Z"), None);
		assert_eq!(iso8601_ms("not a date"), None);
	}

	#[test]
	fn price_type_has_expected_precision() {
		assert_eq!(Price::accuracy(), 1_000_000_000_000_000_000);
	}
}
