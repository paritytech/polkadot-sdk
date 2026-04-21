//! Price feed endpoint decoders.
//!
//! Each public function takes raw HTTP response bytes from a specific price API
//! and extracts the DOT/USD price as `FixedU128`. These are `no_std`-compatible
//! (using `alloc` + `serde_json` with the `alloc` feature).

extern crate alloc;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::str::FromStr;
use scale_info::TypeInfo;
use sp_runtime::FixedU128;

/// Identifies which decoder to apply to a raw HTTP response body.
///
/// Each variant corresponds to a specific price-feed API's response format.
/// This lives inside the pallet; the outside world (runtime API, extrinsic
/// input, node) refers to each variant by its `u8` id via the
/// [`From<ParsingMethod> for u8`] and [`TryFrom<u8>`] conversions below.
#[derive(
	Debug,
	Clone,
	Copy,
	PartialEq,
	Eq,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	TypeInfo,
)]
pub enum ParsingMethod {
	Binance = 0,
	CoinLore = 1,
	CryptoCompare = 2,
	CoinGecko = 3,
	CoinMarketCap = 4,
	CoinPaprika = 5,
	LiveCoinWatch = 6,
	Dia = 7,
	Coinbase = 8,
	Kraken = 9,
	Okx = 10,
	Bybit = 11,
	KuCoin = 12,
	CryptoCom = 13,
	GateIo = 14,
}

impl From<ParsingMethod> for u8 {
	fn from(m: ParsingMethod) -> u8 {
		m as u8
	}
}

impl TryFrom<u8> for ParsingMethod {
	type Error = ();

	fn try_from(v: u8) -> Result<Self, Self::Error> {
		match v {
			0 => Ok(ParsingMethod::Binance),
			1 => Ok(ParsingMethod::CoinLore),
			2 => Ok(ParsingMethod::CryptoCompare),
			3 => Ok(ParsingMethod::CoinGecko),
			4 => Ok(ParsingMethod::CoinMarketCap),
			5 => Ok(ParsingMethod::CoinPaprika),
			6 => Ok(ParsingMethod::LiveCoinWatch),
			7 => Ok(ParsingMethod::Dia),
			8 => Ok(ParsingMethod::Coinbase),
			9 => Ok(ParsingMethod::Kraken),
			10 => Ok(ParsingMethod::Okx),
			11 => Ok(ParsingMethod::Bybit),
			12 => Ok(ParsingMethod::KuCoin),
			13 => Ok(ParsingMethod::CryptoCom),
			14 => Ok(ParsingMethod::GateIo),
			_ => Err(()),
		}
	}
}

/// Dispatch a raw response body to the decoder identified by `method`.
pub fn decode(method: ParsingMethod, body: &[u8]) -> Option<FixedU128> {
	match method {
		ParsingMethod::Binance => decode_binance(body),
		ParsingMethod::CoinLore => decode_coinlore(body),
		ParsingMethod::CryptoCompare => decode_cryptocompare(body),
		ParsingMethod::CoinGecko => decode_coingecko(body),
		ParsingMethod::CoinMarketCap => decode_coinmarketcap(body),
		ParsingMethod::CoinPaprika => decode_coinpaprika(body),
		ParsingMethod::LiveCoinWatch => decode_livecoinwatch(body),
		ParsingMethod::Dia => decode_dia(body),
		ParsingMethod::Coinbase => decode_coinbase(body),
		ParsingMethod::Kraken => decode_kraken(body),
		ParsingMethod::Okx => decode_okx(body),
		ParsingMethod::Bybit => decode_bybit(body),
		ParsingMethod::KuCoin => decode_kucoin(body),
		ParsingMethod::CryptoCom => decode_cryptocom(body),
		ParsingMethod::GateIo => decode_gateio(body),
	}
}

/// Decode a raw response body given a `u8` parsing method id.
///
/// Returns `None` if the id does not map to a known [`ParsingMethod`] or if
/// the body cannot be parsed.
pub fn decode_by_id(id: u8, body: &[u8]) -> Option<FixedU128> {
	ParsingMethod::try_from(id).ok().and_then(|m| decode(m, body))
}

/// Binance: `{"symbol":"DOTUSDT","price":"4.20600000"}`
pub fn decode_binance(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("price")?.as_str()?;
	FixedU128::from_str(s).ok()
}

/// CoinLore: `[{"id":"45219", ..., "price_usd":"4.20", ...}]`
pub fn decode_coinlore(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.as_array()?.first()?.get("price_usd")?.as_str()?;
	FixedU128::from_str(s).ok()
}

/// CryptoCompare: `{"USD":4.202}`
pub fn decode_cryptocompare(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("USD")?.as_number()?.as_str();
	FixedU128::from_str(n).ok()
}

/// CoinGecko: `{"polkadot":{"usd":4.20}}`
pub fn decode_coingecko(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("polkadot")?.get("usd")?.as_number()?.as_str();
	FixedU128::from_str(n).ok()
}

/// CoinMarketCap: `{"data":[{"quote":[{"price":4.20}]}]}`
pub fn decode_coinmarketcap(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("data")?.as_array()?.first()?.get("quote")?.as_array()?.first()?.get("price")?.as_number()?.as_str();
	FixedU128::from_str(n).ok()
}

/// CoinPaprika: `{"quotes":{"USD":{"price":4.20}}}`
pub fn decode_coinpaprika(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("quotes")?.get("USD")?.get("price")?.as_number()?.as_str();
	FixedU128::from_str(n).ok()
}

/// LiveCoinWatch: `{"rate":4.20}`
pub fn decode_livecoinwatch(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("rate")?.as_number()?.as_str();
	FixedU128::from_str(n).ok()
}

/// Dia: `{"Price":4.20}`
pub fn decode_dia(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("Price")?.as_number()?.as_str();
	FixedU128::from_str(n).ok()
}

/// Coinbase: `{"data":{"amount":"4.20"}}`
pub fn decode_coinbase(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("data")?.get("amount")?.as_str()?;
	FixedU128::from_str(s).ok()
}

/// Kraken: `{"result":{"DOTUSD":{"c":["4.20"]}}}`
pub fn decode_kraken(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("result")?.get("DOTUSD")?.get("c")?.as_array()?.first()?.as_str()?;
	FixedU128::from_str(s).ok()
}

/// OKX: `{"data":[{"last":"4.20"}]}`
pub fn decode_okx(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("data")?.as_array()?.first()?.get("last")?.as_str()?;
	FixedU128::from_str(s).ok()
}

/// Bybit: `{"result":{"list":[{"lastPrice":"4.20"}]}}`
pub fn decode_bybit(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v
		.get("result")?
		.get("list")?
		.as_array()?
		.first()?
		.get("lastPrice")?
		.as_str()?;
	FixedU128::from_str(s).ok()
}

/// KuCoin: `{"data":{"price":"4.20"}}`
pub fn decode_kucoin(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("data")?.get("price")?.as_str()?;
	FixedU128::from_str(s).ok()
}

/// Crypto.com: `{"result":{"data":[{"a":"4.20"}]}}`
pub fn decode_cryptocom(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("result")?.get("data")?.as_array()?.first()?.get("a")?.as_str()?;
	FixedU128::from_str(s).ok()
}

/// Gate.io: `[{"last":"4.20"}]`
pub fn decode_gateio(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.as_array()?.first()?.get("last")?.as_str()?;
	FixedU128::from_str(s).ok()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn assert_plausible(price: FixedU128) {
		assert!(
			price > FixedU128::from_u32(0) && price < FixedU128::from_u32(10_000),
			"Implausible price: {:?}",
			price
		);
	}

	#[test]
	fn binance() {
		let body = br#"{"symbol":"DOTUSDT","price":"4.20600000"}"#;
		let price = decode_binance(body).unwrap();
		assert_plausible(price);

		assert!(decode_binance(br#"{"symbol":"DOTUSDT"}"#).is_none());
		assert!(decode_binance(br#"{"price":123}"#).is_none()); // not a string
	}

	#[test]
	fn coinlore() {
		let body = br#"[{"id":"45219","price_usd":"4.20"}]"#;
		let price = decode_coinlore(body).unwrap();
		assert_plausible(price);

		assert!(decode_coinlore(br#"[{"id":"45219"}]"#).is_none());
		assert!(decode_coinlore(br#"[]"#).is_none());
	}

	#[test]
	fn cryptocompare() {
		let body = br#"{"USD":4.202}"#;
		let price = decode_cryptocompare(body).unwrap();
		assert_plausible(price);

		assert!(decode_cryptocompare(br#"{"EUR":1.5}"#).is_none());
	}

	#[test]
	fn coingecko() {
		let body = br#"{"polkadot":{"usd":4.20}}"#;
		let price = decode_coingecko(body).unwrap();
		assert_plausible(price);

		assert!(decode_coingecko(br#"{"polkadot":{}}"#).is_none());
	}

	#[test]
	fn coinmarketcap() {
		let body = br#"{"data":[{"quote":[{"price":4.20}]}]}"#;
		let price = decode_coinmarketcap(body).unwrap();
		assert_plausible(price);

		assert!(decode_coinmarketcap(br#"{"data":[]}"#).is_none());
	}

	#[test]
	fn coinpaprika() {
		let body = br#"{"quotes":{"USD":{"price":4.20}}}"#;
		let price = decode_coinpaprika(body).unwrap();
		assert_plausible(price);

		assert!(decode_coinpaprika(br#"{"quotes":{}}"#).is_none());
	}

	#[test]
	fn livecoinwatch() {
		let body = br#"{"rate":4.20}"#;
		let price = decode_livecoinwatch(body).unwrap();
		assert_plausible(price);

		assert!(decode_livecoinwatch(br#"{"notrate":4.20}"#).is_none());
	}

	#[test]
	fn dia() {
		let body = br#"{"Price":4.20}"#;
		let price = decode_dia(body).unwrap();
		assert_plausible(price);

		assert!(decode_dia(br#"{"price":4.20}"#).is_none()); // case sensitive
	}

	#[test]
	fn coinbase() {
		let body = br#"{"data":{"amount":"4.20"}}"#;
		let price = decode_coinbase(body).unwrap();
		assert_plausible(price);

		assert!(decode_coinbase(br#"{"data":{}}"#).is_none());
	}

	#[test]
	fn kraken() {
		let body = br#"{"result":{"DOTUSD":{"c":["4.20","lot"]}}}"#;
		let price = decode_kraken(body).unwrap();
		assert_plausible(price);

		assert!(decode_kraken(br#"{"result":{"DOTUSD":{"c":[]}}}"#).is_none());
	}

	#[test]
	fn okx() {
		let body = br#"{"data":[{"last":"4.20"}]}"#;
		let price = decode_okx(body).unwrap();
		assert_plausible(price);

		assert!(decode_okx(br#"{"data":[]}"#).is_none());
	}

	#[test]
	fn bybit() {
		let body = br#"{"result":{"list":[{"lastPrice":"4.20"}]}}"#;
		let price = decode_bybit(body).unwrap();
		assert_plausible(price);

		assert!(decode_bybit(br#"{"result":{"list":[]}}"#).is_none());
	}

	#[test]
	fn kucoin() {
		let body = br#"{"data":{"price":"4.20"}}"#;
		let price = decode_kucoin(body).unwrap();
		assert_plausible(price);

		assert!(decode_kucoin(br#"{"data":{}}"#).is_none());
	}

	#[test]
	fn cryptocom() {
		let body = br#"{"result":{"data":[{"a":"4.20"}]}}"#;
		let price = decode_cryptocom(body).unwrap();
		assert_plausible(price);

		assert!(decode_cryptocom(br#"{"result":{"data":[]}}"#).is_none());
	}

	#[test]
	fn gateio() {
		let body = br#"[{"last":"4.20"}]"#;
		let price = decode_gateio(body).unwrap();
		assert_plausible(price);

		assert!(decode_gateio(br#"[]"#).is_none());
	}

	#[test]
	fn parsing_method_u8_roundtrip() {
		let all = [
			ParsingMethod::Binance,
			ParsingMethod::CoinLore,
			ParsingMethod::CryptoCompare,
			ParsingMethod::CoinGecko,
			ParsingMethod::CoinMarketCap,
			ParsingMethod::CoinPaprika,
			ParsingMethod::LiveCoinWatch,
			ParsingMethod::Dia,
			ParsingMethod::Coinbase,
			ParsingMethod::Kraken,
			ParsingMethod::Okx,
			ParsingMethod::Bybit,
			ParsingMethod::KuCoin,
			ParsingMethod::CryptoCom,
			ParsingMethod::GateIo,
		];
		for m in all {
			let id: u8 = m.into();
			assert_eq!(ParsingMethod::try_from(id).unwrap(), m);
		}
		assert!(ParsingMethod::try_from(15u8).is_err());
		assert!(ParsingMethod::try_from(u8::MAX).is_err());
	}

	#[test]
	fn decode_by_id_dispatches() {
		let body = br#"{"symbol":"DOTUSDT","price":"4.20"}"#;
		assert!(decode_by_id(u8::from(ParsingMethod::Binance), body).is_some());
		assert!(decode_by_id(99, body).is_none());
	}
}
