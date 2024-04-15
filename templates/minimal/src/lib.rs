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
//! ### Pallet
//!
//! A minimal [`pallet`](`pallet_minimal_template`) that is built using FRAME. It is a unit of
//! encapsulated logic that has a clearly defined responsibility and can be linked to other pallets.
