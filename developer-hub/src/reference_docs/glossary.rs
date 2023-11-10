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
//! without bloating the blockchain.
//!
//! #### Host Function:
//!
//! Host functions are functions provided by the runtime environment (the [host](#host)) to the Wasm
//! runtime. These functions allow the Wasm code to interact with and perform operations on the
//! [node](#node), like accessing the blockchain state.
//!
//! #### Runtime API:
//!
//! The runtime API acts as a communication bridge between the runtime and the node, serving as the
//! exposed interface that facilitates their interactions.
//!
//! #### Dispatchable:
//!
//! Dispatchables are functions that can be called by external entities, such as users or external
//! systems, to interact with the blockchain's state. They are a core aspect of the runtime logic,
//! handling transactions and other state-changing operations.
//!
//! **Synonyms**: Callable
//!
//! #### Extrinsic
//!
//! An extrinsic is a general term for a piece of data that is originated outside of the runtime and
//! fed into the it as a part of the block-body. This includes user-initiated transactions as well
//! as inherents which are placed into the block by the block-builder.
//!
//! #### Pallet
//!
//! FRAME pallets are modular components that encapsulate specific functionalities or
//! business logic of a blockchain. They are the building blocks used to construct a blockchain's
//! runtime, allowing for customizable and upgradeable networks. Pallets can be used to extend the
//! capabilities of a Substrate-based blockchain in a composable way.
//!
//! #### Full Node
//!
//! It is a node that prunes historical states, keeping only recent finalized block states to reduce
//! storage needs. It can potentially rebuild all states to become an archive node. Full nodes
//! provide current chain state access and allow direct submission and validation of extrinsics,
//! maintaining network decentralization.
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
//! A validator is a node that participates in the consensus mechanism, validating transactions and
//! blocks, and maintaining the integrity and security of the network.
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
