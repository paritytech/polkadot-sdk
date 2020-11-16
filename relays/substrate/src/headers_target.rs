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

use async_trait::async_trait;
use codec::{Decode, Encode};
use futures::TryFutureExt;
use headers_relay::{
	sync_loop::TargetClient,
	sync_types::{HeaderIdOf, HeadersSyncPipeline, QueuedHeader, SubmittedHeaders},
};
use relay_substrate_client::{Chain, Client, Error as SubstrateError};
use relay_utils::HeaderId;
use sp_core::Bytes;
use sp_runtime::Justification;
use std::collections::HashSet;

/// Headers sync pipeline for Substrate <-> Substrate relays.
#[async_trait]
pub trait SubstrateHeadersSyncPipeline: HeadersSyncPipeline {
	/// Name of the `best_block` runtime method.
	const BEST_BLOCK_METHOD: &'static str;
	/// Name of the `finalized_block` runtime method.
	const FINALIZED_BLOCK_METHOD: &'static str;
	/// Name of the `is_known_block` runtime method.
	const IS_KNOWN_BLOCK_METHOD: &'static str;
	/// Name of the `incomplete_headers` runtime method.
	const INCOMPLETE_HEADERS_METHOD: &'static str;

	/// Signed transaction type.
	type SignedTransaction: Send + Sync + Encode;

	/// Make submit header transaction.
	async fn make_submit_header_transaction(
		&self,
		header: QueuedHeader<Self>,
	) -> Result<Self::SignedTransaction, SubstrateError>;

	/// Make completion transaction for the header.
	async fn make_complete_header_transaction(
		&self,
		id: HeaderIdOf<Self>,
		completion: Justification,
	) -> Result<Self::SignedTransaction, SubstrateError>;
}

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

		const WARNING_MSG: &str = "Parsed an empty list of headers, we should always have at least
									one. Has the bridge pallet been initialized yet?";
		let best_header = decoded_response
			.last()
			.ok_or_else(|| SubstrateError::ResponseParseFailed(WARNING_MSG.into()))?;
		let best_header_id = HeaderId(best_header.0, best_header.1);
		Ok(best_header_id)
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
