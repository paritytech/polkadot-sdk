// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use core::time::Duration;
use futures::prelude::*;
use sc_service::SpawnTaskHandle;
use smoldot::libp2p::{websocket, with_buffers};
use smoldot_light::platform::{
	Address, ConnectError, ConnectionType, IpAddr, MultiStreamWebRtcConnection, PlatformRef,
	SubstreamDirection,
};
use std::{net::SocketAddr, pin::Pin, time::Instant};
use tokio::net::TcpStream;

use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};
type CompatTcpStream = Compat<TcpStream>;

/// Platform implementation for tokio
/// This implementation is a port of the implementation for smol:
/// https://github.com/smol-dot/smoldot/blob/8c577b4a753fe96190f813070564ecc742b91a16/light-base/src/platform/default.rs
#[derive(Clone)]
pub struct TokioPlatform {
	spawner: SpawnTaskHandle,
}

impl TokioPlatform {
	pub fn new(spawner: SpawnTaskHandle) -> Self {
		TokioPlatform { spawner }
	}
}

impl PlatformRef for TokioPlatform {
	type Delay = future::BoxFuture<'static, ()>;
	type Instant = Instant;
	type MultiStream = std::convert::Infallible;
	type Stream = Stream;
	type StreamConnectFuture = future::BoxFuture<'static, Result<Self::Stream, ConnectError>>;
	type MultiStreamConnectFuture = future::BoxFuture<
		'static,
		Result<MultiStreamWebRtcConnection<Self::MultiStream>, ConnectError>,
	>;
	type ReadWriteAccess<'a> = with_buffers::ReadWriteAccess<'a>;
	type StreamUpdateFuture<'a> = future::BoxFuture<'a, ()>;
	type StreamErrorRef<'a> = &'a std::io::Error;
	type NextSubstreamFuture<'a> = future::Pending<Option<(Self::Stream, SubstreamDirection)>>;

	fn now_from_unix_epoch(&self) -> Duration {
		// Intentionally panic if the time is configured earlier than the UNIX EPOCH.
		std::time::UNIX_EPOCH.elapsed().unwrap()
	}

	fn now(&self) -> Self::Instant {
		Instant::now()
	}

	fn fill_random_bytes(&self, buffer: &mut [u8]) {
		rand::RngCore::fill_bytes(&mut rand::thread_rng(), buffer);
	}

	fn sleep(&self, duration: Duration) -> Self::Delay {
		tokio::time::sleep(duration).boxed()
	}

	fn sleep_until(&self, when: Self::Instant) -> Self::Delay {
		let duration = when.saturating_duration_since(Instant::now());
		self.sleep(duration)
	}

	fn supports_connection_type(&self, connection_type: ConnectionType) -> bool {
		matches!(
			connection_type,
			ConnectionType::TcpIpv4 |
				ConnectionType::TcpIpv6 |
				ConnectionType::TcpDns |
				ConnectionType::WebSocketIpv4 { .. } |
				ConnectionType::WebSocketIpv6 { .. } |
				ConnectionType::WebSocketDns { secure: false, .. }
		)
	}

	fn connect_stream(&self, multiaddr: Address) -> Self::StreamConnectFuture {
		let (tcp_socket_addr, host_if_websocket): (
			either::Either<SocketAddr, (String, u16)>,
			Option<String>,
		) = match multiaddr {
			Address::TcpDns { hostname, port } =>
				(either::Right((hostname.to_string(), port)), None),
			Address::TcpIp { ip: IpAddr::V4(ip), port } =>
				(either::Left(SocketAddr::from((ip, port))), None),
			Address::TcpIp { ip: IpAddr::V6(ip), port } =>
				(either::Left(SocketAddr::from((ip, port))), None),
			Address::WebSocketDns { hostname, port, secure: false } => (
				either::Right((hostname.to_string(), port)),
				Some(format!("{}:{}", hostname, port)),
			),
			Address::WebSocketIp { ip: IpAddr::V4(ip), port } => {
				let addr = SocketAddr::from((ip, port));
				(either::Left(addr), Some(addr.to_string()))
			},
			Address::WebSocketIp { ip: IpAddr::V6(ip), port } => {
				let addr = SocketAddr::from((ip, port));
				(either::Left(addr), Some(addr.to_string()))
			},

			// The API user of the `PlatformRef` trait is never supposed to open connections of
			// a type that isn't supported.
			_ => unreachable!(),
		};

		Box::pin(async move {
			let tcp_socket = match tcp_socket_addr {
				either::Left(socket_addr) => TcpStream::connect(socket_addr).await,
				either::Right((dns, port)) => TcpStream::connect((&dns[..], port)).await,
			};

			if let Ok(tcp_socket) = &tcp_socket {
				let _ = tcp_socket.set_nodelay(true);
			}

			let socket: TcpOrWs = match (tcp_socket, host_if_websocket) {
				(Ok(tcp_socket), Some(host)) => future::Either::Right(
					websocket::websocket_client_handshake(websocket::Config {
						tcp_socket: tcp_socket.compat(),
						host: &host,
						url: "/",
					})
					.await
					.map_err(|err| ConnectError {
						message: format!("Failed to negotiate WebSocket: {err}"),
					})?,
				),
				(Ok(tcp_socket), None) => future::Either::Left(tcp_socket.compat()),
				(Err(err), _) =>
					return Err(ConnectError { message: format!("Failed to reach peer: {err}") }),
			};

			Ok(Stream(with_buffers::WithBuffers::new(socket)))
		})
	}

	fn open_out_substream(&self, _c: &mut Self::MultiStream) {
		// This function can only be called with so-called "multi-stream" connections. We never
		// open such connection.
	}

	fn next_substream<'a>(&self, c: &'a mut Self::MultiStream) -> Self::NextSubstreamFuture<'a> {
		// This function can only be called with so-called "multi-stream" connections. We never
		// open such connection.
		match *c {}
	}

	fn spawn_task(
		&self,
		_: std::borrow::Cow<str>,
		task: impl Future<Output = ()> + Send + 'static,
	) {
		self.spawner.spawn("cumulus-internal-light-client-task", None, task)
	}

	fn client_name(&self) -> std::borrow::Cow<str> {
		"cumulus-relay-chain-light-client".into()
	}

	fn client_version(&self) -> std::borrow::Cow<str> {
		env!("CARGO_PKG_VERSION").into()
	}

	fn connect_multistream(
		&self,
		_address: smoldot_light::platform::MultiStreamAddress,
	) -> Self::MultiStreamConnectFuture {
		unimplemented!("Multistream not supported!")
	}

	fn read_write_access<'a>(
		&self,
		stream: Pin<&'a mut Self::Stream>,
	) -> Result<Self::ReadWriteAccess<'a>, &'a std::io::Error> {
		let stream = stream.project();
		stream.0.read_write_access(Instant::now())
	}

	fn wait_read_write_again<'a>(
		&self,
		stream: Pin<&'a mut Self::Stream>,
	) -> Self::StreamUpdateFuture<'a> {
		let stream = stream.project();
		Box::pin(stream.0.wait_read_write_again(|when| async move {
			tokio::time::sleep_until(when.into()).await;
		}))
	}
}

type TcpOrWs = future::Either<CompatTcpStream, websocket::Connection<CompatTcpStream>>;

/// Implementation detail of [`TokioPlatform`].
#[pin_project::pin_project]
pub struct Stream(#[pin] with_buffers::WithBuffers<TcpOrWs>);
