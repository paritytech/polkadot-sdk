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

use crate::ocw_log;
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_system::{
	offchain::{SendSignedTransaction, Signer},
	pallet_prelude::BlockNumberFor,
};
use scale_info::TypeInfo;
use sp_core::Get;
use sp_runtime::{offchain::Duration, traits::Zero, FixedU128};

pub(crate) struct OracleOffchainWorker<T>(core::marker::PhantomData<T>);

#[derive(Debug)]
#[allow(dead_code)] // we want the unused inner values for now to debug, but someday can all be removed.
pub(crate) enum OffchainError {
	AssetNotFound,
	CannotSign,
	TimedOut,
	HttpError(sp_runtime::offchain::http::Error),
	CoreHttpError(sp_core::offchain::HttpError),
	UnexpectedStatusCode(u16),
	ParseError(serde_json::Error),
	InvalidEndpoint,
	Other(&'static str),
}

impl From<&'static str> for OffchainError {
	fn from(e: &'static str) -> Self {
		OffchainError::Other(e)
	}
}

/// Different endpoint parsing methods.
#[derive(
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Debug,
	Clone,
	Eq,
	PartialEq,
	Default,
	MaxEncodedLen,
)]
pub enum ParsingMethod {
	CryptoCompare,
	Binance,
	Kraken,
	Coingecko,
}

impl<T: crate::oracle::Config> OracleOffchainWorker<T> {
	pub(crate) fn offchain_worker(
		local_block_number: BlockNumberFor<T>,
	) -> Result<u32, OffchainError> {
		// TODO: better error handling and return type.
		if local_block_number % T::PriceUpdateInterval::get() != Zero::zero() {
			return Ok(0);
		}

		ocw_log!(info, "Offchain worker starting at #{:?}", local_block_number);
		let keystore_accounts =
			Signer::<T, T::AuthorityId>::keystore_accounts().collect::<Vec<_>>();
		for account in keystore_accounts.iter() {
			ocw_log!(
				info,
				"Account: {:?} / {:?} / {:?}",
				account.id,
				account.public,
				account.index
			);
		}
		let signer = Signer::<T, T::AuthorityId>::all_accounts();
		if !signer.can_sign() {
			ocw_log!(error, "cannot sign!");
			return Err(OffchainError::CannotSign);
		}

		let random_u8 = sp_io::offchain::random_seed()[0];
		let mut assets_updated = 0;
		for (asset_id, endpoints) in crate::oracle::StorageManager::<T>::tracked_assets_with_feeds()
		{
			// pick a random endpoint
			let index = random_u8 as usize % endpoints.len();
			let endpoint_raw = &endpoints[index];
			let endpoint =
				core::str::from_utf8(endpoint_raw).map_err(|_| OffchainError::InvalidEndpoint)?;
			match Self::fetch_price(endpoint) {
				Ok(price) => {
					ocw_log!(info, "fetched price: {:?} for asset {:?}", price, asset_id);

					let call = crate::oracle::Call::<T>::vote {
						asset_id,
						price,
						produced_in: local_block_number,
					};
					let res = signer
						.send_single_signed_transaction(keystore_accounts.first().unwrap(), call);
					ocw_log!(info, "submitted, result is {:?}", res);
					assets_updated += 1;
				},
				Err(e) => {
					ocw_log!(error, "Error fetching price: {:?}", e);
				},
			};
		}

		Ok(assets_updated)
	}

	fn fetch_price(endpoint: &str) -> Result<FixedU128, OffchainError> {
		// send request with deadline.
		let deadline = sp_io::offchain::timestamp().add(Duration::from_millis(2_000));
		let request = sp_runtime::offchain::http::Request::get(&endpoint);
		let pending = request.deadline(deadline).send().map_err(OffchainError::CoreHttpError)?;

		// wait til response is ready or timed out.
		let response = pending
			.try_wait(deadline)
			.map_err(|_pending_request| OffchainError::TimedOut)?
			.map_err(OffchainError::HttpError)?;

		// check status code.
		if response.code != 200 {
			return Err(OffchainError::UnexpectedStatusCode(response.code));
		}

		// extract response body.
		let body = response.body().collect::<Vec<u8>>();
		Self::parse_price(body)
	}

	fn parse_price(body: Vec<u8>) -> Result<FixedU128, OffchainError> {
		log::debug!(target: "runtime::price-oracle::offchain", "body: {:?}", body);
		let v: serde_json::Value =
			serde_json::from_slice(&body).map_err(|e| OffchainError::ParseError(e))?;
		// scenario: https://min-api.cryptocompare.com/data/price?fsym=DOT&tsyms=USD
		match v {
			serde_json::Value::Object(obj) if obj.contains_key("USD") => {
				log::debug!(target: "runtime::price-oracle::offchain", "obj: {:?}", obj);
				use alloc::string::ToString;
				let price_str =
					obj["USD"].as_number().map(|n| n.to_string()).ok_or("failed to parse")?;
				log::debug!(target: "runtime::price-oracle::offchain", "price_str: {:?}", price_str);
				let price = FixedU128::from_float_str(&price_str).map_err(OffchainError::Other)?;
				Ok(price)
			},
			_ => Err(OffchainError::Other("bad json")),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_price_works() {
		let test_data = vec![
			(b"{\"USD\": 100.00}".to_vec(), FixedU128::from_rational(100, 1)),
			(b"{\"USD\": 100.01}".to_vec(), FixedU128::from_rational(10001, 100)),
			(b"{\"USD\": 42.01}".to_vec(), FixedU128::from_rational(4201, 100)),
			(b"{\"USD\": 0.01}".to_vec(), FixedU128::from_rational(1, 100)),
			(b"{\"USD\": .01}".to_vec(), FixedU128::from_rational(1, 100)),
		];

		todo!();
	}

	#[test]
	fn cryptocompare_work() {
		todo!();
	}
}
