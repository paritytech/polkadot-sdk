use log::warn;
use std::time::Duration;

const LOG_TARGET: &str = "price-oracle::fetcher";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub struct PriceFetcher {
	client: reqwest::Client,
}

impl PriceFetcher {
	pub fn new() -> Self {
		let client = reqwest::Client::builder()
			.timeout(REQUEST_TIMEOUT)
			.build()
			.expect("Failed to build HTTP client; qed");
		Self { client }
	}

	/// Fetch raw HTTP response bytes from a single endpoint.
	pub async fn fetch_raw(&self, url: &str) -> Result<Vec<u8>, String> {
		self.client
			.get(url)
			.send()
			.await
			.map_err(|e| format!("Request to {} failed: {}", url, e))?
			.bytes()
			.await
			.map(|b| b.to_vec())
			.map_err(|e| format!("Reading body from {} failed: {}", url, e))
	}

	/// Fetch raw response bytes from multiple endpoints.
	/// Returns `(endpoint_id, raw_bytes)` for each successful fetch.
	pub async fn fetch_all(&self, endpoints: &[(u8, String)]) -> Vec<(u8, Vec<u8>)> {
		let mut results = Vec::new();
		for (id, url) in endpoints {
			match self.fetch_raw(url).await {
				Ok(bytes) => results.push((*id, bytes)),
				Err(e) => warn!(target: LOG_TARGET, "Endpoint {} ({}) failed: {}", id, url, e),
			}
		}
		results
	}
}

/// Live tests — run with:
/// ```bash
/// cargo test -p polkadot-node-price-oracle --features live-test live_
/// ```
#[cfg(all(test, feature = "live-test"))]
mod live_tests {
	use super::*;

	fn curl_get(url: &str) -> Vec<u8> {
		let output = std::process::Command::new("curl")
			.args(["-s", "--fail", "--max-time", "15", url])
			.output()
			.expect("curl not installed");
		assert!(
			output.status.success(),
			"curl to {url} failed: {}",
			String::from_utf8_lossy(&output.stderr)
		);
		output.stdout
	}

	#[test]
	fn live_binance() {
		let url = "https://data-api.binance.vision/api/v3/ticker/price?symbol=DOTUSDT";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_binance(&body)
			.expect("Binance response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_coinlore() {
		let url = "https://api.coinlore.net/api/ticker/?id=45219";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_coinlore(&body)
			.expect("CoinLore response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_cryptocompare() {
		let url = "https://min-api.cryptocompare.com/data/price?fsym=DOT&tsyms=USD";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_cryptocompare(&body)
			.expect("CryptoCompare response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_coingecko() {
		let url = "https://api.coingecko.com/api/v3/simple/price?ids=polkadot&vs_currencies=usd";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_coingecko(&body)
			.expect("CoinGecko response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_coinpaprika() {
		let url = "https://api.coinpaprika.com/v1/tickers/dot-polkadot";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_coinpaprika(&body)
			.expect("CoinPaprika response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_dia() {
		let url = "https://api.diadata.org/v1/assetQuotation/Polkadot/0x0000000000000000000000000000000000000000";
		let body = curl_get(url);
		let price =
			pallet_price_oracle::decoders::decode_dia(&body).expect("Dia response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_coinbase() {
		let url = "https://api.coinbase.com/v2/prices/DOT-USD/spot";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_coinbase(&body)
			.expect("Coinbase response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_kraken() {
		let url = "https://api.kraken.com/0/public/Ticker?pair=DOTUSD";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_kraken(&body)
			.expect("Kraken response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_okx() {
		let url = "https://www.okx.com/api/v5/market/ticker?instId=DOT-USDT";
		let body = curl_get(url);
		let price =
			pallet_price_oracle::decoders::decode_okx(&body).expect("OKX response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_bybit() {
		let url = "https://api.bybit.com/v5/market/tickers?category=spot&symbol=DOTUSDT";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_bybit(&body)
			.expect("Bybit response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_kucoin() {
		let url = "https://api.kucoin.com/api/v1/market/orderbook/level1?symbol=DOT-USDT";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_kucoin(&body)
			.expect("KuCoin response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_cryptocom() {
		let url = "https://api.crypto.com/v2/public/get-ticker?instrument_name=DOT_USDT";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_cryptocom(&body)
			.expect("Crypto.com response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	#[test]
	fn live_gateio() {
		let url = "https://api.gateio.ws/api/v4/spot/tickers?currency_pair=DOT_USDT";
		let body = curl_get(url);
		let price = pallet_price_oracle::decoders::decode_gateio(&body)
			.expect("Gate.io response format changed");
		assert!(
			price > sp_runtime::FixedU128::from_rational(1, 100) &&
				price < sp_runtime::FixedU128::from_rational(10_000, 1)
		);
	}

	/// Tests fetch_all with multiple endpoints in one call.
	#[tokio::test]
	async fn live_fetch_all_multiple_endpoints() {
		let fetcher = PriceFetcher::new();
		let endpoints = vec![
			(0u8, "https://data-api.binance.vision/api/v3/ticker/price?symbol=DOTUSDT".to_string()),
			(2u8, "https://min-api.cryptocompare.com/data/price?fsym=DOT&tsyms=USD".to_string()),
			(9u8, "https://api.kraken.com/0/public/Ticker?pair=DOTUSD".to_string()),
		];
		let results = fetcher.fetch_all(&endpoints).await;
		// At least one endpoint should succeed
		assert!(!results.is_empty(), "All endpoints failed in fetch_all");
		for (id, bytes) in &results {
			assert!(!bytes.is_empty(), "Empty response from endpoint {}", id);
		}
	}

	/// Tests that fetch_raw returns an error for an unreachable endpoint.
	#[tokio::test]
	async fn live_fetch_raw_invalid_url() {
		let fetcher = PriceFetcher::new();
		let result = fetcher.fetch_raw("http://localhost:1/nonexistent").await;
		assert!(result.is_err());
	}

	/// Tests that fetch_all gracefully skips failing endpoints.
	#[tokio::test]
	async fn live_fetch_all_skips_failures() {
		let fetcher = PriceFetcher::new();
		let endpoints = vec![
			(99u8, "http://localhost:1/nonexistent".to_string()),
			(0u8, "https://data-api.binance.vision/api/v3/ticker/price?symbol=DOTUSDT".to_string()),
		];
		let results = fetcher.fetch_all(&endpoints).await;
		// The bad endpoint should be skipped, Binance should succeed
		assert_eq!(results.len(), 1);
		assert_eq!(results[0].0, 0);
	}
}
