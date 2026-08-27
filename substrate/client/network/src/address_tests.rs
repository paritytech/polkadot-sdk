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

//! The addresses a node is configured to listen on and to advertise: what is accepted, what is
//! rejected, and what the node is left holding. The certificate a `webrtc-direct` address carries
//! is derived and pinned in [`crate::webrtc`].

use crate::{
	config::{NetworkBackendType, NetworkConfiguration, NodeKeyConfig},
	error::Error,
	webrtc::*,
};

use sc_network_types::{
	multiaddr::{Multiaddr, Protocol},
	multihash::Code,
	PeerId,
};

/// `/ip4/1.2.3.4/udp/30334/webrtc-direct`: the shape an operator is expected to supply.
fn webrtc_address() -> Multiaddr {
	Multiaddr::empty()
		.with(Protocol::Ip4([1, 2, 3, 4].into()))
		.with(Protocol::Udp(30334))
		.with(Protocol::WebRTCDirect)
}

#[test]
fn bare_webrtc_address_accepted() {
	assert!(validate_listen_address(&webrtc_address()).is_ok());
	assert!(validate_public_address(&webrtc_address()).is_ok());
}

/// The one difference between the two kinds: a public address is dialed, so the dialer can
/// resolve a name, while a listen address is bound and a name is nothing to bind.
#[test]
fn dns_host_accepted_for_a_public_address_only() {
	let address = Multiaddr::empty()
		.with(Protocol::Dns("example.com".into()))
		.with(Protocol::Udp(30334))
		.with(Protocol::WebRTCDirect);

	assert!(validate_public_address(&address).is_ok());
	assert!(matches!(validate_listen_address(&address), Err(Error::InvalidWebRtcAddress { .. }),));
}

#[test]
fn certhash_past_webrtc_direct_rejected() {
	// This is the shape check alone: a supplied `/certhash` is verified against the node's own
	// and removed before it ever reaches here, so anything left past `webrtc-direct` is junk.
	let their_certhash = Code::Sha2_256.digest(b"theirs");
	let address = webrtc_address().with(Protocol::Certhash(their_certhash));

	assert!(matches!(validate_listen_address(&address), Err(Error::InvalidWebRtcAddress { .. }),));
}

#[test]
fn webrtc_over_tcp_rejected() {
	let address = Multiaddr::empty()
		.with(Protocol::Ip4([1, 2, 3, 4].into()))
		.with(Protocol::Tcp(30334))
		.with(Protocol::WebRTCDirect);

	assert!(matches!(validate_listen_address(&address), Err(Error::InvalidWebRtcAddress { .. }),));
}

/// A configuration with a fixed node key, so the certificate it will present can be derived
/// in the test as well.
fn webrtc_config(public_address: &str) -> NetworkConfiguration {
	use crate::config::{ed25519, Secret};

	let mut config = NetworkConfiguration::new_local();
	config.node_key = NodeKeyConfig::Ed25519(Secret::Input(
		ed25519::SecretKey::try_from_bytes([7u8; 32]).unwrap(),
	));
	config.listen_addresses = vec!["/ip4/0.0.0.0/udp/30333/webrtc-direct".parse().unwrap()];
	config.public_addresses = vec![public_address.parse().unwrap()];

	config
}

/// The `/certhash` the node of [`webrtc_config`] presents.
fn node_certhash() -> Protocol<'static> {
	let keypair = webrtc_config("/ip4/1.2.3.4/tcp/30333").node_key.into_keypair().unwrap();
	let certificate = derive_certificate(keypair.secret().into()).unwrap();

	Protocol::Certhash(certificate.certhash().into())
}

/// The `/p2p` of the node of [`webrtc_config`].
fn node_peer_id() -> Protocol<'static> {
	let keypair = webrtc_config("/ip4/1.2.3.4/tcp/30333").node_key.into_keypair().unwrap();

	Protocol::P2p(keypair.public().to_peer_id().into())
}

#[test]
fn webrtc_public_address_completed() {
	let mut config = webrtc_config("/ip4/203.0.113.9/udp/31234/webrtc-direct");
	config.validate_and_complete_addresses().unwrap();

	assert_eq!(
		config.public_addresses,
		vec!["/ip4/203.0.113.9/udp/31234/webrtc-direct"
			.parse::<Multiaddr>()
			.unwrap()
			.with(node_certhash())],
	);
}

#[test]
fn completion_is_idempotent() {
	// A `/certhash` already there is the node's own by the time the second run sees it, so it
	// is stripped and appended back rather than taken for junk.
	let mut config = webrtc_config("/ip4/203.0.113.9/udp/31234/webrtc-direct");
	config.validate_and_complete_addresses().unwrap();
	let completed = config.public_addresses.clone();

	config.validate_and_complete_addresses().unwrap();

	assert_eq!(config.public_addresses, completed);
}

#[test]
fn removing_webrtc_addresses_drops_completed_public_address() {
	// A collator drops the relay-side WebRTC listeners after validation completed the
	// public address: the removal must drop the now listener-less public address with
	// them instead of leaving it advertised with nothing serving it.
	let mut config = webrtc_config("/ip4/203.0.113.9/udp/31234/webrtc-direct");
	config.validate_and_complete_addresses().unwrap();

	config.remove_webrtc_addresses();

	assert!(config.listen_addresses.is_empty());
	assert!(config.public_addresses.is_empty());
}

#[test]
fn removing_webrtc_addresses_keeps_other_addresses() {
	let public_address = "/ip4/203.0.113.9/tcp/31234";
	let mut config = webrtc_config(public_address);
	let tcp_listener: Multiaddr = "/ip4/0.0.0.0/tcp/30333".parse().unwrap();
	config.listen_addresses.push(tcp_listener.clone());
	config.validate_and_complete_addresses().unwrap();

	config.remove_webrtc_addresses();

	assert_eq!(config.listen_addresses, vec![tcp_listener]);
	assert_eq!(config.public_addresses, vec![public_address.parse::<Multiaddr>().unwrap()]);
}

#[test]
fn malformed_webrtc_public_address_rejected() {
	// `tcp` rather than `udp`, so there is no shape to complete.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234/webrtc-direct");

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::InvalidWebRtcAddress { .. }),
	));
}

#[test]
fn webrtc_public_address_with_wrong_certhash_rejected() {
	// The certificate is derived from the node key, so advertising another hash would send
	// peers into a handshake this node cannot complete.
	let address = "/ip4/203.0.113.9/udp/31234/webrtc-direct"
		.parse::<Multiaddr>()
		.unwrap()
		.with(Protocol::Certhash(Code::Sha2_256.digest(b"theirs")));
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.public_addresses = vec![address];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::MismatchedAddressIdentity { .. }),
	));
}

#[test]
fn webrtc_public_address_with_matching_certhash_accepted() {
	// Half of the advertised shape, the peer id being the other half.
	let public_address = "/ip4/203.0.113.9/udp/31234/webrtc-direct";
	let mut config = webrtc_config(public_address);
	config.public_addresses =
		vec![public_address.parse::<Multiaddr>().unwrap().with(node_certhash())];

	config.validate_and_complete_addresses().unwrap();

	// Stripped by the check, put back by the completion: the same address either way.
	assert_eq!(
		config.public_addresses,
		vec![public_address.parse::<Multiaddr>().unwrap().with(node_certhash())],
	);
}

#[test]
fn webrtc_listen_address_with_wrong_certhash_rejected() {
	// The certificate is derived from the node key, so a hash that disagrees with it is a hash
	// this node will never present, however the two came to differ.
	let address = "/ip4/0.0.0.0/udp/30333/webrtc-direct"
		.parse::<Multiaddr>()
		.unwrap()
		.with(Protocol::Certhash(Code::Sha2_256.digest(b"theirs")));
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec![address];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::MismatchedAddressIdentity { .. }),
	));
}

#[test]
fn webrtc_listen_address_with_matching_certhash_accepted() {
	// The shape an operator gets by pasting back the address the node advertises.
	let listen_address = "/ip4/0.0.0.0/udp/30333/webrtc-direct";
	let mut config = webrtc_config("/ip4/203.0.113.9/udp/31234/webrtc-direct");
	config.listen_addresses =
		vec![listen_address.parse::<Multiaddr>().unwrap().with(node_certhash())];

	config.validate_and_complete_addresses().unwrap();

	// Checked, then removed: what gets bound is the bare address.
	assert_eq!(config.listen_addresses, vec![listen_address.parse::<Multiaddr>().unwrap()]);
	assert_eq!(
		config.public_addresses,
		vec!["/ip4/203.0.113.9/udp/31234/webrtc-direct"
			.parse::<Multiaddr>()
			.unwrap()
			.with(node_certhash())],
	);
}

#[test]
fn webrtc_listen_address_with_matching_certhash_and_peer_id_accepted() {
	let listen_address = "/ip4/0.0.0.0/udp/30333/webrtc-direct";
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec![listen_address
		.parse::<Multiaddr>()
		.unwrap()
		.with(node_certhash())
		.with(node_peer_id())];

	config.validate_and_complete_addresses().unwrap();

	assert_eq!(config.listen_addresses, vec![listen_address.parse::<Multiaddr>().unwrap()]);
}

#[test]
fn listen_address_with_matching_peer_id_accepted() {
	// Nothing about this configuration is WebRTC: the peer id of a plain listen address is
	// checked all the same.
	let listen_address = "/ip4/0.0.0.0/tcp/30333";
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses =
		vec![listen_address.parse::<Multiaddr>().unwrap().with(node_peer_id())];

	config.validate_and_complete_addresses().unwrap();

	assert_eq!(config.listen_addresses, vec![listen_address.parse::<Multiaddr>().unwrap()]);
}

#[test]
fn listen_address_with_wrong_peer_id_rejected() {
	// The regenerated-node-key case: the address the operator published names another node,
	// and starting anyway would leave this one reachable on nothing anybody dials.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/30333"
		.parse::<Multiaddr>()
		.unwrap()
		.with(Protocol::P2p(PeerId::random().into()))];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::MismatchedAddressIdentity { .. }),
	));
}

#[test]
fn listen_address_identity_checked_on_libp2p() {
	// The check sits above the backends, so it fires for libp2p as well.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.network_backend = NetworkBackendType::Libp2p;
	config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/30333"
		.parse::<Multiaddr>()
		.unwrap()
		.with(Protocol::P2p(PeerId::random().into()))];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::MismatchedAddressIdentity { .. }),
	));
}

#[test]
fn certhash_outside_webrtc_rejected() {
	// A `/certhash` names a DTLS certificate, and only the WebRTC transport presents one, so
	// this is a WebRTC address that isn't one, reported as such.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses =
		vec!["/ip4/0.0.0.0/tcp/30333".parse::<Multiaddr>().unwrap().with(node_certhash())];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::InvalidWebRtcAddress { .. }),
	));
}

#[test]
fn certhash_outside_webrtc_rejected_beside_a_webrtc_listener() {
	// The node does listen for WebRTC here, so it has a certificate and the hash below is its
	// own. TCP still presents no certificate, so the address is nonsense whatever the hash
	// says, and accepting it would strip the hash and bind the address as if it had been bare.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec![
		"/ip4/0.0.0.0/udp/30333/webrtc-direct".parse().unwrap(),
		"/ip4/0.0.0.0/tcp/30333".parse::<Multiaddr>().unwrap().with(node_certhash()),
	];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::InvalidWebRtcAddress { .. }),
	));
}

#[test]
fn identity_components_in_the_wrong_order_rejected() {
	// `/certhash` belongs before the peer id. Both hashes here are this node's own, so only
	// the order is wrong, and taking the two for an unordered set would accept this.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec!["/ip4/0.0.0.0/udp/30333/webrtc-direct"
		.parse::<Multiaddr>()
		.unwrap()
		.with(node_peer_id())
		.with(node_certhash())];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::MalformedAddressIdentity { .. }),
	));
}

#[test]
fn repeated_identity_component_rejected() {
	// At most one of each. Both peer ids are this node's own, so stripping whatever matches
	// would accept this and bind an address the operator never meant to write.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/30333"
		.parse::<Multiaddr>()
		.unwrap()
		.with(node_peer_id())
		.with(node_peer_id())];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::MalformedAddressIdentity { .. }),
	));
}

#[test]
fn repeated_certhash_rejected() {
	// The outer hash is not this node's, but the fault is that there are two of them, and
	// comparing before the shape is checked would report a hash mismatch instead.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec!["/ip4/0.0.0.0/udp/30333/webrtc-direct"
		.parse::<Multiaddr>()
		.unwrap()
		.with(node_certhash())
		.with(Protocol::Certhash(Code::Sha2_256.digest(b"theirs")))];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::MalformedAddressIdentity { .. }),
	));
}

#[test]
fn identity_component_before_the_end_rejected() {
	// A peer id that is not the last component is still a peer id. Left unchecked, litep2p
	// stops parsing at the `/p2p` and binds plain TCP under an identity nobody dials.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/30333"
		.parse::<Multiaddr>()
		.unwrap()
		.with(Protocol::P2p(PeerId::random().into()))
		.with(Protocol::Ws("/".into()))];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::MalformedAddressIdentity { .. }),
	));
}

#[test]
fn node_key_untouched_without_configured_identity() {
	// Nothing here names an identity, so the key it would be checked against is never
	// resolved — and a file-backed one is never written.
	use crate::config::Secret;

	let directory = tempfile::Builder::new().prefix("webrtc").tempdir().unwrap();
	let key_path = directory.path().join("node_key");

	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.node_key = NodeKeyConfig::Ed25519(Secret::File(key_path.clone()));
	config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/30333".parse().unwrap()];

	config.validate_and_complete_addresses().unwrap();

	assert!(!key_path.exists(), "the node key must not be resolved with nothing to check");
}

#[test]
fn public_address_with_matching_peer_id_accepted() {
	// The shape an operator gets by pasting back the address they hand out as a bootnode.
	let public_address = "/ip4/203.0.113.9/tcp/31234";
	let mut config = webrtc_config(public_address);
	config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/30333".parse().unwrap()];
	config.public_addresses =
		vec![public_address.parse::<Multiaddr>().unwrap().with(node_peer_id())];

	config.validate_and_complete_addresses().unwrap();

	// Checked, then removed: every consumer of the list appends the peer id itself.
	assert_eq!(config.public_addresses, vec![public_address.parse::<Multiaddr>().unwrap()]);
}

#[test]
fn public_address_with_wrong_peer_id_rejected() {
	// What peers are told to dial, so this one matters more than a listener's: litep2p drops
	// it with a warning and the node runs on, advertising nothing at that address.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/30333".parse().unwrap()];
	config.public_addresses = vec!["/ip4/203.0.113.9/tcp/31234"
		.parse::<Multiaddr>()
		.unwrap()
		.with(Protocol::P2p(PeerId::random().into()))];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::MismatchedAddressIdentity { .. }),
	));
}

#[test]
fn webrtc_public_address_with_peer_id_completed() {
	// The peer id goes, the node's own `/certhash` is appended in its place.
	let public_address = "/ip4/203.0.113.9/udp/31234/webrtc-direct";
	let mut config = webrtc_config(public_address);
	config.public_addresses =
		vec![public_address.parse::<Multiaddr>().unwrap().with(node_peer_id())];

	config.validate_and_complete_addresses().unwrap();

	assert_eq!(
		config.public_addresses,
		vec![public_address.parse::<Multiaddr>().unwrap().with(node_certhash())],
	);
}

#[test]
fn webrtc_public_address_with_matching_certhash_and_peer_id_accepted() {
	// The whole shape an operator gets by pasting back the address the node advertises.
	let public_address = "/ip4/203.0.113.9/udp/31234/webrtc-direct";
	let mut config = webrtc_config(public_address);
	config.public_addresses = vec![public_address
		.parse::<Multiaddr>()
		.unwrap()
		.with(node_certhash())
		.with(node_peer_id())];

	config.validate_and_complete_addresses().unwrap();

	assert_eq!(
		config.public_addresses,
		vec![public_address.parse::<Multiaddr>().unwrap().with(node_certhash())],
	);
}

#[test]
fn certhash_parsed_from_text_accepted() {
	// The operator path: the hash arrives as the base64 the node printed, not as a `Multihash`
	// this test derived, so the comparison has to survive the round trip through text.
	let public_address = "/ip4/203.0.113.9/udp/31234/webrtc-direct";
	let mut config = webrtc_config(public_address);
	config.public_addresses =
		vec![format!("{public_address}{}", node_certhash()).parse::<Multiaddr>().unwrap()];

	config.validate_and_complete_addresses().unwrap();

	assert_eq!(
		config.public_addresses,
		vec![public_address.parse::<Multiaddr>().unwrap().with(node_certhash())],
	);
}

#[test]
fn dns_webrtc_public_address_completed() {
	// A name resolves to whatever address answers it, so a public one may be a `dns` host.
	let public_address = "/dns/example.com/udp/443/webrtc-direct";
	let mut config = webrtc_config(public_address);

	config.validate_and_complete_addresses().unwrap();

	assert_eq!(
		config.public_addresses,
		vec![public_address.parse::<Multiaddr>().unwrap().with(node_certhash())],
	);
}

#[test]
fn address_of_nothing_but_an_identity_rejected() {
	// Stripping it leaves nothing to bind, and a node that binds nothing is reachable by
	// nobody — the outcome the check exists to prevent, reached through the check itself.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec![Multiaddr::empty().with(node_peer_id())];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::MalformedAddressIdentity { .. }),
	));
}

#[test]
fn unrepresentable_component_left_alone() {
	// `/webrtc` is not `/webrtc-direct`, and `Protocol` cannot name it. Taking the peer id off
	// must not rewrite what is underneath it, however little sc-network can say about it.
	let listen_address = "/ip4/0.0.0.0/udp/30333/webrtc";
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses =
		vec![listen_address.parse::<Multiaddr>().unwrap().with(node_peer_id())];

	config.validate_and_complete_addresses().unwrap();

	assert_eq!(config.listen_addresses, vec![listen_address.parse::<Multiaddr>().unwrap()]);
}

#[test]
fn mismatched_identity_names_both_sides_and_the_key() {
	// The three things an operator needs to tell the two 52-character strings apart.
	let configured_peer_id = Protocol::P2p(PeerId::random().into());
	let address = "/ip4/0.0.0.0/tcp/30333"
		.parse::<Multiaddr>()
		.unwrap()
		.with(configured_peer_id.clone());
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.listen_addresses = vec![address.clone()];

	let Err(Error::MismatchedAddressIdentity {
		address: reported,
		configured,
		expected,
		node_key_origin,
	}) = config.validate_and_complete_addresses()
	else {
		panic!("a peer id that is not this node's must be reported as such");
	};

	// The address as written, not as the check left it.
	assert_eq!(reported, address);
	assert_eq!(configured, configured_peer_id.to_string());
	assert_eq!(expected, node_peer_id().to_string());
	assert!(node_key_origin.contains("--node-key"), "{node_key_origin}");
}

#[test]
fn node_key_origin_names_where_the_key_came_from() {
	use crate::config::Secret;

	let wrong_peer_id = || {
		vec!["/ip4/0.0.0.0/tcp/30333"
			.parse::<Multiaddr>()
			.unwrap()
			.with(Protocol::P2p(PeerId::random().into()))]
	};
	let origin_of = |node_key| {
		let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
		config.node_key = node_key;
		config.listen_addresses = wrong_peer_id();

		match config.validate_and_complete_addresses() {
			Err(Error::MismatchedAddressIdentity { node_key_origin, .. }) => node_key_origin,
			other => panic!("expected a mismatch, got {other:?}"),
		}
	};

	let directory = tempfile::Builder::new().prefix("webrtc").tempdir().unwrap();
	let key_path = directory.path().join("secret_ed25519");

	assert!(origin_of(NodeKeyConfig::Ed25519(Secret::New)).contains("generated anew on each start"));
	assert!(origin_of(webrtc_config("/ip4/1.2.3.4/tcp/1").node_key).contains("--node-key"));

	// Written by the resolution that this very error compared against: naming it without
	// saying so would send the operator after a key that is already gone by the next start.
	let generated = origin_of(NodeKeyConfig::Ed25519(Secret::File(key_path.clone())));
	assert!(generated.contains(&key_path.display().to_string()), "{generated}");
	assert!(generated.contains("generated on this start"), "{generated}");

	let existing = origin_of(NodeKeyConfig::Ed25519(Secret::File(key_path.clone())));
	assert!(existing.contains(&key_path.display().to_string()), "{existing}");
	assert!(!existing.contains("generated on this start"), "{existing}");
}

#[test]
fn webrtc_public_address_without_a_listener_rejected() {
	// No WebRTC listen address means no certificate, so there is nothing to advertise.
	let mut config = webrtc_config("/ip4/203.0.113.9/udp/31234/webrtc-direct");
	config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/30333".parse().unwrap()];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::WebRtcTransportNotConfigured { .. }),
	));
}

#[test]
fn dns_webrtc_listen_address_rejected() {
	// A listen address is bound, so a `dns` host names nothing bindable. Accepting it would
	// leave the node advertising a certhash for a transport that was never started.
	let mut config = webrtc_config("/ip4/203.0.113.9/udp/31234/webrtc-direct");
	config.listen_addresses = vec!["/dns/example.com/udp/30333/webrtc-direct".parse().unwrap()];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::InvalidWebRtcAddress { .. }),
	));
}

#[test]
fn public_address_untouched_without_webrtc() {
	// An address of another transport is never touched.
	let address = "/ip4/203.0.113.9/tcp/31234";
	let mut config = webrtc_config(address);
	config.validate_and_complete_addresses().unwrap();

	assert_eq!(config.public_addresses, vec![address.parse::<Multiaddr>().unwrap()]);
}

#[test]
fn webrtc_listen_address_rejected_on_libp2p() {
	// libp2p has no WebRTC transport: it would drop the listen address with one warning and
	// carry on, leaving the node reachable on nothing it advertises.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.network_backend = NetworkBackendType::Libp2p;

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::WebRtcNotSupportedByBackend),
	));
}

#[test]
fn webrtc_public_address_rejected_on_libp2p() {
	// Only the public address is WebRTC here, so there is nothing to complete; the backend
	// check must still reject it.
	let mut config = webrtc_config("/ip4/203.0.113.9/udp/31234/webrtc-direct");
	config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/30333".parse().unwrap()];
	config.network_backend = NetworkBackendType::Libp2p;

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::WebRtcNotSupportedByBackend),
	));
}

#[test]
fn certhash_rejected_on_libp2p() {
	// No `webrtc-direct` component, so nothing here says WebRTC except the certificate hash
	// itself — and pointing this backend at the `webrtc-direct` shape would be a dead end.
	let mut config = webrtc_config("/ip4/203.0.113.9/tcp/31234");
	config.network_backend = NetworkBackendType::Libp2p;
	config.listen_addresses =
		vec!["/ip4/0.0.0.0/tcp/30333".parse::<Multiaddr>().unwrap().with(node_certhash())];

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::WebRtcNotSupportedByBackend),
	));
}

#[test]
fn non_webrtc_addresses_accepted_on_libp2p() {
	let address = "/ip4/203.0.113.9/tcp/31234";
	let mut config = webrtc_config(address);
	config.listen_addresses = vec!["/ip4/0.0.0.0/tcp/30333".parse().unwrap()];
	config.network_backend = NetworkBackendType::Libp2p;

	config.validate_and_complete_addresses().unwrap();

	assert_eq!(config.public_addresses, vec![address.parse::<Multiaddr>().unwrap()]);
}

#[test]
fn webrtc_rejected_on_libp2p_before_node_key_is_resolved() {
	// Resolving a file-backed node key writes the file. Rejecting first means a configuration
	// that will not start leaves nothing behind.
	use crate::config::Secret;

	let directory = tempfile::Builder::new().prefix("webrtc").tempdir().unwrap();
	let key_path = directory.path().join("node_key");

	let mut config = webrtc_config("/ip4/203.0.113.9/udp/31234/webrtc-direct");
	config.node_key = NodeKeyConfig::Ed25519(Secret::File(key_path.clone()));
	config.network_backend = NetworkBackendType::Libp2p;

	assert!(matches!(
		config.validate_and_complete_addresses(),
		Err(Error::WebRtcNotSupportedByBackend),
	));
	assert!(!key_path.exists(), "the node key file must not be created for a rejected config");
}
