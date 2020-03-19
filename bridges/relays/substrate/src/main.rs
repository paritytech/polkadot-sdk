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

mod bridge;
mod error;
mod params;
mod rpc;

use bridge::run_async;
use params::{Params, RPCUrlParam};

use clap::{value_t, values_t, App, Arg};
use futures::{channel, prelude::*};
use std::cell::Cell;
use std::process;

fn main() {
	let params = parse_args();
	env_logger::init();
	let exit = setup_exit_handler();

	let result = async_std::task::block_on(async move { run_async(params, exit).await });
	if let Err(err) = result {
		log::error!("{}", err);
		process::exit(1);
	}
}

fn parse_args() -> Params {
	let matches = App::new("substrate-bridge")
		.version("1.0")
		.author("Parity Technologies")
		.about("Bridges Substrates, duh")
		.arg(
			Arg::with_name("base-path")
				.long("base-path")
				.value_name("DIRECTORY")
				.required(true)
				.help("Sets the base path")
				.takes_value(true),
		)
		.arg(
			Arg::with_name("rpc-url")
				.long("rpc-url")
				.value_name("HOST[:PORT]")
				.help("The URL of a bridged Substrate node")
				.takes_value(true)
				.multiple(true),
		)
		.get_matches();

	let base_path = value_t!(matches, "base-path", String).unwrap_or_else(|e| e.exit());
	let rpc_urls = values_t!(matches, "rpc-url", RPCUrlParam).unwrap_or_else(|e| e.exit());

	Params { base_path, rpc_urls }
}

fn setup_exit_handler() -> Box<dyn Future<Output = ()> + Unpin + Send> {
	let (exit_sender, exit_receiver) = channel::oneshot::channel();
	let exit_sender = Cell::new(Some(exit_sender));

	ctrlc::set_handler(move || {
		if let Some(exit_sender) = exit_sender.take() {
			if let Err(()) = exit_sender.send(()) {
				log::warn!("failed to send exit signal");
			}
		}
	})
	.expect("must be able to set Ctrl-C handler");

	Box::new(
		exit_receiver.map(|result| {
			result.expect("exit_sender cannot be dropped as it is moved into a globally-referenced closure")
		}),
	)
}
