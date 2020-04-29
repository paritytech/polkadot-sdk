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

use bridge_node_runtime::{BlockNumber, Hash, Header as RuntimeHeader};
use codec::Decode;
use sp_blockchain::Error as ClientError;

/// Builtin errors.
#[derive(Debug)]
pub enum Error {
	/// Failed to decode Substrate header.
	HeaderDecode(codec::Error),
	/// Failed to decode best voters set.
	BestVotersDecode(codec::Error),
	/// Failed to decode finality proof.
	FinalityProofDecode(codec::Error),
	/// Failed to verify justification.
	JustificationVerify(ClientError),
}

/// Substrate header.
#[derive(Debug)]
pub struct Header {
	/// Header hash.
	pub hash: Hash,
	/// Parent header hash.
	pub parent_hash: Hash,
	/// Header number.
	pub number: BlockNumber,
	/// GRANDPA validators change signal.
	pub signal: Option<ValidatorsSetSignal>,
}

/// GRANDPA validators set change signal.
#[derive(Debug)]
pub struct ValidatorsSetSignal {
	/// Signal delay.
	pub delay: BlockNumber,
	/// New validators set.
	pub validators: Vec<u8>,
}

/// Parse Substrate header.
pub fn parse_substrate_header(raw_header: &[u8]) -> Result<Header, Error> {
	RuntimeHeader::decode(&mut &raw_header[..])
		.map(|header| Header {
			hash: header.hash(),
			parent_hash: header.parent_hash,
			number: header.number,
			signal: None, // TODO: parse me
		})
		.map_err(Error::HeaderDecode)
}

/// Verify GRANDPA finality proof.
pub fn verify_substrate_finality_proof(
	_best_set_id: u64,
	_raw_best_voters: &[u8],
	_raw_best_header: &[u8],
	_raw_headers: &[&[u8]],
	_raw_finality_proof: &[u8],
) -> Result<(usize, usize), Error> {
	Err(Error::JustificationVerify(ClientError::Msg(
		"Not yet implemented".into(),
	))) // TODO: implement me
}
