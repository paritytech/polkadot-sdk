//! Substrate Parachain Node Template CLI

#![warn(missing_docs)]

mod builder;
mod cli;
mod command;
mod rpc;
mod service;
mod standards;
use crate::builder::Builder;

fn main() -> sc_cli::Result<()> {
	Builder::default().node_type(builder::NodeType::Solochain).build()
}
