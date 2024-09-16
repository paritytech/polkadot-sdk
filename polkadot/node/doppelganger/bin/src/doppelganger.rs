use clap::Parser;
use color_eyre::eyre;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[allow(missing_docs)]
struct DoppelgangerCli {
	#[allow(missing_docs)]
	#[clap(flatten)]
	pub inner_cli: polkadot_cli::Cli,

	/// json
	pub json_overrides: Option<PathBuf>,
}

fn main() -> eyre::Result<()> {
	color_eyre::install()?;
	let cli = DoppelgangerCli::parse();
	println!("{:?}", cli);
	polkadot_cli::run_doppelganger(cli.inner_cli)?;
	Ok(())
}
