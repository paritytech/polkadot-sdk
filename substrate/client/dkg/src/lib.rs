pub mod communication;

#[cfg(feature = "bls-experimental")]
pub mod keys;
#[cfg(feature = "bls-experimental")]
pub mod worker;

use std::{marker::PhantomData, sync::Arc};

use rand::rngs::OsRng;
use sc_network::{NotificationService, ProtocolName};

use adkg_vrf::bls::vanilla::BlsSigner;
use adkg_vrf::dkg::{transcript::Transcript, Dkg};

use ark_ec::hashing::{
	curve_maps::wb::{WBConfig, WBMap},
	map_to_curve_hasher::MapToCurve,
};
use ark_ec::{pairing::Pairing, CurveGroup};

const LOG_TARGET: &str = "dkg";

/// Observability handle for the dealings collected by the DKG worker (see the
/// `worker` module, requires `bls-experimental`). A clone is handed to the
/// node that spawns the worker so tests can assert that dealings are gossiped
/// and collected.
#[derive(Debug, Default)]
pub struct CollectionStats {
	/// Total number of dealings collected from gossip.
	pub dealings: std::sync::atomic::AtomicU64,
	/// Session index of the most recently collected dealing.
	pub latest_session: std::sync::atomic::AtomicU32,
}

/// Shared handle to a worker's [`CollectionStats`].
pub type CollectionStatsHandle = Arc<CollectionStats>;

/// DKG gadget network parameters.
pub struct DkgNetworkParams<N, S> {
	/// Network implementing gossip, requests and sync-oracle.
	pub network: Arc<N>,
	/// Syncing service implementing a sync oracle and an event stream for peers.
	pub sync: Arc<S>,
	/// Handle for receiving notification events.
	pub notification_service: Box<dyn NotificationService>,
	/// Chain specific DKG gossip protocol name. See
	/// [`communication::dkg_protocol_name::gossip_protocol_name`].
	pub gossip_protocol_name: ProtocolName,

	pub _phantom: PhantomData<(N, S)>,
}

/// Produces a single dealing (a DKG transcript) for a demo validator set.
///
/// TODO: This is a stub. The signer/dealer keys must come from the validator
/// set and the node's keystore (BLS12-381 session keys), and the transcript
/// must be gossiped to the other validators and submitted to the aggregation
/// parachain.
pub fn perform_dealing<C: Pairing>() -> Option<Transcript<C>>
where
	<C::G2 as CurveGroup>::Config: WBConfig,
	WBMap<<C::G2 as CurveGroup>::Config>: MapToCurve<C::G2>,
{
	let mut os_rng = OsRng;

	// TODO: Get this from somewhere
	let num_validators = 42;
	let f = num_validators;

	let (n, t) = (3 * f + 1, 2 * f + 1);
	// TODO: Add real BLS keys.. These will be the bls keys for each validator
	let signers: Vec<BlsSigner<C>> = (0..n).map(|_| BlsSigner::new(&mut os_rng)).collect();
	let signers_pks: Vec<_> = signers.iter().map(|s| s.bls_pk_g2).collect();

	// TODO: For now a single throwaway dealer; validators deal with their own BLS keys.
	let dealer = BlsSigner::<C>::new(&mut os_rng);

	let dkg = Dkg::<C>::new(signers_pks, t, vec![dealer.bls_pk_g1], 1).ok()?;
	dkg.deal_and_sign(&mut os_rng, (dealer.sk, dealer.bls_pk_g1)).ok()
}
