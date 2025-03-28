// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

use futures::{channel::oneshot, Stream};
use sc_network::{
	config::MultiaddrWithPeerId,
	event::Event,
	multiaddr::Multiaddr,
	network_state::NetworkState,
	request_responses::{IfDisconnected, RequestFailure},
	service::{
		signature::{Signature, SigningError},
		traits::{
			KademliaKey, NetworkDHTProvider, NetworkEventStream, NetworkPeers, NetworkRequest,
			NetworkSigner, NetworkStateInfo, NetworkStatus, NetworkStatusProvider, Record,
		},
	},
	ObservedRole, PeerId, ProtocolName, ReputationChange,
};
use sp_consensus::SyncOracle;
use std::{collections::HashSet, pin::Pin, time::Instant};

/// Dummy [`SyncOracle`] that is not triggered by tests, but required for the initialization.
pub struct DummySyncOracle {}

impl SyncOracle for DummySyncOracle {
	fn is_major_syncing(&self) -> bool {
		unimplemented!("Not needed for test")
	}

	fn is_offline(&self) -> bool {
		unimplemented!("Not needed for test")
	}
}

/// Dummy network service that is not triggered by tests, but required for the initialization.
pub struct DummyNetworkService {}

impl NetworkSigner for DummyNetworkService {
	fn sign_with_local_identity(&self, _msg: Vec<u8>) -> Result<Signature, SigningError> {
		unimplemented!("Not needed for test")
	}

	fn verify(
		&self,
		_peer_id: PeerId,
		_public_key: &Vec<u8>,
		_signature: &Vec<u8>,
		_message: &Vec<u8>,
	) -> Result<bool, String> {
		unimplemented!("Not needed for test")
	}
}

impl NetworkDHTProvider for DummyNetworkService {
	fn find_closest_peers(&self, _target: PeerId) {
		unimplemented!("Not needed for test")
	}

	fn get_value(&self, _key: &KademliaKey) {
		unimplemented!("Not needed for test")
	}

	fn put_value(&self, _key: KademliaKey, _value: Vec<u8>) {
		unimplemented!("Not needed for test")
	}

	fn put_record_to(&self, _record: Record, _peers: HashSet<PeerId>, _update_local_storage: bool) {
		unimplemented!("Not needed for test")
	}

	fn store_record(
		&self,
		_key: KademliaKey,
		_value: Vec<u8>,
		_publisher: Option<PeerId>,
		_expires: Option<Instant>,
	) {
		unimplemented!("Not needed for test")
	}

	fn start_providing(&self, _key: KademliaKey) {
		unimplemented!("Not needed for test")
	}

	fn stop_providing(&self, _key: KademliaKey) {
		unimplemented!("Not needed for test")
	}

	fn get_providers(&self, _key: KademliaKey) {
		unimplemented!("Not needed for test")
	}
}

#[async_trait::async_trait]
impl NetworkStatusProvider for DummyNetworkService {
	async fn status(&self) -> Result<NetworkStatus, ()> {
		unimplemented!("Not needed for test")
	}

	async fn network_state(&self) -> Result<NetworkState, ()> {
		unimplemented!("Not needed for test")
	}
}

#[async_trait::async_trait]
impl NetworkPeers for DummyNetworkService {
	fn set_authorized_peers(&self, _peers: HashSet<PeerId>) {
		unimplemented!("Not needed for test")
	}

	fn set_authorized_only(&self, _reserved_only: bool) {
		unimplemented!("Not needed for test")
	}

	fn add_known_address(&self, _peer_id: PeerId, _addr: Multiaddr) {
		unimplemented!("Not needed for test")
	}

	fn report_peer(&self, _peer_id: PeerId, _cost_benefit: ReputationChange) {
		unimplemented!("Not needed for test")
	}

	fn peer_reputation(&self, _peer_id: &PeerId) -> i32 {
		unimplemented!("Not needed for test")
	}

	fn disconnect_peer(&self, _peer_id: PeerId, _protocol: ProtocolName) {
		unimplemented!("Not needed for test")
	}

	fn accept_unreserved_peers(&self) {
		unimplemented!("Not needed for test")
	}

	fn deny_unreserved_peers(&self) {
		unimplemented!("Not needed for test")
	}

	fn add_reserved_peer(&self, _peer: MultiaddrWithPeerId) -> Result<(), String> {
		unimplemented!("Not needed for test")
	}

	fn remove_reserved_peer(&self, _peer_id: PeerId) {
		unimplemented!("Not needed for test")
	}

	fn set_reserved_peers(
		&self,
		_protocol: ProtocolName,
		_peers: HashSet<Multiaddr>,
	) -> Result<(), String> {
		unimplemented!("Not needed for test")
	}

	fn add_peers_to_reserved_set(
		&self,
		_protocol: ProtocolName,
		_peers: HashSet<Multiaddr>,
	) -> Result<(), String> {
		unimplemented!("Not needed for test")
	}

	fn remove_peers_from_reserved_set(
		&self,
		_protocol: ProtocolName,
		_peers: Vec<PeerId>,
	) -> Result<(), String> {
		unimplemented!("Not needed for test")
	}

	fn sync_num_connected(&self) -> usize {
		unimplemented!("Not needed for test")
	}

	fn peer_role(&self, _peer_id: PeerId, _handshake: Vec<u8>) -> Option<ObservedRole> {
		unimplemented!("Not needed for test")
	}

	async fn reserved_peers(&self) -> Result<Vec<PeerId>, ()> {
		unimplemented!("Not needed for test")
	}
}

impl NetworkEventStream for DummyNetworkService {
	fn event_stream(&self, _name: &'static str) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
		unimplemented!("Not needed for test")
	}
}

impl NetworkStateInfo for DummyNetworkService {
	fn external_addresses(&self) -> Vec<Multiaddr> {
		unimplemented!("Not needed for test")
	}

	fn listen_addresses(&self) -> Vec<Multiaddr> {
		unimplemented!("Not needed for test")
	}

	fn local_peer_id(&self) -> PeerId {
		unimplemented!("Not needed for test")
	}
}

#[async_trait::async_trait]
impl NetworkRequest for DummyNetworkService {
	async fn request(
		&self,
		_target: PeerId,
		_protocol: ProtocolName,
		_request: Vec<u8>,
		_fallback_request: Option<(Vec<u8>, ProtocolName)>,
		_connect: IfDisconnected,
	) -> Result<(Vec<u8>, ProtocolName), RequestFailure> {
		unimplemented!("Not needed for test")
	}

	fn start_request(
		&self,
		_target: PeerId,
		_protocol: ProtocolName,
		_request: Vec<u8>,
		_fallback_request: Option<(Vec<u8>, ProtocolName)>,
		_tx: oneshot::Sender<Result<(Vec<u8>, ProtocolName), RequestFailure>>,
		_connect: IfDisconnected,
	) {
		unimplemented!("Not needed for test")
	}
}
