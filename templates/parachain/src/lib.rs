pub mod runtime {
    pub mod genesis_config_presets {
        // Import the module as part of the crate's directory structure
        include!("../runtime/src/genesis_config_presets.rs");
    }
}

pub use runtime::genesis_config_presets::PARACHAIN_ID;

#[docify::export_content]
fn embed_parachain_id() -> String {
    PARACHAIN_ID.to_string();
}
