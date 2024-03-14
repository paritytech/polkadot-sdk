//! # XCM Docs
//!
//! Documentation and guides for XCM
//!
//! Welcome to the Cross-Consensus Messaging documentation!
//!
//! XCM is a **language** for communicating **intentions** between **consensus systems**.
//! Whether you're a developer, a blockchain enthusiast, or just interested in Polkadot, this guide
//! aims to provide you with an easy-to-understand and comprehensive introduction to XCM.
//!
//! ## Getting started
//!
//! Head over to the [fundamentals](fundamentals) section.
//! Then, go to the [guides](guides), to learn about how to do things with XCM.
//!
//! ## Cookbook
//!
//! There's also the [cookbook](cookbook) for useful recipes for XCM.
//!
//! ## Glossary
//!
//! There's a [glossary](glossary) with common terms used throughout the docs.
//!
//! ## Contribute
//!
//! To contribute to the format, check out the [RFC process](https://github.com/paritytech/xcm-format/blob/master/proposals/0001-process.md).
//! To contribute to these docs, [make a PR](https://github.com/paritytech/polkadot-sdk/tree/master/polkadot/xcm/docs).
//!
//! ## Why Rust Docs?
//!
//! Rust Docs allow docs to be as close to the source as possible.
//! They're also available offline automatically for anyone who has the `polkadot-sdk` repo locally.
//! Given all the content is here, it's simple to .
//!
//! ## Docs structure
#![doc = simple_mermaid::mermaid!("../mermaid/structure.mmd")]

/// Fundamentals of the XCM language. The virtual machine, instructions, locations and assets.
pub mod fundamentals;

/// Step-by-step guides to set up an XCM environment and start hacking.
pub mod guides;

/// Useful recipes for programs and configurations.
pub mod cookbook;

/// Glossary
pub mod glossary;

/// Mock message queue for some examples
pub mod mock_message_queue;
