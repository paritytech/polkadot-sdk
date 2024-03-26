// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use async_std::prelude::*;
use futures::{select, FutureExt};
use signal_hook::consts::*;
use signal_hook_async_std::Signals;
use structopt::StructOpt;

mod chain_schema;
mod detect_equivocations;
mod init_bridge;
mod relay_headers;
mod relay_headers_and_messages;
mod relay_messages;
mod relay_parachains;

/// The target that will be used when publishing logs related to this pallet.
pub const LOG_TARGET: &str = "bridge";

/// Parse relay CLI args.
pub fn parse_args() -> Command {
	Command::from_args()
}

/// Substrate-to-Substrate bridge utilities.
#[derive(StructOpt)]
#[structopt(about = "Substrate-to-Substrate relay")]
pub enum Command {
	/// Initialize on-chain bridge pallet with current header data.
	///
	/// Sends initialization transaction to bootstrap the bridge with current finalized block data.
	InitBridge(init_bridge::InitBridge),
	/// Start headers relay between two chains.
	///
	/// The on-chain bridge component should have been already initialized with
	/// `init-bridge` sub-command.
	RelayHeaders(relay_headers::RelayHeaders),
	/// Relay parachain heads.
	RelayParachains(relay_parachains::RelayParachains),
	/// Start messages relay between two chains.
	///
	/// Ties up to `Messages` pallets on both chains and starts relaying messages.
	/// Requires the header relay to be already running.
	RelayMessages(relay_messages::RelayMessages),
	/// Start headers and messages relay between two Substrate chains.
	///
	/// This high-level relay internally starts four low-level relays: two `RelayHeaders`
	/// and two `RelayMessages` relays. Headers are only relayed when they are required by
	/// the message relays - i.e. when there are messages or confirmations that needs to be
	/// relayed between chains.
	RelayHeadersAndMessages(Box<relay_headers_and_messages::RelayHeadersAndMessages>),
	/// Detect and report equivocations.
	///
	/// Parses the source chain headers that were synchronized with the target chain looking for
	/// equivocations. If any equivocation is found, it is reported to the source chain.
	DetectEquivocations(detect_equivocations::DetectEquivocations),
}

impl Command {
	// Initialize logger depending on the command.
	fn init_logger(&self) {
		use relay_utils::initialize::{initialize_logger, initialize_relay};

		match self {
			Self::InitBridge(_) |
			Self::RelayHeaders(_) |
			Self::RelayMessages(_) |
			Self::RelayHeadersAndMessages(_) => {
				initialize_relay();
			},
			_ => {
				initialize_logger(false);
			},
		}
	}

	/// Run the command.
	async fn do_run(self) -> anyhow::Result<()> {
		match self {
			Self::InitBridge(arg) => arg.run().await?,
			Self::RelayHeaders(arg) => arg.run().await?,
			Self::RelayParachains(arg) => arg.run().await?,
			Self::RelayMessages(arg) => arg.run().await?,
			Self::RelayHeadersAndMessages(arg) => arg.run().await?,
			Self::DetectEquivocations(arg) => arg.run().await?,
		}
		Ok(())
	}

	/// Run the command.
	pub async fn run(self) {
		self.init_logger();

		let exit_signals = match Signals::new([SIGINT, SIGTERM]) {
			Ok(signals) => signals,
			Err(e) => {
				log::error!(target: LOG_TARGET, "Could not register exit signals: {}", e);
				return
			},
		};
		let run = self.do_run().fuse();
		futures::pin_mut!(exit_signals, run);

		select! {
			signal = exit_signals.next().fuse() => {
				log::info!(target: LOG_TARGET, "Received exit signal {:?}", signal);
			},
			result = run => {
				if let Err(e) = result {
					log::error!(target: LOG_TARGET, "substrate-relay: {}", e);
				}
			},
		}
	}
}
