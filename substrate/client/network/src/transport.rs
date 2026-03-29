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

//! Transport that serves as a common ground for all connections.
//!
//! 支持 WSS（WebSocket Secure）+ 自签 TLS 证书。
//! TLS 层只负责传输加密，身份认证由 Noise 协议通过 peer ID 完成。

use either::Either;
use libp2p::{
	core::{
		muxing::StreamMuxerBox,
		transport::{Boxed, OptionalTransport},
		upgrade,
	},
	dns, identity, noise, tcp, websocket, PeerId, Transport, TransportExt,
};
use std::sync::Arc;
use std::time::Duration;

// TODO: Create a wrapper similar to upstream `BandwidthTransport` that tracks sent/received bytes
#[allow(deprecated)]
pub use libp2p::bandwidth::BandwidthSinks;

/// 构建 P2P 传输层。
///
/// 当提供 TLS 证书参数时，启用 WSS（WebSocket Secure）监听和拨号；
/// 拨号端同时信任对方的自签证书（通过 `add_trust`），安全性由 Noise 层保证。
///
/// 当 `memory_only` 为 true 时，仅允许进程内通信（测试用）。
#[allow(deprecated)]
pub fn build_transport(
	keypair: identity::Keypair,
	memory_only: bool,
	tls_private_key_der: Option<Vec<u8>>,
	tls_certificate_chain_der: Option<Vec<Vec<u8>>>,
) -> (Boxed<(PeerId, StreamMuxerBox)>, Arc<BandwidthSinks>) {
	// 构建 TLS 配置（如果提供了证书参数）。
	let tls_config = match (&tls_private_key_der, &tls_certificate_chain_der) {
		(Some(key_der), Some(cert_chain_der)) if !cert_chain_der.is_empty() => {
			let key = websocket::tls::PrivateKey::new(key_der.clone());
			let certs: Vec<websocket::tls::Certificate> = cert_chain_der
				.iter()
				.map(|c| websocket::tls::Certificate::new(c.clone()))
				.collect();

			// 构建 TLS 服务端配置（监听 WSS）。
			let mut builder = websocket::tls::Config::builder();
			if let Err(e) = builder.server(key, certs) {
				log::warn!("TLS 服务端配置失败，回退到明文 WS: {e:?}");
				None
			} else {
				let mut config = builder.finish();

				// 替换客户端 TLS 配置：跳过 CA 校验，接受任何自签证书。
				// P2P 网络中 TLS 只负责加密传输，身份认证由 Noise 协议通过 peer ID 完成。
				let provider = futures_rustls::rustls::crypto::ring::default_provider();
				let danger_client = futures_rustls::rustls::ClientConfig::builder_with_provider(
						provider.into(),
					)
					.with_safe_default_protocol_versions()
					.unwrap()
					.dangerous()
					.with_custom_certificate_verifier(Arc::new(NoCertVerifier))
					.with_no_client_auth();
				config.client = Arc::new(danger_client).into();

				Some(config)
			}
		},
		_ => None,
	};

	// 构建传输层基础。
	let transport = if !memory_only {
		let tcp_config = tcp::Config::new().nodelay(true);
		let tcp_trans = tcp::tokio::Transport::new(tcp_config.clone());
		let dns_init = dns::tokio::Transport::system(tcp_trans);

		Either::Left(if let Ok(dns) = dns_init {
			// WSS/WS 传输：需要一个独立的 DNS transport 来解析 WSS 地址。
			let tcp_trans = tcp::tokio::Transport::new(tcp_config);
			let dns_for_wss = dns::tokio::Transport::system(tcp_trans)
				.expect("same system_conf & resolver to work");
			let mut ws_config = websocket::WsConfig::new(dns_for_wss);

			// 如果有 TLS 配置，启用 WSS 监听和拨号。
			if let Some(tls) = tls_config {
				ws_config.set_tls_config(tls);
				log::info!("P2P 传输层已启用 WSS（WebSocket Secure）");
			}

			Either::Left(ws_config.or_transport(dns))
		} else {
			// DNS 不可用时回退到 TCP + WS（WSS 不可用）。
			let tcp_trans = tcp::tokio::Transport::new(tcp_config.clone());
			let desktop_trans = websocket::WsConfig::new(tcp_trans)
				.or_transport(tcp::tokio::Transport::new(tcp_config));
			Either::Right(desktop_trans)
		})
	} else {
		Either::Right(OptionalTransport::some(
			libp2p::core::transport::MemoryTransport::default(),
		))
	};

	let authentication_config =
		noise::Config::new(&keypair).expect("Can create noise config. qed");
	let multiplexing_config = libp2p::yamux::Config::default();

	let transport = transport
		.upgrade(upgrade::Version::V1Lazy)
		.authenticate(authentication_config)
		.multiplex(multiplexing_config)
		.timeout(Duration::from_secs(20))
		.boxed();

	transport.with_bandwidth_logging()
}

/// 自定义 TLS 证书验证器：接受任何证书（含自签证书）。
///
/// P2P 网络中 TLS 只负责传输加密，身份认证由 Noise 协议通过 peer ID 完成。
/// 因此不需要通过 CA 验证对方的 TLS 证书。
#[derive(Debug)]
struct NoCertVerifier;

impl futures_rustls::rustls::client::danger::ServerCertVerifier for NoCertVerifier {
	fn verify_server_cert(
		&self,
		_end_entity: &futures_rustls::rustls::pki_types::CertificateDer<'_>,
		_intermediates: &[futures_rustls::rustls::pki_types::CertificateDer<'_>],
		_server_name: &futures_rustls::rustls::pki_types::ServerName<'_>,
		_ocsp_response: &[u8],
		_now: futures_rustls::rustls::pki_types::UnixTime,
	) -> Result<
		futures_rustls::rustls::client::danger::ServerCertVerified,
		futures_rustls::rustls::Error,
	> {
		Ok(futures_rustls::rustls::client::danger::ServerCertVerified::assertion())
	}

	fn verify_tls12_signature(
		&self,
		_message: &[u8],
		_cert: &futures_rustls::rustls::pki_types::CertificateDer<'_>,
		_dss: &futures_rustls::rustls::DigitallySignedStruct,
	) -> Result<
		futures_rustls::rustls::client::danger::HandshakeSignatureValid,
		futures_rustls::rustls::Error,
	> {
		Ok(futures_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
	}

	fn verify_tls13_signature(
		&self,
		_message: &[u8],
		_cert: &futures_rustls::rustls::pki_types::CertificateDer<'_>,
		_dss: &futures_rustls::rustls::DigitallySignedStruct,
	) -> Result<
		futures_rustls::rustls::client::danger::HandshakeSignatureValid,
		futures_rustls::rustls::Error,
	> {
		Ok(futures_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
	}

	fn supported_verify_schemes(&self) -> Vec<futures_rustls::rustls::SignatureScheme> {
		use futures_rustls::rustls::SignatureScheme;
		vec![
			SignatureScheme::RSA_PKCS1_SHA256,
			SignatureScheme::RSA_PKCS1_SHA384,
			SignatureScheme::RSA_PKCS1_SHA512,
			SignatureScheme::ECDSA_NISTP256_SHA256,
			SignatureScheme::ECDSA_NISTP384_SHA384,
			SignatureScheme::ECDSA_NISTP521_SHA512,
			SignatureScheme::RSA_PSS_SHA256,
			SignatureScheme::RSA_PSS_SHA384,
			SignatureScheme::RSA_PSS_SHA512,
			SignatureScheme::ED25519,
			SignatureScheme::ED448,
		]
	}
}
