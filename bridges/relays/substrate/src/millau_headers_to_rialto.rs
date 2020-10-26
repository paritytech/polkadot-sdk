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

use crate::{
	headers_maintain::SubstrateHeadersToSubstrateMaintain,
	headers_target::{SubstrateHeadersSyncPipeline, SubstrateHeadersTarget},
	MillauClient, RialtoClient,
};

use async_trait::async_trait;
use bp_millau::{
	BEST_MILLAU_BLOCK_METHOD, FINALIZED_MILLAU_BLOCK_METHOD, INCOMPLETE_MILLAU_HEADERS_METHOD,
	IS_KNOWN_MILLAU_BLOCK_METHOD,
};
use codec::Encode;
use headers_relay::{
	sync::{HeadersSyncParams, TargetTransactionMode},
	sync_types::{HeadersSyncPipeline, QueuedHeader},
};
use relay_millau_client::{HeaderId as MillauHeaderId, Millau, SyncHeader as MillauSyncHeader};
use relay_rialto_client::{BridgeMillauCall, Rialto, SigningParams as RialtoSigningParams};
use relay_substrate_client::{
	headers_source::HeadersSource, BlockNumberOf, Error as SubstrateError, HashOf, TransactionSignScheme,
};
use sp_core::Pair;
use sp_runtime::Justification;
use std::time::Duration;

/// Millau-to-Rialto headers pipeline.
#[derive(Debug, Clone)]
pub struct MillauHeadersToRialto {
	client: RialtoClient,
	sign: RialtoSigningParams,
}

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

#[async_trait]
impl SubstrateHeadersSyncPipeline for MillauHeadersToRialto {
	const BEST_BLOCK_METHOD: &'static str = BEST_MILLAU_BLOCK_METHOD;
	const FINALIZED_BLOCK_METHOD: &'static str = FINALIZED_MILLAU_BLOCK_METHOD;
	const IS_KNOWN_BLOCK_METHOD: &'static str = IS_KNOWN_MILLAU_BLOCK_METHOD;
	const INCOMPLETE_HEADERS_METHOD: &'static str = INCOMPLETE_MILLAU_HEADERS_METHOD;

	type SignedTransaction = <Rialto as TransactionSignScheme>::SignedTransaction;

	async fn make_submit_header_transaction(
		&self,
		header: QueuedMillauHeader,
	) -> Result<Self::SignedTransaction, SubstrateError> {
		let account_id = self.sign.signer.public().as_array_ref().clone().into();
		let nonce = self.client.next_account_index(account_id).await?;
		let call = BridgeMillauCall::import_signed_header(header.header().clone().into()).into();
		let transaction = Rialto::sign_transaction(&self.client, &self.sign.signer, nonce, call);
		Ok(transaction)
	}

	async fn make_complete_header_transaction(
		&self,
		id: MillauHeaderId,
		completion: Justification,
	) -> Result<Self::SignedTransaction, SubstrateError> {
		let account_id = self.sign.signer.public().as_array_ref().clone().into();
		let nonce = self.client.next_account_index(account_id).await?;
		let call = BridgeMillauCall::finalize_header(id.1, completion).into();
		let transaction = Rialto::sign_transaction(&self.client, &self.sign.signer, nonce, call);
		Ok(transaction)
	}
}

/// Millau header in-the-queue.
type QueuedMillauHeader = QueuedHeader<MillauHeadersToRialto>;

/// Millau node as headers source.
type MillauSourceClient = HeadersSource<Millau, MillauHeadersToRialto>;

/// Rialto node as headers target.
type RialtoTargetClient = SubstrateHeadersTarget<Rialto, MillauHeadersToRialto>;

/// Return sync parameters for Millau-to-Rialto headers sync.
pub fn sync_params() -> HeadersSyncParams {
	HeadersSyncParams {
		max_future_headers_to_download: 32,
		max_headers_in_submitted_status: 8,
		max_headers_in_single_submit: 1,
		max_headers_size_in_single_submit: 1024 * 1024,
		prune_depth: 256,
		target_tx_mode: TargetTransactionMode::Signed,
	}
}

/// Run Millau-to-Rialto headers sync.
pub async fn run(
	millau_client: MillauClient,
	rialto_client: RialtoClient,
	rialto_sign: RialtoSigningParams,
	metrics_params: Option<relay_utils::metrics::MetricsParams>,
) {
	let millau_tick = Duration::from_secs(5);
	let rialto_tick = Duration::from_secs(5);

	let millau_justifications = match millau_client.clone().subscribe_justifications().await {
		Ok(millau_justifications) => millau_justifications,
		Err(error) => {
			log::warn!(
				target: "bridge",
				"Failed to subscribe to Millau justifications: {:?}",
				error,
			);

			return;
		}
	};

	let pipeline = MillauHeadersToRialto {
		client: rialto_client.clone(),
		sign: rialto_sign,
	};
	let sync_maintain =
		SubstrateHeadersToSubstrateMaintain::new(pipeline.clone(), rialto_client.clone(), millau_justifications);

	headers_relay::sync_loop::run(
		MillauSourceClient::new(millau_client),
		millau_tick,
		RialtoTargetClient::new(rialto_client, pipeline),
		rialto_tick,
		sync_maintain,
		sync_params(),
		metrics_params,
		futures::future::pending(),
	);
}
