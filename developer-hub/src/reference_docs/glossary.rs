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
//! #### Client
//!
//! The full software artifact that contains the [host](#host), but importantly also all the other
//! modules needed to be part of a blockchain network, such as peer-to-peer networking, database and
//! such.
//!
//! **Synonyms**: Node
//!
//! #### Light Client
//!
//! Same as [client](#client), but when capable of following the network only through listening to
//! block headers. Usually capable of running in more constrained environments, such as an embedded
//! device, phone, or a web browser.
//!
//! #### Offchain
//!
//! #### Host Function:
//!
//! #### Runtime API:
//!
//! #### Dispatchable:
//!
//! Callable
//!
//! #### Extrinsic
//!
//!
//! #### Pallet
