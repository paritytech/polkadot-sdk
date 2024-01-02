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

//! Contains mock implementations of `ChainSync` and 'BlockDownloader'.

use crate::block_relay_protocol::{BlockDownloader as BlockDownloaderT, BlockResponseError};

use futures::channel::oneshot;
use libp2p::PeerId;
use sc_network::RequestFailure;
use sc_network_common::sync::message::{BlockData, BlockRequest};
use sp_runtime::traits::Block as BlockT;

mockall::mock! {
	pub BlockDownloader<Block: BlockT> {}

	#[async_trait::async_trait]
	impl<Block: BlockT> BlockDownloaderT<Block> for BlockDownloader<Block> {
		async fn download_blocks(
			&self,
			who: PeerId,
			request: BlockRequest<Block>,
		) -> Result<Result<Vec<u8>, RequestFailure>, oneshot::Canceled>;
		fn block_response_into_blocks(
			&self,
			request: &BlockRequest<Block>,
			response: Vec<u8>,
		) -> Result<Vec<BlockData<Block>>, BlockResponseError>;
	}
}
