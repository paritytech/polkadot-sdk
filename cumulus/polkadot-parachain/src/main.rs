// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Polkadot parachain node.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

pub(crate) fn examples(executable_name: String) -> String {
	color_print::cformat!(
		r#"<bold><underline>Examples:</></>

   <bold>{0} --chain para.json --sync warp -- --chain relay.json --sync warp</>
        Launch a warp-syncing full node of a given para's chain-spec, and a given relay's chain-spec.

	<green><italic>The above approach is the most flexible, and the most forward-compatible way to spawn an omni-node.</></>

	You can find the chain-spec of some networks in:
	https://paritytech.github.io/chainspecs

   <bold>{0} --chain asset-hub-polkadot --sync warp -- --chain polkadot --sync warp</>
        Launch a warp-syncing full node of the <italic>Asset Hub</> parachain on the <italic>Polkadot</> Relay Chain.

   <bold>{0} --chain asset-hub-kusama --sync warp --relay-chain-rpc-url ws://rpc.example.com -- --chain kusama</>
        Launch a warp-syncing full node of the <italic>Asset Hub</> parachain on the <italic>Kusama</> Relay Chain.
        Uses <italic>ws://rpc.example.com</> as remote relay chain node.
 "#,
		executable_name,
	)
}

mod chain_spec;
mod cli;
mod command;
mod common;
mod fake_runtime_api;
mod rpc;
mod service;

fn main() -> color_eyre::eyre::Result<()> {
	color_eyre::install()?;
	Ok(command::run()?)
}
