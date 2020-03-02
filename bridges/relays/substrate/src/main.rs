mod bridge;
mod error;
mod params;
mod rpc;

use bridge::run_async;
use params::{Params, RPCUrlParam};

use clap::{App, Arg, value_t, values_t};
use futures::{prelude::*, channel};
use std::cell::Cell;
use std::process;

fn main() {
	let params = parse_args();
	env_logger::init();
	let exit = setup_exit_handler();

	let result = async_std::task::block_on(async move {
		run_async(params, exit).await
	});
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
				.multiple(true)
		)
		.get_matches();

	let base_path = value_t!(matches, "base-path", String)
		.unwrap_or_else(|e| e.exit());
	let rpc_urls = values_t!(matches, "rpc-url", RPCUrlParam)
		.unwrap_or_else(|e| e.exit());

	Params {
		base_path,
		rpc_urls,
	}
}

fn setup_exit_handler() -> Box<dyn Future<Output=()> + Unpin + Send> {
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

	Box::new(exit_receiver.map(|result| {
		result.expect(
			"exit_sender cannot be dropped as it is moved into a globally-referenced closure"
		)
	}))
}
