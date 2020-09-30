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

//! Deal with CLI args of substrate-to-substrate relay.

use structopt::StructOpt;

/// Parse relay CLI args.
pub fn parse_args() -> Command {
	Command::from_args()
}

/// Substrate-to-Substrate relay CLI args.
#[derive(StructOpt)]
#[structopt(about = "Substrate-to-Substrate relay")]
pub enum Command {
	MillauHeadersToRialto {
		#[structopt(flatten)]
		millau: MillauConnectionParams,
		#[structopt(flatten)]
		rialto: RialtoConnectionParams,
		#[structopt(flatten)]
		rialto_sign: RialtoSigningParams,
	},
}

macro_rules! declare_chain_options {
	($chain:ident, $chain_prefix:ident) => {
		paste::item! {
			#[doc = $chain " connection params."]
			#[derive(StructOpt)]
			pub struct [<$chain ConnectionParams>] {
				#[doc = "Connect to " $chain " node at given host."]
				pub [<$chain_prefix _host>]: String,
				#[doc = "Connect to " $chain " node at given port."]
				pub [<$chain_prefix _port>]: u16,
			}

			#[doc = $chain " signing params."]
			#[derive(StructOpt)]
			pub struct [<$chain SigningParams>] {
				#[doc = "The SURI of secret key to use when transactions are submitted to the " $chain " node."]
				pub [<$chain_prefix _signer>]: String,
				#[doc = "The password for the SURI of secret key to use when transactions are submitted to the " $chain " node."]
				pub [<$chain_prefix _signer_password>]: Option<String>,
			}
		}
	};
}

declare_chain_options!(Rialto, rialto);
declare_chain_options!(Millau, millau);
