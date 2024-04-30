pub mod environment;
pub mod paras;

#[subxt::subxt(runtime_metadata_path = "artifacts/polkadot_metadata_full.scale")]
pub mod polkadot {}

pub type Error = Box<dyn std::error::Error>;
