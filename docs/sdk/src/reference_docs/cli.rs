//! # Command Line Arguments
//!
//!
//! Notes:
//!
//! - Command line arguments of a typical substrate based chain
//!   - how to find and learn them.
//! - How to extend them with your custom stuff.
//!
//! source https://docs.substrate.io/reference/command-line-tools/
//!
//! ## Command-line tools
//!
//! | Command Entry Point | Description |
//! |---------------------|-------------|
//! | [`archive`](#archive)          | Index and store all blocks, state, and transaction data for a Substrate-based chain in a relational SQL database. |
//! | `memory-profiler`   | Collect information about memory allocation and the behavior of blockchain applications over time. |
//! | `node-template`     | Start and manage a Substrate node preconfigured with a subset of commonly-used FRAME pallets. |
//! | `polkadot-launch`   | Launch a local Polkadot test network. |
//! | `polkadot-apps`     | Interact with Polkadot or a Substrate node using a browser. |
//! | `sidecar`           | Use a REST service to interact with blockchain nodes built using FRAME. |
//! | `srtool`            | Build WASM runtime in a deterministic way, allowing continuous integration pipelines and users to produce a strictly identical WASM runtime. |
//! | `subkey`            | Generate and manage public and private key pairs for accounts. |
//! | `subxt`             | Submit extrinsics to a Substrate node using RPC. |
//! | `try-runtime`       | Query a snapshot of runtime storage to retrieve state. |
//! | `tx-wrapper`        | Publish chain specific offline transaction generation libraries. |
//!
//! ### `archive`
//!  
//! The `archive` program is used to index all blocks, state, and transaction data for a
//! Substrate-based chain and store the indexed data in a relational SQL database. The database
//! created by the `archive` program mirrors all data from a running Substrate blockchain. After you
//! archive the data, you can use database tools to query and retrieve information from the SQL
//! database about the blockchain state. For examples of queries you might want to run against a
//! Substrate archive database, see [Useful queries](https://github.com/paritytech/substrate-archive/wiki/Useful-Queries).
//!
//! #### Before you begin
//!
//! Before you use `archive` to create a database for a Substrate-based chain, you need to prepare
//! your environment with the required files:
//!
//! - You must have PostgreSQL installed on the computer where you are running a Substrate node.
//!
//!   You can download PostgreSQL packages for different platforms from the PostgreSQL [Downloads](https://www.postgresql.org/download/) page.
//!
//!   Depending on your platform, you might be able to install PostgreSQL using a local package
//! manager.   For example, you can install a PostgreSQL package on a macOS computer by running
//! `brew install postgresql` in a Terminal.
//!
//! - You must have RabbitMQ or Docker Compose installed on the computer where you have PostgreSQL
//!   installed.
//!
//!   Depending on your platform, the instruction and system requirements for installing RabbitMQ or
//! Docker can vary.   For information about using [RabbitMQ](https://www.rabbitmq.com/) or [Docker](https://www.docker.com/),
//! see the [Setup](https://github.com/paritytech/substrate-archive/wiki/1-Setup)
//! `substrate-archive` wiki page.
//!
//! - Your Substrate chain must use RocksDB as its backend database.
//!
//! #### Install and configure
//!
//! To install the `substrate-archive-cli` program:
//!
//! 1. Open a terminal shell on your computer.
//!
//! 2. Clone the `substrate-archive` repository by running the following command:
//!
//!    ```
//!    git clone https://github.com/paritytech/substrate-archive.git
//!    ```
//!
//! 3. Change to the root directory of the `substrate-archive` repository by running the following
//!    command:
//!
//!    ```
//!    cd substrate-archive
//!    ```
//!
//! 4. Start the PostgreSQL database (`postgres`) and Postgre administrative process (`pgadmin`) on
//!    the Substrate node.
//!
//!    If you have Docker Compose, you can start the services automatically by running the
//! `docker-compose up -d` command.
//!
//! 5. Start your Substrate node, with `pruning` set to archive.  For example:  ```
//!    ./target/release/node-template --pruning=archive ```
//!
//! 6. Look at the current DBs: `psql -U postgres -hlocalhost -p6432`
//!
//! 7. Run `DATABASE_URL=postgres://postgres:123@localhost:6432/local_chain_db sqlx` database create
//!    in `substrate-archive/src` to create the database.
//!
//! 8. Set `CHAIN_DATA_DB="<your_path>"`.
//!
//! 9. Set up your `archive.conf` file:
//!
//!    - make sure to set your base bath to primary DB
//!    - tell it where the rocksdb is. State using CHAIN_DATA_DB
//!    - secondary DB is an optimization
//!    - postgres url (set to var if in prod)
//!
//! 10. (Optional) setup up logging and debugging.
//!
//! 11. Run a node template. Make sure you run it in `--release --dev base-path=/tmp/dir
//!     --pruning=archive`
//!
//! 12. Make a transaction with your node template.
//!
//! 13. Start up the `substrate-archive` node for your target chain:
//!    `cargo run --release -- -c archive-conf.toml --chain=polkadot`
//!
//! 14. Open a web browser and log in to the Postgres administrative console.
//!    
//!    - Default URL:  localhost:16543
//!    - Default user name: pgadmin4@pgadmin.org
//!    - Default password: admin
//!
//!
//! 15. Look at the reference to start making your queries.
//!
//!
//!
//!
//!
//!
//!
//!
//!
//! ## Extension through Community Tools
//! The [`substrate-cli-tools`](https://github.com/StakeKat/substrate-cli-tools) repository provides
//! a set of high-level tools to connect and consume Substrate-based chains. These tools leverage
//! the `py-substrate-interface`` library and offer functionalities like monitoring events as they
//! happen, decoding balances, and more. The library provided can also be reused to build your own
//! commands, offering a higher level of abstraction for easier use​​.
