//! # Substrate CLI
//!
//! Let's see some examples of typical CLI arguments used when setting up and running a
//! Substrate-based blockchain. We use the [`substrate-node-template`](https://github.com/substrate-developer-hub/substrate-node-template)
//! on these examples.
//!
//! #### Checking the available CLI arguments
//! ```bash
//! ./target/debug/node-template --help
//! ```
//! - `--help`: Displays the available CLI arguments.
//!
//! #### Starting a Local Substrate Node in Development Mode
//! ```bash
//! ./target/release/node-template \
//! --dev
//! ```
//! - `--dev`: Runs the node in development mode, using a pre-defined development chain
//!   specification.
//! This mode ensures a fresh state by deleting existing data on restart.
//!
//! #### Generating Custom Chain Specification
//! ```bash
//! ./target/debug/node-template \
//! build-spec \
//! --disable-default-bootnode \
//! --chain local \
//! > customSpec.json
//! ```
//!
//! - `build-spec`: A subcommand to generate a chain specification file.
//! - `--disable-default-bootnode`: Disables the default bootnodes in the node template.
//! - `--chain local`: Indicates the chain specification is for a local development chain.
//! - `> customSpec.json`: Redirects the output into a customSpec.json file.
//!
//! #### Converting Chain Specification to Raw Format
//! ```bash
//! ./target/debug/node-template build-spec \
//! --chain=customSpec.json \
//! --raw \
//! --disable-default-bootnode \
//! > customSpecRaw.json
//! ```
//!
//! - `--chain=customSpec.json`: Uses the custom chain specification as input.
//! - `--disable-default-bootnode`: Disables the default bootnodes in the node template.
//! - `--raw`: Converts the chain specification into a raw format with encoded storage keys.
//! - `> customSpecRaw.json`: Outputs to customSpecRaw.json.
//!
//! #### Starting the First Node in a Private Network
//! ```bash
//! ./target/debug/node-template \
//!   --base-path /tmp/node01 \
//!   --chain ./customSpecRaw.json \
//!   --port 30333 \
//!   --ws-port 9945 \
//!   --rpc-port 9933 \
//!   --telemetry-url "wss://telemetry.polkadot.io/submit/ 0" \
//!   --validator \
//!   --rpc-methods Unsafe \
//!   --name MyNode01
//! ```
//!
//! - `--base-path`: Sets the directory for node data.
//! - `--chain`: Specifies the chain specification file.
//! - `--port`: TCP port for peer-to-peer communication.
//! - `--ws-port`: WebSocket port for RPC.
//! - `--rpc-port`: HTTP port for JSON-RPC.
//! - `--telemetry-url`: Endpoint for sending telemetry data.
//! - `--validator`: Indicates the node’s participation in block production.
//! - `--rpc-methods Unsafe`: Allows potentially unsafe RPC methods.
//! - `--name`: Sets a human-readable name for the node.
//!
//! #### Adding a Second Node to the Network
//! ```bash
//! ./target/release/node-template \
//!   --base-path /tmp/bob \
//!   --chain local \
//!   --bob \
//!   --port 30334 \
//!   --rpc-port 9946 \
//!   --telemetry-url "wss://telemetry.polkadot.io/submit/ 0" \
//!   --validator \
//!   --bootnodes /ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp
//! ```
//!
//! - `--base-path`: Sets the directory for node data.
//! - `--chain`: Specifies the chain specification file.
//! - `--bob`: Initializes the node with the session keys of the "Bob" account.
//! - `--port`: TCP port for peer-to-peer communication.
//! - `--rpc-port`: HTTP port for JSON-RPC.
//! - `--telemetry-url`: Endpoint for sending telemetry data.
//! - `--validator`: Indicates the node’s participation in block production.
//! - `--bootnodes`: Specifies the address of the first node for peer discovery. Nodes should find
//!   each other using mDNS. This command needs to be used if they don't find each other.
//!
//! ---
//!
//! > If you are interested in learning how to extend the CLI with your custom arguments, you can
//! > check out the [Customize your Substrate chain CLI](https://www.youtube.com/watch?v=IVifko1fqjw)
//! > seminar.
//! > Please note that the seminar is based on an older version of Substrate, and [Clap](https://docs.rs/clap/latest/clap/)
//! > is now used instead of [StructOpt](https://docs.rs/structopt/latest/structopt/) for parsing
//! > CLI arguments.
