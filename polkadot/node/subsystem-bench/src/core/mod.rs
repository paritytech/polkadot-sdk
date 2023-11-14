// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use itertools::Itertools;
use std::{
	collections::HashMap,
	iter::Cycle,
	ops::{Div, Sub},
	sync::Arc,
	time::{Duration, Instant},
};

use sc_keystore::LocalKeystore;
use sp_application_crypto::AppCrypto;
use sp_keystore::{Keystore, KeystorePtr};

use futures::{
	channel::{mpsc, oneshot},
	stream::FuturesUnordered,
	FutureExt, SinkExt, StreamExt,
};
use futures_timer::Delay;

use polkadot_node_metrics::metrics::Metrics;

use polkadot_availability_recovery::AvailabilityRecoverySubsystem;

use parity_scale_codec::Encode;
use polkadot_node_network_protocol::request_response::{
	self as req_res, v1::ChunkResponse, IncomingRequest, ReqProtocolNames, Requests,
};
use rand::{distributions::Uniform, prelude::Distribution, seq::IteratorRandom, thread_rng};

use prometheus::Registry;
use sc_network::{config::RequestResponseConfig, OutboundFailure, RequestFailure};

use polkadot_erasure_coding::{branches, obtain_chunks_v1 as obtain_chunks};
use polkadot_node_primitives::{BlockData, PoV, Proof};
use polkadot_node_subsystem::{
	messages::{
		AllMessages, AvailabilityRecoveryMessage, AvailabilityStoreMessage, NetworkBridgeTxMessage,
		RuntimeApiMessage, RuntimeApiRequest,
	},
	ActiveLeavesUpdate, FromOrchestra, OverseerSignal, Subsystem,
};
use std::net::{Ipv4Addr, SocketAddr};

use super::core::{keyring::Keyring, network::*, test_env::TestEnvironmentMetrics};

const LOG_TARGET: &str = "subsystem-bench::core";

use polkadot_node_primitives::{AvailableData, ErasureChunk};

use polkadot_node_subsystem_test_helpers::{
	make_buffered_subsystem_context, mock::new_leaf, TestSubsystemContextHandle,
};
use polkadot_node_subsystem_util::TimeoutExt;
use polkadot_primitives::{
	AuthorityDiscoveryId, CandidateHash, CandidateReceipt, GroupIndex, Hash, HeadData, IndexedVec,
	PersistedValidationData, SessionIndex, SessionInfo, ValidatorId, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt, dummy_hash};
use sc_service::{SpawnTaskHandle, TaskManager};

pub mod keyring;
pub mod network;
pub mod test_env;
