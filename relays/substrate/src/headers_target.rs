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

//! Substrate client as Substrate headers target. The chain we connect to should have
//! runtime that implements `<BridgedChainName>HeaderApi` to allow bridging with
//! <BridgedName> chain.

use crate::headers_pipeline::SubstrateHeadersSyncPipeline;

use async_trait::async_trait;
use codec::{Decode, Encode};
use futures::TryFutureExt;
use headers_relay::{
	sync_loop::TargetClient,
	sync_types::{HeaderIdOf, QueuedHeader, SubmittedHeaders},
};
use relay_substrate_client::{Chain, Client, Error as SubstrateError};
use relay_utils::HeaderId;
use sp_core::Bytes;
use sp_runtime::Justification;
use std::collections::HashSet;

/// Substrate client as Substrate headers target.
pub struct SubstrateHeadersTarget<C: Chain, P> {
	client: Client<C>,
	pipeline: P,
}

impl<C: Chain, P> SubstrateHeadersTarget<C, P> {
	/// Create new Substrate headers target.
	pub fn new(client: Client<C>, pipeline: P) -> Self {
		SubstrateHeadersTarget { client, pipeline }
	}
}

#[async_trait]
impl<C, P> TargetClient<P> for SubstrateHeadersTarget<C, P>
where
	C: Chain,
	P::Number: Decode,
	P::Hash: Decode + Encode,
	P: SubstrateHeadersSyncPipeline<Completion = Justification, Extra = ()>,
{
	type Error = SubstrateError;

	async fn best_header_id(&self) -> Result<HeaderIdOf<P>, Self::Error> {
		let call = P::BEST_BLOCK_METHOD.into();
		let data = Bytes(Vec::new());

		let encoded_response = self.client.state_call(call, data, None).await?;
		let decoded_response: Vec<(P::Number, P::Hash)> =
			Decode::decode(&mut &encoded_response.0[..]).map_err(SubstrateError::ResponseParseFailed)?;

		// If we parse an empty list of headers it means that bridge pallet has not been initalized
		// yet. Otherwise we expect to always have at least one header.
		decoded_response
			.last()
			.ok_or(SubstrateError::UninitializedBridgePallet)
			.map(|(num, hash)| HeaderId(*num, *hash))
	}

	async fn is_known_header(&self, id: HeaderIdOf<P>) -> Result<(HeaderIdOf<P>, bool), Self::Error> {
		let call = P::IS_KNOWN_BLOCK_METHOD.into();
		let data = Bytes(id.1.encode());

		let encoded_response = self.client.state_call(call, data, None).await?;
		let is_known_block: bool =
			Decode::decode(&mut &encoded_response.0[..]).map_err(SubstrateError::ResponseParseFailed)?;

		Ok((id, is_known_block))
	}

	async fn submit_headers(&self, mut headers: Vec<QueuedHeader<P>>) -> SubmittedHeaders<HeaderIdOf<P>, Self::Error> {
		debug_assert_eq!(
			headers.len(),
			1,
			"Substrate pallet only supports single header / transaction"
		);

		let header = headers.remove(0);
		let id = header.id();
		let submit_transaction_result = self
			.pipeline
			.make_submit_header_transaction(header)
			.and_then(|tx| self.client.submit_extrinsic(Bytes(tx.encode())))
			.await;

		match submit_transaction_result {
			Ok(_) => SubmittedHeaders {
				submitted: vec![id],
				incomplete: Vec::new(),
				rejected: Vec::new(),
				fatal_error: None,
			},
			Err(error) => SubmittedHeaders {
				submitted: Vec::new(),
				incomplete: Vec::new(),
				rejected: vec![id],
				fatal_error: Some(error),
			},
		}
	}

	async fn incomplete_headers_ids(&self) -> Result<HashSet<HeaderIdOf<P>>, Self::Error> {
		let call = P::INCOMPLETE_HEADERS_METHOD.into();
		let data = Bytes(Vec::new());

		let encoded_response = self.client.state_call(call, data, None).await?;
		let decoded_response: Vec<(P::Number, P::Hash)> =
			Decode::decode(&mut &encoded_response.0[..]).map_err(SubstrateError::ResponseParseFailed)?;

		let incomplete_headers = decoded_response
			.into_iter()
			.map(|(number, hash)| HeaderId(number, hash))
			.collect();
		Ok(incomplete_headers)
	}

	async fn complete_header(
		&self,
		id: HeaderIdOf<P>,
		completion: Justification,
	) -> Result<HeaderIdOf<P>, Self::Error> {
		let tx = self.pipeline.make_complete_header_transaction(id, completion).await?;
		self.client.submit_extrinsic(Bytes(tx.encode())).await?;
		Ok(id)
	}

	async fn requires_extra(&self, header: QueuedHeader<P>) -> Result<(HeaderIdOf<P>, bool), Self::Error> {
		Ok((header.id(), false))
	}
}
