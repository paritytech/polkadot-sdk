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

//! Polkadot parachain-omni-node.

#![warn(missing_docs)]
#![warn(unused_extern_crates)]

pub(crate) const EXAMPLES: &str = color_print::cstr!(
	r#"<bold><underline>Examples:</></>

   <bold>polkadot-parachain-omni-node --chain para.json --sync warp -- --chain relay.json --sync warp</>
        Launch a warp-syncing full node of a given para's chain-spec, and a given relay's chain-spec.

	<green><italic>The above approach is the most flexible, and the most forward-compatible way to spawn an omni-node.</></>

	You can find the chain-spec of some networks in:

	https://paritytech.github.io/chainspecs

   <bold>polkadot-parachain-omni-node --chain asset-hub-polkadot --sync warp -- --chain polkadot --sync warp</>
        Launch a warp-syncing full node of the <italic>Asset Hub</> parachain on the <italic>Polkadot</> Relay Chain.

   <bold>polkadot-parachain-omni-node --chain asset-hub-kusama --sync warp --relay-chain-rpc-url ws://rpc.example.com -- --chain polkadot</>
        Launch a warp-syncing full node of the <italic>Asset Hub</> parachain on the <italic>Kusama</> Relay Chain.
        Uses <italic>ws://rpc.example.com</> as remote relay chain node.
 "#
);

pub(crate) const BANNER: &str = color_print::cstr!(
	r#"
 _____                     _           _
|  __ \                   | |         (_)
| |__) |_ _ _ __ __ _  ___| |__   __ _ _ _ __
|  ___/ _` | '__/ _` |/ __| '_ \ / _` | | '_ \
| |  | (_| | | | (_| | (__| | | | (_| | | | | |
|_|___\__,_|_|  \__,_|\___|_| |_|\__,_|_|_| |_|
/ __ \                (_) | \ | |         | |
| |  | |_ __ ___  _ __  _  |  \| | ___   __| | ___
| |  | | '_ ` _ \| '_ \| | | . ` |/ _ \ / _` |/ _ \
| |__| | | | | | | | | | | | |\  | (_) | (_| |  __/
 \____/|_| |_| |_|_| |_|_| |_| \_|\___/ \__,_|\___|

<i>
Formerly know as polkadot-parachain, now called polkadot-parachain-omni-node, equipped with running more parachains in the Polkadot networks.
</>

Run with --help to see example usages.

Please refer to the following resources to learn more about the omni-node:
- https://forum.polkadot.network/t/polkadot-parachain-omni-node-gathering-ideas-and-feedback/7823/4
- https://github.com/paritytech/polkadot-sdk/issues/5
- sdk-docs

<y>
Warning: The degree of flexibility of this node is still expanding, as explained in the issue
above. Please only use this if your parachain is using <bold>Aura</> consensus, and has not special
node side specialization. Later versions of the omni-node could support customizing details
such as RPCs, inherents and consensus.
</>
"#,
);

mod chain_spec;
mod cli;
mod command;
mod common;
mod fake_runtime_api;
mod rpc;
mod service;

fn main() -> sc_cli::Result<()> {
	command::run()
}
