use clap::Parser;
use sc_service::{chain_ops, ChainSpec};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use crate::error::{Error, Result};

/// Export the embedded chain-spec to a JSON file.
///
/// This command loads the embedded chain-spec (for example, when you pass
/// `--chain /full/path/to/asset-hub-rococo`) and exports it to a JSON file. If `--output`
/// is not provided, the JSON is printed to stdout.
#[derive(Debug, Clone, Parser)]
pub struct ExportChainSpecCmd {
    /// The chain spec identifier to export.
    #[arg(long, default_value = "local")]
    pub chain: String,

    /// Output file path. If omitted, prints to stdout.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Export in raw genesis storage format.
    #[arg(long)]
    pub raw: bool,
}

impl ExportChainSpecCmd {
    pub fn run(&self, spec: Box<dyn ChainSpec>) -> Result<()> {
        let json = chain_ops::build_spec(&*spec, self.raw)
            .map_err(|e| format!("{}", e))?;
        if let Some(ref path) = self.output {
            fs::write(path, json).map_err(|e| format!("{}", e))?;
            eprintln!("Exported chain spec to {}", path.display());
        } else {
            io::stdout()
                .write_all(json.as_bytes())
                .map_err(|e| format!("{}", e))?;
        }
        Ok(())
    }
}
