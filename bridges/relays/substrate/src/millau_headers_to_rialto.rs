// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Millau-to-Rialto headers sync entrypoint.

use crate::{MillauClient, RialtoClient};

use async_trait::async_trait;
use codec::Encode;
use headers_relay::{
	sync::{HeadersSyncParams, TargetTransactionMode},
	sync_loop::TargetClient,
	sync_types::{HeadersSyncPipeline, QueuedHeader, SubmittedHeaders},
};
use relay_millau_client::{HeaderId as MillauHeaderId, Millau, SyncHeader as MillauSyncHeader};
use relay_rialto_client::SigningParams as RialtoSigningParams;
use relay_substrate_client::{headers_source::HeadersSource, BlockNumberOf, Error as SubstrateError, HashOf};
use sp_runtime::Justification;
use std::{collections::HashSet, time::Duration};

/// Millau-to-Rialto headers pipeline.
#[derive(Debug, Clone, Copy)]
struct MillauHeadersToRialto;

impl HeadersSyncPipeline for MillauHeadersToRialto {
	const SOURCE_NAME: &'static str = "Millau";
	const TARGET_NAME: &'static str = "Rialto";

	type Hash = HashOf<Millau>;
	type Number = BlockNumberOf<Millau>;
	type Header = MillauSyncHeader;
	type Extra = ();
	type Completion = Justification;

	fn estimate_size(source: &QueuedHeader<Self>) -> usize {
		source.header().encode().len()
	}
}

/// Millau header in-the-queue.
type QueuedMillauHeader = QueuedHeader<MillauHeadersToRialto>;

/// Millau node as headers source.
type MillauSourceClient = HeadersSource<Millau, MillauHeadersToRialto>;

/// Rialto node as headers target.
struct RialtoTargetClient {
	_client: RialtoClient,
	_sign: RialtoSigningParams,
}

#[async_trait]
impl TargetClient<MillauHeadersToRialto> for RialtoTargetClient {
	type Error = SubstrateError;

	async fn best_header_id(&self) -> Result<MillauHeaderId, Self::Error> {
		unimplemented!("https://github.com/paritytech/parity-bridges-common/issues/209")
	}

	async fn is_known_header(&self, _id: MillauHeaderId) -> Result<(MillauHeaderId, bool), Self::Error> {
		unimplemented!("https://github.com/paritytech/parity-bridges-common/issues/209")
	}

	async fn submit_headers(&self, _headers: Vec<QueuedMillauHeader>) -> SubmittedHeaders<MillauHeaderId, Self::Error> {
		unimplemented!("https://github.com/paritytech/parity-bridges-common/issues/209")
	}

	async fn incomplete_headers_ids(&self) -> Result<HashSet<MillauHeaderId>, Self::Error> {
		unimplemented!("https://github.com/paritytech/parity-bridges-common/issues/209")
	}

	#[allow(clippy::unit_arg)]
	async fn complete_header(
		&self,
		_id: MillauHeaderId,
		_completion: Justification,
	) -> Result<MillauHeaderId, Self::Error> {
		unimplemented!("https://github.com/paritytech/parity-bridges-common/issues/209")
	}

	async fn requires_extra(&self, _header: QueuedMillauHeader) -> Result<(MillauHeaderId, bool), Self::Error> {
		unimplemented!("https://github.com/paritytech/parity-bridges-common/issues/209")
	}
}

/// Run Millau-to-Rialto headers sync.
pub fn run(
	millau_client: MillauClient,
	rialto_client: RialtoClient,
	rialto_sign: RialtoSigningParams,
	metrics_params: Option<relay_utils::metrics::MetricsParams>,
) {
	let millau_tick = Duration::from_secs(5);
	let rialto_tick = Duration::from_secs(5);
	let sync_params = HeadersSyncParams {
		max_future_headers_to_download: 32,
		max_headers_in_submitted_status: 1024,
		max_headers_in_single_submit: 8,
		max_headers_size_in_single_submit: 1024 * 1024,
		prune_depth: 256,
		target_tx_mode: TargetTransactionMode::Signed,
	};

	headers_relay::sync_loop::run(
		MillauSourceClient::new(millau_client),
		millau_tick,
		RialtoTargetClient {
			_client: rialto_client,
			_sign: rialto_sign,
		},
		rialto_tick,
		sync_params,
		metrics_params,
		futures::future::pending(),
	);
}
