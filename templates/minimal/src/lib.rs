//! # Minimal Template
//!
//! This is a minimal template for creating a blockchain using the Polkadot SDK.
//!
//! ## Components
//!
//! The template consists of the following components:
//!
//! ### Node
//!
//! A minimal blockchain [`node`](`minimal_template_node`) that is capable of running a
//! runtime. It uses a simple chain specification, provides an option to choose Manual or
//! InstantSeal for consensus and exposes a few commands to interact with the node.
//!
//! ### Runtime
//!
//! A minimal [`runtime`](`minimal_template_runtime`) (or a state transition function) that
//! is capable of being run on the node. It is built using the [`FRAME`](`frame`) framework
//! that enables the composition of the core logic via separate modules called "pallets".
//! FRAME defines a complete DSL for building such pallets and the runtime itself.
//!
//! #### Transaction Fees
//!
//! The runtime charges a transaction fee for every transaction that is executed. The fee is
//! calculated based on the weight of the transaction (accouting for the execution time) and
//! length of the call data. Please refer to
//! [`benchmarking docs`](`polkadot_sdk_docs::reference_docs::frame_benchmarking_weight`) for
//! more information on how the weight is calculated.
//!
//! This template sets the fee as independent of the weight of the extrinsic and fixed for any
//! length of the call data for demo purposes.
//!
//! ### Pallet
//!
//! A minimal [`pallet`](`pallet_minimal_template`) that is built using FRAME. It is a unit of
//! encapsulated logic that has a clearly defined responsibility and can be linked to other pallets.
//!
//! ## Getting Started
//!
//! To get started with the template, follow the steps below:
//!
//! ### Build the Node
//!
//! Build the node using the following command:
//!
//! ```bash
//! cargo build -p minimal-template-node --release
//! ```
//!
//! ### Run the Node
//!
//! Run the node using the following command:
//!
//! ```bash
//! ./target/release/minimal-template-node --dev
//! ```
//!
//! ### CLI Options
//!
//! The node exposes a few options that can be used to interact with the node. To see the list of
//! available options, run the following command:
//!
//! ```bash
//! ./target/release/minimal-template-node --help
//! ```
//!
//! #### Consensus Algorithm
//!
//! In order to run the node with a specific consensus algorithm, use the `--consensus` flag. For
//! example, to run the node with ManualSeal consensus with a block time of 5000ms, use the
//! following command:
//!
//! ```bash
//! ./target/release/minimal-template-node --dev --consensus manual-seal-5000
//! ```
