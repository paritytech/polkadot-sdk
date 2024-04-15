
//! Helpers functions to get configuration (e.g. Provider and images) from the env vars

use std::env;

// We should find a way to keep this update
const DEFAULT_POLKADOT_IMAGE: &str = "docker.io/paritypr/polkadot-debug:master-12eb285d";
const DEFAULT_MALUS_IMAGE: &str = "docker.io/paritypr/malus:master-12eb285d";
const DEFAULT_CUMULUS_IMAGE: &str = "docker.io/paritypr/polkadot-parachain-debug:master-12eb285d";
const DEFAULT_COLANDER_IMAGE: &str = "docker.io/paritypr/colander:master-12eb285d";

#[derive(Debug, Default)]
pub struct Images {
	pub polkadot: String,
	pub malus: String,
	pub cumulus: String,
	pub colander: String,
}

pub enum Provider {
	Native,
	K8s,
}

// Use `kubernetes` as default provider
impl From<String> for Provider {
	fn from(value: String) -> Self {
		if value.to_ascii_lowercase() == "native" {
			Provider::Native
		} else {
			Provider::K8s
		}
	}
}

pub fn get_images_from_env() -> Images {
	let polkadot = env::var("ZOMBIENET_INTEGRATION_TEST_IMAGE").unwrap_or(DEFAULT_POLKADOT_IMAGE.into());
	let malus = env::var("MALUS_IMAGE").unwrap_or(DEFAULT_MALUS_IMAGE.into());
	let cumulus = env::var("CUMULUS_IMAGE").unwrap_or(DEFAULT_CUMULUS_IMAGE.into());
	// adder/undying
	let colander = env::var("COLANDER_IMAGE").unwrap_or(DEFAULT_COLANDER_IMAGE.into());
	Images { polkadot, malus, cumulus, colander }
}

pub fn get_provider_from_env() -> Provider {
	env::var("ZOMBIE_PROVIDER").unwrap_or_default().into()
}
