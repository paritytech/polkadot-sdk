// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Authority discovery errors.

/// AuthorityDiscovery Result.
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for the authority discovery module.
#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum Error {
	/// Failed to verify a dht payload with the given signature.
	VerifyingDhtPayload,
	/// Failed to hash the authority id to be used as a dht key.
	HashingAuthorityId(libp2p::core::multiaddr::multihash::EncodeError),
	/// Failed calling into the Substrate runtime.
	CallingRuntime(client::error::Error),
	/// Failed signing the dht payload via the Substrate runtime.
	SigningDhtPayload,
	/// From the Dht we only get the hashed authority id. In order to retrieve the actual authority id and to ensure it
	/// is actually an authority, we match the hash against the hash of the authority id of all other authorities. This
	/// error is the result of the above failing.
	MatchingHashedAuthorityIdWithAuthorityId,
	/// Failed to set the authority discovery peerset priority group in the peerset module.
	SettingPeersetPriorityGroup(String),
	/// Failed to encode a dht payload.
	Encoding(prost::EncodeError),
	/// Failed to decode a dht payload.
	Decoding(prost::DecodeError),
	/// Failed to parse a libp2p multi address.
	ParsingMultiaddress(libp2p::core::multiaddr::Error),
	/// Tokio timer error.
	PollingTokioTimer(tokio_timer::Error)
}
