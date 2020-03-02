use crate::error::Error;

use url::Url;
use std::str::FromStr;

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
		let mut url = Url::parse(url_str)
			.map_err(|e| Error::UrlError(format!("could not parse {}: {}", url_str, e)))?;

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
