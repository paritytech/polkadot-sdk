//! # Glossary
//!
//! #### State
//!
//! The data around which the blockchain network wishes to come to consensus. Also
//! referred to as "onchain data", "onchain storage" or sometimes just "storage". In UTXO based
//! blockchains, is referred to as "ledger".
//!
//! **Synonyms**: Onchain data, Onchain storage, Storage, Ledger
//!
//! #### State Transition Function
//!
//! The WASM Blob that dictates how the blockchain should transition its state upon encountering new
//! blocks.
//!
//! #### Host
//!
//! The environment that hosts and executes the [state transition function's WASM
//! blob](#state-transition-function).
//!
//! #### Node
//!
//! The full software artifact that contains the [host](#host), but importantly also all the other
//! modules needed to be part of a blockchain network, such as peer-to-peer networking, database and
//! such.
//!
//! **Synonyms**: Client
//!
//! #### Light Node
//!
//! Same as [node](#nodes), but when capable of following the network only through listening to
//! block headers. Usually capable of running in more constrained environments, such as an embedded
//! device, phone, or a web browser.
//!
//! **Synonyms**: Light Client
//!
//! #### Offchain
//!
//! Refers to operations conducted outside the blockchain's consensus mechanism. They are essential
//! for enhancing scalability and efficiency, enabling activities like data fetching and computation
//! without bloating the blockchain state.
//!
//! #### Host Functions:
//!
//! Host functions are the node's API, these are functions provided by the runtime environment (the
//! [host](#host)) to the Wasm runtime. These functions allow the Wasm code to interact with and
//! perform operations on the [node](#node), like accessing the blockchain state.
//!
//! #### Runtime API:
//!
//! This is the API of the runtime, it acts as a communication bridge between the runtime and the
//! node, serving as the exposed interface that facilitates their interactions.
//!
//! #### Dispatchable:
//!
//! Dispatchables are [function objects](https://en.wikipedia.org/wiki/Function_object) that act as
//! the entry points in [FRAME](frame) pallets. They can be called by internal or external entities
//! to interact with the blockchain's state. They are a core aspect of the runtime logic, handling
//! transactions and other state-changing operations.
//!
//! **Synonyms**: Callable
//!
//! #### Extrinsic
//!
//! An extrinsic is a general term for a piece of data that is originated outside of the runtime,
//! included into a block and leads to some action. This includes user-initiated transactions as
//! well as inherents which are placed into the block by the block-builder.
//!
//! #### Pallet
//!
//! Similar to software modules in traditional programming, [FRAME](frame) pallets in Substrate are
//! modular components that encapsulate distinct functionalities or business logic. Just as
//! libraries or modules are used to build and extend the capabilities of a software application,
//! pallets are the foundational building blocks for constructing a blockchain's runtime with frame.
//! They enable the creation of customizable and upgradeable networks, offering a composable
//! framework for a Substrate-based blockchain. Each pallet can be thought of as a plug-and-play
//! module, enhancing the blockchain's functionality in a cohesive and integrated manner.
//!
//! #### Full Node
//!
//! It is a node that prunes historical states, keeping only recent finalized block states to reduce
//! storage needs. Full nodes provide current chain state access and allow direct submission and
//! validation of extrinsics, maintaining network decentralization.
//!
//! #### Archive Node
//!
//! An archive node is a specialized node that maintains a complete history of all block states and
//! transactions. Unlike a full node, it does not prune historical data, ensuring full access to the
//! entire blockchain history. This makes it essential for detailed blockchain analysis and
//! historical queries, but requires significantly more storage capacity.
//!
//! #### Validator
//!
//! A validator is a node that participates in the consensus mechanism of the network.
//! Its role includes block production, transaction validation, network integrity and security
//! maintenance.
//!
//! #### Collator
//!
//! A collator is a node that is responsible for producing candidate blocks for the validators.
//! Collators are similar to validators on any other blockchain but, they do not need to provide
//! security guarantees as the Relay Chain handles this.
//!
//! #### Parachain
//!
//! Short for "parallelized chain" a parachain is a specialized blockchain that runs in parallel to
//! the Relay Chain (Polkadot, Kusama, etc.), benefiting from the shared security and
//! interoperability features of it.
//!
//! **Synonyms**: AppChain
//!
//! #### PVF
//! The Parachain Validation Function (PVF) is the current runtime Wasm for a parachain that is
//! stored on the Relay chain. It is an essential component in the Polkadot ecosystem, encapsulating
//! the validation logic for each parachain. The PVF is executed by validators to verify the
//! correctness of parachain blocks. This is critical for ensuring that each block follows the logic
//! set by its respective parachain, thus maintaining the integrity and security of the entire
//! network.
//!
//! **Synonyms**: Parachain Validation Function
