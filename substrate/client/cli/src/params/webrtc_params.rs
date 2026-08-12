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

use clap::{ArgMatches, Args, Command, FromArgMatches};

/// The mutually exclusive `--enable-webrtc`/`--disable-webrtc` flags.
///
/// Only used to derive the parser for [`WebRtcParams`].
#[derive(Debug, Clone, Copy, Args)]
struct WebRtcFlags {
	/// Do not listen on WebRTC addresses by default. Implied on a validator or
	/// collator (both para- & relaychain node sides).
	///
	/// Only affects the default listen addresses — WebRTC addresses passed
	/// via `--listen-addr` are always used.
	#[arg(long, conflicts_with = "enable_webrtc")]
	disable_webrtc: bool,

	/// Listen on WebRTC addresses even on a validator or collator (on para-
	/// & relaychain side of the node depending on the position of the flag).
	///
	/// Only applies if no explicit `--listen-addr` is passed.
	#[arg(long)]
	enable_webrtc: bool,
}

impl From<WebRtcFlags> for Option<bool> {
	fn from(flags: WebRtcFlags) -> Self {
		match (flags.enable_webrtc, flags.disable_webrtc) {
			(true, false) => Some(true),
			(false, true) => Some(false),
			(false, false) => None,
			(true, true) => unreachable!("`*_webrtc` flags are mutually exclusive; qed"),
		}
	}
}

/// Parameters controlling WebRTC listen addresses.
///
/// Parses [`WebRtcFlags`] into a single tri-state value.
#[derive(Debug, Clone, Copy, Default)]
pub struct WebRtcParams {
	/// `Some(true)`/`Some(false)` if explicitly enabled/disabled on the command
	/// line, `None` to use the role-based default.
	pub enable: Option<bool>,
}

impl FromArgMatches for WebRtcParams {
	fn from_arg_matches(matches: &ArgMatches) -> Result<Self, clap::Error> {
		Ok(Self { enable: WebRtcFlags::from_arg_matches(matches)?.into() })
	}

	fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), clap::Error> {
		if let Some(enable) = Option::<bool>::from(WebRtcFlags::from_arg_matches(matches)?) {
			self.enable = Some(enable);
		}
		Ok(())
	}
}

impl Args for WebRtcParams {
	fn augment_args(cmd: Command) -> Command {
		WebRtcFlags::augment_args(cmd)
	}

	fn augment_args_for_update(cmd: Command) -> Command {
		WebRtcFlags::augment_args_for_update(cmd)
	}
}
