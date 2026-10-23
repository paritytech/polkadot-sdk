pub mod notification;

pub(crate) mod gossip;
pub(crate) mod peers;

pub mod dkg_protocol_name {
	use array_bytes::bytes2hex;
	use sc_network::ProtocolName;

	// TODO: What are we exactly gossiping to other validators in the DKG? What are we sending? Are we broadcasting something or just sending to individuals?
	/// DKG dealings gossip protocol name suffix.
	const GOSSIP_NAME: &str = "/dkg/2";

	/// Name of the dealings gossip protocol used by the DKG.
	///
	/// Must be registered towards the networking in order for DKG participant to properly function.
	pub fn gossip_protocol_name<Hash: AsRef<[u8]>>(
		genesis_hash: Hash,
		// TODO: Do we need the Fork ID here?
		fork_id: Option<&str>,
	) -> ProtocolName {
		let genesis_hash = genesis_hash.as_ref();
		if let Some(fork_id) = fork_id {
			format!("/{}/{}{}", bytes2hex("", genesis_hash), fork_id, GOSSIP_NAME).into()
		} else {
			format!("/{}{}", bytes2hex("", genesis_hash), GOSSIP_NAME).into()
		}
	}
}

/// Returns the configuration value to put in
/// [`sc_network::config::FullNetworkConfiguration`].
/// For standard protocol name see [`dkg_protocol_name::gossip_protocol_name`].
pub fn dkg_peers_set_config<
	B: sp_runtime::traits::Block,
	N: sc_network::NetworkBackend<B, <B as sp_runtime::traits::Block>::Hash>,
>(
	gossip_protocol_name: sc_network::ProtocolName,
	metrics: sc_network::service::NotificationMetrics,
	peer_store_handle: std::sync::Arc<dyn sc_network::peer_store::PeerStoreProvider>,
) -> (N::NotificationProtocolConfig, Box<dyn sc_network::NotificationService>) {
	let (cfg, notification_service) = N::notification_config(
		gossip_protocol_name,
		Vec::new(),
		1024 * 1024,
		None,
		sc_network::config::SetConfig {
			in_peers: 25, // TODO: This should be all the validators so how do we change this to be all validators?
			out_peers: 25, // TODO: This should be all the validators so how do we change this to be all validators?
			reserved_nodes: Vec::new(),
			non_reserved_mode: sc_network::config::NonReservedPeerMode::Accept,
		},
		metrics,
		peer_store_handle,
	);
	(cfg, notification_service)
}

// cost scalars for reporting peers.
mod cost {
	use sc_network::ReputationChange as Rep;
	// Message that's for an outdated round.
	pub(super) const OUTDATED_MESSAGE: Rep = Rep::new(-50, "DKG: Past message");
	// Message that's from the future relative to our current set-id.
	pub(super) const FUTURE_MESSAGE: Rep = Rep::new(-100, "DKG: Future message");
	// DKG message dealing containing bad signature.
	pub(super) const BAD_SIGNATURE: Rep = Rep::new(-100, "DKG: Bad signature");
	// Message received with dealing from participant not in validator set.
	pub(super) const UNKNOWN_PARTICIPANT: Rep = Rep::new(-150, "DKG: Unknown participant");
	// Message containing invalid dealing.
	pub(super) const INVALID_DEALING: Rep = Rep::new(-5000, "DKG: Invalid dealing");
	// Reputation cost per signature checked for invalid dealing.
	pub(super) const PER_SIGNATURE_CHECKED: i32 = -25;
	// Reputation cost per byte for un-decodable message.
	pub(super) const PER_UNDECODABLE_BYTE: i32 = -5;
}

// benefit scalars for reporting peers.
mod benefit {
	use sc_network::ReputationChange as Rep;
	pub(super) const DEALING_MESSAGE: Rep = Rep::new(100, "DKG: dealing message");
	pub(super) const NOT_INTERESTED: Rep = Rep::new(10, "DKG: Not interested in round");
}