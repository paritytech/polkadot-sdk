#![doc = docify::embed!("../../templates/parachain/node/src/service.rs", wasm_executor)]
#![doc = docify::embed!("../../templates/parachain/node/src/service.rs", component_instantiation)]
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", template_signed_extra)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

// [`substrate documentation`]: crate::polkadot_sdk::substrate#anatomy-of-a-binary-crate
