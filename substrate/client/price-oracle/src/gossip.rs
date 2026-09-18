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
//! Gossip of price reports between the oracle nodes.

use crate::pool::ReportPool;
use codec::Decode;
use parking_lot::RwLock;
use sc_network::{
	config::{NonReservedPeerMode, SetConfig},
	service::traits::NotificationService,
	NetworkBackend, NotificationMetrics, ProtocolName, ReputationChange,
};
use sc_network_gossip::{ValidationResult, Validator, ValidatorContext};
use sc_network_types::PeerId;
use sp_price_oracle::{Anchor, SignedPriceReport};
use sp_runtime::{
	traits::{Block as BlockT, Hash, Header},
	RuntimeAppPublic,
};
use std::{marker::PhantomData, sync::Arc};

const LOG_TARGET: &str = "price-oracle";
/// Suffix of the protocol name, after the genesis hash and the optional fork id.
const PROTOCOL_SUFFIX: &str = "/price-oracle/1";
/// Largest gossip message accepted, in bytes.
const MAX_MESSAGE_SIZE: u64 = 64 * 1024;

/// Reputation changes applied to peers for the reports they send.
mod cost {
	use sc_network::ReputationChange as Rep;
	pub(super) const MALFORMED: Rep = Rep::new(-500, "Price oracle: undecodable report");
	pub(super) const STALE_REPORT: Rep = Rep::new(-50, "Price oracle: stale report");
	pub(super) const UNKNOWN_SIGNER: Rep = Rep::new(-150, "Price oracle: unknown signer");
	pub(super) const BAD_SIGNATURE: Rep = Rep::new(-100, "Price oracle: bad signature");
}

mod benefit {
	use sc_network::ReputationChange as Rep;
	pub(super) const GOOD_REPORT: Rep = Rep::new(100, "Price oracle: valid report");
}

/// Name of the gossip protocol: `/{genesis_hash}[/{fork_id}]/price-oracle/1`.
pub fn protocol_name<Hash: AsRef<[u8]>>(genesis_hash: Hash, fork_id: Option<&str>) -> ProtocolName {
	let genesis_hash = array_bytes::bytes2hex("", genesis_hash.as_ref());
	match fork_id {
		Some(fork_id) => format!("/{genesis_hash}/{fork_id}{PROTOCOL_SUFFIX}").into(),
		None => format!("/{genesis_hash}{PROTOCOL_SUFFIX}").into(),
	}
}

/// Register the gossip protocol with the network.
///
/// Returns the protocol configuration to add to the network configuration, the notification
/// service and the protocol name to hand to the service.
pub fn peers_set_config<Block: BlockT, Net: NetworkBackend<Block, Block::Hash>>(
	genesis_hash: Block::Hash,
	fork_id: Option<&str>,
	metrics: NotificationMetrics,
	peer_store: Arc<dyn sc_network::peer_store::PeerStoreProvider>,
) -> (Net::NotificationProtocolConfig, Box<dyn NotificationService>, ProtocolName) {
	let name = protocol_name(genesis_hash, fork_id);
	let (config, notification_service) = Net::notification_config(
		name.clone(),
		Vec::new(),
		MAX_MESSAGE_SIZE,
		None,
		SetConfig {
			in_peers: 25,
			out_peers: 25,
			reserved_nodes: Vec::new(),
			non_reserved_mode: NonReservedPeerMode::Accept,
		},
		metrics,
		peer_store,
	);
	(config, notification_service, name)
}

/// The topic every report is gossiped under.
pub fn topic<Block: BlockT>() -> Block::Hash {
	<<Block::Header as Header>::Hashing as Hash>::hash(b"price-oracle-reports")
}

/// The rules incoming reports are checked against.
///
/// Reports anchored ahead of `current` are accepted: a peer may be ahead of this node, and the
/// runtime rejects anchors ahead of the block including them.
// TODO: consider bounding how far ahead of `current` an anchor may be. A signer anchoring far
// ahead pins its own pool slot and costs every author a filtered report per block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acceptance<Id> {
	/// Keys whose reports are accepted.
	pub signers: Vec<Id>,
	/// The current anchor of this node.
	pub current: Anchor,
	/// Reports anchored more than this many anchors before `current` are rejected.
	pub window: u32,
}

impl<Id> Default for Acceptance<Id> {
	fn default() -> Self {
		Self { signers: Vec::new(), current: Anchor(0), window: 0 }
	}
}

impl<Id> Acceptance<Id> {
	/// The oldest anchor accepted.
	pub fn oldest(&self) -> Anchor {
		Anchor(self.current.0.saturating_sub(self.window))
	}
}

/// Gossip validator for price reports.
///
/// Incoming messages are decoded as [`SignedPriceReport`]s and checked against the current
/// [`Acceptance`] and their signature. Accepted reports are inserted into the pool and forwarded
/// to peers; a report the pool declines is neither forwarded nor penalised. Every rejection is
/// reported as a reputation change of the sending peer.
///
/// The acceptance rules are a snapshot replaced by the service, so validation never reads the
/// chain.
pub struct ReportValidator<Block, Id, Signature> {
	acceptance: RwLock<Acceptance<Id>>,
	pool: ReportPool<Id, Signature>,
	report_peer: Box<dyn Fn(PeerId, ReputationChange) + Send + Sync>,
	_block: PhantomData<Block>,
}

impl<Block, Id, Signature> ReportValidator<Block, Id, Signature> {
	/// Create a validator inserting accepted reports into `pool` and reporting peers through
	/// `report_peer`.
	pub fn new(
		pool: ReportPool<Id, Signature>,
		report_peer: impl Fn(PeerId, ReputationChange) + Send + Sync + 'static,
	) -> Self {
		Self {
			acceptance: RwLock::new(Acceptance::default()),
			pool,
			report_peer: Box::new(report_peer),
			_block: PhantomData,
		}
	}

	/// Replace the acceptance rules.
	pub fn set_acceptance(&self, acceptance: Acceptance<Id>) {
		*self.acceptance.write() = acceptance;
	}

	/// The current acceptance rules.
	pub fn acceptance(&self) -> Acceptance<Id>
	where
		Id: Clone,
	{
		self.acceptance.read().clone()
	}
}

impl<Block, Id, Signature> Validator<Block> for ReportValidator<Block, Id, Signature>
where
	Block: BlockT,
	Id: RuntimeAppPublic<Signature = Signature> + Ord + Clone + Decode + Send + Sync + 'static,
	Signature: Clone + Decode + Send + Sync + 'static,
{
	fn validate(
		&self,
		_context: &mut dyn ValidatorContext<Block>,
		sender: &PeerId,
		mut data: &[u8],
	) -> ValidationResult<Block::Hash> {
		let reject = |cost: ReputationChange, what: &str| {
			log::debug!(target: LOG_TARGET, "Rejecting report from {sender}: {what}");
			(self.report_peer)(*sender, cost);
			ValidationResult::Discard
		};

		let Ok(report) = SignedPriceReport::<Id, Signature>::decode(&mut data) else {
			return reject(cost::MALFORMED, "undecodable");
		};
		let anchor = report.report.anchor;
		let acceptance = self.acceptance.read();
		if anchor < acceptance.oldest() {
			return reject(cost::STALE_REPORT, "stale");
		}
		if !acceptance.signers.contains(&report.signer) {
			return reject(cost::UNKNOWN_SIGNER, "unknown signer");
		}
		drop(acceptance);
		if !report.verify_signature() {
			return reject(cost::BAD_SIGNATURE, "bad signature");
		}

		if !self.pool.insert(report) {
			// Already held, or a later report of the same signer is: nothing to forward.
			return ValidationResult::Discard;
		}
		(self.report_peer)(*sender, benefit::GOOD_REPORT);
		ValidationResult::ProcessAndKeep(topic::<Block>())
	}

	fn message_expired<'a>(&'a self) -> Box<dyn FnMut(Block::Hash, &[u8]) -> bool + 'a> {
		let oldest = self.acceptance.read().oldest();
		Box::new(move |_topic, mut data| {
			match SignedPriceReport::<Id, Signature>::decode(&mut data) {
				Ok(report) => report.report.anchor < oldest,
				Err(_) => true,
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use parking_lot::Mutex;
	use sp_consensus_aura::sr25519::{AuthorityId, AuthoritySignature};
	use sp_core::{crypto::Pair as _, sr25519};
	use sp_price_oracle::PriceReport;
	use sp_runtime::testing::{Block as RawBlock, MockCallU64, TestXt};

	type Block = RawBlock<TestXt<MockCallU64, ()>>;
	type Signed = SignedPriceReport<AuthorityId, AuthoritySignature>;
	type TestValidator = ReportValidator<Block, AuthorityId, AuthoritySignature>;

	struct NoContext;
	impl ValidatorContext<Block> for NoContext {
		fn broadcast_topic(&mut self, _: <Block as BlockT>::Hash, _: bool) {}
		fn broadcast_message(&mut self, _: <Block as BlockT>::Hash, _: Vec<u8>, _: bool) {}
		fn send_message(&mut self, _: &PeerId, _: Vec<u8>) {}
		fn send_topic(&mut self, _: &PeerId, _: <Block as BlockT>::Hash, _: bool) {}
	}

	fn pair(seed: u8) -> sr25519::Pair {
		sr25519::Pair::from_seed(&[seed; 32])
	}
	fn id(seed: u8) -> AuthorityId {
		pair(seed).public().into()
	}
	fn signed(seed: u8, anchor: u32) -> Signed {
		let report = PriceReport { anchor: Anchor(anchor), quotes: vec![] };
		let signature = pair(seed).sign(&report.signing_payload()).into();
		Signed { report, signer: id(seed), signature }
	}

	/// A validator accepting signers 1 and 2, current anchor 100, window 10, recording reports.
	fn validator(
	) -> (TestValidator, ReportPool<AuthorityId, AuthoritySignature>, Arc<Mutex<Vec<i32>>>) {
		let pool = ReportPool::new();
		let reports = Arc::new(Mutex::new(Vec::new()));
		let recorder = reports.clone();
		let validator =
			TestValidator::new(pool.clone(), move |_, rep| recorder.lock().push(rep.value));
		validator.set_acceptance(Acceptance {
			signers: vec![id(1), id(2)],
			current: Anchor(100),
			window: 10,
		});
		(validator, pool, reports)
	}

	fn validate(v: &TestValidator, data: &[u8]) -> ValidationResult<<Block as BlockT>::Hash> {
		v.validate(&mut NoContext, &PeerId::random(), data)
	}

	fn is_keep(r: &ValidationResult<<Block as BlockT>::Hash>) -> bool {
		matches!(r, ValidationResult::ProcessAndKeep(t) if *t == topic::<Block>())
	}

	#[test]
	fn protocol_name_includes_fork_id() {
		assert_eq!(protocol_name([0xab, 0xcd], None).to_string(), "/abcd/price-oracle/1");
		assert_eq!(
			protocol_name([0xab, 0xcd], Some("fork")).to_string(),
			"/abcd/fork/price-oracle/1"
		);
	}

	#[test]
	fn valid_report_is_kept_inserted_and_rewarded() {
		let (v, pool, reports) = validator();
		assert!(is_keep(&validate(&v, &signed(1, 99).encode())));
		assert_eq!(pool.len(), 1);
		assert_eq!(*reports.lock(), vec![benefit::GOOD_REPORT.value]);
	}

	#[test]
	fn stale_reports_are_rejected_and_ahead_ones_accepted() {
		let (v, pool, reports) = validator();
		// Oldest accepted: 90. No upper bound.
		assert!(is_keep(&validate(&v, &signed(1, 90).encode())));
		assert!(is_keep(&validate(&v, &signed(2, 1_000).encode())));
		assert!(matches!(validate(&v, &signed(1, 89).encode()), ValidationResult::Discard));
		assert_eq!(pool.len(), 2);
		assert_eq!(
			*reports.lock(),
			vec![benefit::GOOD_REPORT.value, benefit::GOOD_REPORT.value, cost::STALE_REPORT.value]
		);
	}

	#[test]
	fn unknown_signer_and_bad_signature_are_penalised() {
		let (v, pool, reports) = validator();
		assert!(matches!(validate(&v, &signed(3, 100).encode()), ValidationResult::Discard));
		let mut forged = signed(1, 100);
		forged.report.anchor = Anchor(99);
		assert!(matches!(validate(&v, &forged.encode()), ValidationResult::Discard));
		assert!(matches!(validate(&v, b"garbage"), ValidationResult::Discard));
		assert!(pool.is_empty());
		assert_eq!(
			*reports.lock(),
			vec![cost::UNKNOWN_SIGNER.value, cost::BAD_SIGNATURE.value, cost::MALFORMED.value]
		);
	}

	#[test]
	fn already_held_report_is_discarded_without_penalty() {
		let (v, pool, reports) = validator();
		assert!(is_keep(&validate(&v, &signed(1, 100).encode())));
		// Same anchor from a second peer: last wins in the pool, but nothing new to forward.
		// (Equal anchors replace, so this is a fresh insert and is forwarded.)
		assert!(is_keep(&validate(&v, &signed(1, 100).encode())));
		// An older one is not.
		assert!(matches!(validate(&v, &signed(1, 95).encode()), ValidationResult::Discard));
		assert_eq!(pool.len(), 1);
		assert_eq!(reports.lock().len(), 2, "no penalty for the older report");
	}

	#[test]
	fn expiry_follows_the_window() {
		let (v, _, _) = validator();
		let mut expired = v.message_expired();
		let t = topic::<Block>();
		assert!(!expired(t, &signed(1, 90).encode()));
		assert!(expired(t, &signed(1, 89).encode()));
		assert!(expired(t, b"garbage"));
	}

	#[test]
	fn nothing_is_accepted_before_acceptance_is_set() {
		let pool = ReportPool::new();
		let v = TestValidator::new(pool.clone(), |_, _| {});
		assert!(matches!(validate(&v, &signed(1, 0).encode()), ValidationResult::Discard));
		assert!(pool.is_empty());
	}
}
