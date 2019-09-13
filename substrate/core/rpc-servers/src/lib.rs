// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate RPC servers.

#[warn(missing_docs)]

use std::io;
use jsonrpc_core::IoHandlerExtension;
use log::error;
use pubsub::PubSubMetadata;

/// Maximal payload accepted by RPC servers.
const MAX_PAYLOAD: usize = 15 * 1024 * 1024;

/// Default maximum number of connections for WS RPC servers.
const WS_MAX_CONNECTIONS: usize = 100;

/// The RPC IoHandler containing all requested APIs.
pub type RpcHandler<T> = pubsub::PubSubHandler<T>;

pub use self::inner::*;

/// Construct rpc `IoHandler`
pub fn rpc_handler<M: PubSubMetadata>(
	extension: impl IoHandlerExtension<M>
) -> RpcHandler<M> {
	let mut io = pubsub::PubSubHandler::default();
	extension.augment(&mut io);

	// add an endpoint to list all available methods.
	let mut methods = io.iter().map(|x| x.0.clone()).collect::<Vec<String>>();
	io.add_method("rpc_methods", {
		methods.sort();
		let methods = serde_json::to_value(&methods)
			.expect("Serialization of Vec<String> is infallible; qed");

		move |_| Ok(serde_json::json!({
			"version": 1,
			"methods": methods.clone(),
		}))
	});
	io
}

#[cfg(not(target_os = "unknown"))]
mod inner {
	use super::*;

	pub type HttpServer = http::Server;
	pub type WsServer = ws::Server;

	/// Start HTTP server listening on given address.
	///
	/// **Note**: Only available if `not(target_os = "unknown")`.
	pub fn start_http<M: pubsub::PubSubMetadata + Default>(
		addr: &std::net::SocketAddr,
		cors: Option<&Vec<String>>,
		io: RpcHandler<M>,
	) -> io::Result<http::Server> {
		http::ServerBuilder::new(io)
			.threads(4)
			.health_api(("/health", "system_health"))
			.allowed_hosts(hosts_filtering(cors.is_some()))
			.rest_api(if cors.is_some() {
				http::RestApi::Secure
			} else {
				http::RestApi::Unsecure
			})
			.cors(map_cors::<http::AccessControlAllowOrigin>(cors))
			.max_request_body_size(MAX_PAYLOAD)
			.start_http(addr)
	}

	/// Start WS server listening on given address.
	///
	/// **Note**: Only available if `not(target_os = "unknown")`.
	pub fn start_ws<M: pubsub::PubSubMetadata + From<jsonrpc_core::futures::sync::mpsc::Sender<String>>> (
		addr: &std::net::SocketAddr,
		max_connections: Option<usize>,
		cors: Option<&Vec<String>>,
		io: RpcHandler<M>,
	) -> io::Result<ws::Server> {
		ws::ServerBuilder::with_meta_extractor(io, |context: &ws::RequestContext| context.sender().into())
			.max_payload(MAX_PAYLOAD)
			.max_connections(max_connections.unwrap_or(WS_MAX_CONNECTIONS))
			.allowed_origins(map_cors(cors))
			.allowed_hosts(hosts_filtering(cors.is_some()))
			.start(addr)
			.map_err(|err| match err {
				ws::Error::Io(io) => io,
				ws::Error::ConnectionClosed => io::ErrorKind::BrokenPipe.into(),
				e => {
					error!("{}", e);
					io::ErrorKind::Other.into()
				}
			})
	}

	fn map_cors<T: for<'a> From<&'a str>>(
		cors: Option<&Vec<String>>
	) -> http::DomainsValidation<T> {
		cors.map(|x| x.iter().map(AsRef::as_ref).map(Into::into).collect::<Vec<_>>()).into()
	}

	fn hosts_filtering(enable: bool) -> http::DomainsValidation<http::Host> {
		if enable {
			// NOTE The listening address is whitelisted by default.
			// Setting an empty vector here enables the validation
			// and allows only the listening address.
			http::DomainsValidation::AllowOnly(vec![])
		} else {
			http::DomainsValidation::Disabled
		}
	}
}

#[cfg(target_os = "unknown")]
mod inner {
}
