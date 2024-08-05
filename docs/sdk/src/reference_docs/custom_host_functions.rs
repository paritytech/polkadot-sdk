//! # Custom Host Functions
//!
//! Host functions are functions that the wasm instance can use to communicate with the node. Learn
//! more about this in [`crate::reference_docs::wasm_meta_protocol`].
//!
//! ## Finding Host Functions
//!
//! To declare a set of functions as host functions, you need to use the `#[runtime_interface]`
//! ([`sp_runtime_interface`]) attribute macro. The most notable set of host functions are those
//! that allow the runtime to access the chain state, namely [`sp_io::storage`]. Some other notable
//! host functions are also defined in [`sp_io`].
//!
//! ## Adding New Host Functions
//!
//! > Adding a new host function is a big commitment and should be done with care. Namely, the nodes
//! > in the network need to support all host functions forever in order to be able to sync
//! > historical blocks.
//!
//! Adding host functions is only possible when you are using a node-template, so that you have
//! access to the boilerplate of building your node.
//!
//! A group of host functions can always be grouped to gether as a tuple:
#![doc = docify::embed!("../../substrate/primitives/io/src/lib.rs", SubstrateHostFunctions)]
//!
//! The host functions are attached to the node side's [`sc_executor::WasmExecutor`]. For example in
//! the minimal template, the setup looks as follows:
#![doc = docify::embed!("../../templates/minimal/node/src/service.rs", FullClient)]
