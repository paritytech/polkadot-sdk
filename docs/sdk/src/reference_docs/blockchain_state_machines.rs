//! # State Transition Function
//!
//! This document briefly explains how in the context of Substrate-based blockchains, we view the
//! blockchain as a **decentralized state transition function**.
//!
//! Recall that a blockchain's main purpose is to help a permissionless set of entities to agree on
//! a shared data-set, and how it evolves. This is called the **State**, also referred to as
//! "onchain" data, or *Storage* in the context of FRAME. The state is where the account balance of
//! each user is, for example, stored, and there is a canonical version of it that everyone agrees
//! upon.
//!
//! Then, recall that a typical blockchain system will alter its state through execution of blocks.
//! *The component that dictates how this state alteration can happen is called the state transition
//! function*.
#![doc = simple_mermaid::mermaid!("../../../mermaid/stf_simple.mmd")]
//!
//! In Substrate-based blockchains, the state transition function is called the *Runtime*. This is
//! explained further in [`crate::reference_docs::wasm_meta_protocol`].
//!
//! With this in mind, we can paint a complete picture of a blockchain as a state machine:
#![doc = simple_mermaid::mermaid!("../../../mermaid/stf.mmd")]
//!
//! In essence, the state of the blockchain at block N is the outcome of applying the state
//! transition function to the the previous state, and the current block as input. This can be
//! mathematically represented as:
//!
//! ```math
//! STF = F(State_N, Block_N) -> State_{N+1}
//! ```
