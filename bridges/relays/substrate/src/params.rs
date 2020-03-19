// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

use crate::error::Error;

use std::str::FromStr;
use url::Url;

const DEFAULT_WS_PORT: u16 = 9944;

#[derive(Debug, Clone)]
pub struct Params {
	pub base_path: String,
	pub rpc_urls: Vec<RPCUrlParam>,
}

#[derive(Debug, Clone)]
pub struct RPCUrlParam {
	url: Url,
}

impl ToString for RPCUrlParam {
	fn to_string(&self) -> String {
		self.url.to_string()
	}
}

impl FromStr for RPCUrlParam {
	type Err = Error;

	fn from_str(url_str: &str) -> Result<Self, Self::Err> {
		let mut url =
			Url::parse(url_str).map_err(|e| Error::UrlError(format!("could not parse {}: {}", url_str, e)))?;

		if url.scheme() != "ws" {
			return Err(Error::UrlError(format!("must have scheme ws, found {}", url.scheme())));
		}

		if url.port().is_none() {
			url.set_port(Some(DEFAULT_WS_PORT))
				.expect("the scheme is checked above to be ws; qed");
		}

		if url.path() != "/" {
			return Err(Error::UrlError(format!("cannot have a path, found {}", url.path())));
		}
		if let Some(query) = url.query() {
			return Err(Error::UrlError(format!("cannot have a query, found {}", query)));
		}
		if let Some(fragment) = url.fragment() {
			return Err(Error::UrlError(format!("cannot have a fragment, found {}", fragment)));
		}

		Ok(RPCUrlParam { url })
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rpc_url_from_str() {
		assert_eq!(
			RPCUrlParam::from_str("ws://127.0.0.1").unwrap().to_string(),
			"ws://127.0.0.1:9944/"
		);
		assert_eq!(
			RPCUrlParam::from_str("ws://127.0.0.1/").unwrap().to_string(),
			"ws://127.0.0.1:9944/"
		);
		assert_eq!(
			RPCUrlParam::from_str("ws://127.0.0.1:4499").unwrap().to_string(),
			"ws://127.0.0.1:4499/"
		);
		assert!(RPCUrlParam::from_str("http://127.0.0.1").is_err());
	}
}
