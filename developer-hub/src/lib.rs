//! # Developer Hub
//!
//! The Polkadot SDK Developer Hub.
//!
//! This crate is a *minimal*, but *always-accurate* source of information for those wishing to
//! build on the Polkadot SDK.
//!
//! > **Work in Progress**: This crate is under heavy development. Expect content to be moved and
//! > changed. Do not use links to this crate yet. See [`meta_contributing`] for more information.
//!
//! ## Getting Started
//!
//! We suggest the following reading sequence:
//!
//! - Start by learning about the structure of the [`polkadot_sdk`] and its context.
//! - Then, head over the [`tutorial`] to get more hand-on practice.
//! - Whilst reading the tutorial, you might find back-links to [`reference_docs`].
//! - Finally, <https://paritytech.github.io> is the parent website of this crate that contains the
//!   list of further tools related to the Polkadot SDK.
//!
//! ## Information Architecture
//!
//! This section paints a picture over the high-level information architecture of this crate.
#![doc = simple_mermaid::mermaid!("../../docs/mermaid/IA.mmd")]
#![allow(rustdoc::invalid_html_tags)] // TODO: remove later.
#![allow(rustdoc::bare_urls)] // TODO: remove later.
#![warn(rustdoc::broken_intra_doc_links)]
#![warn(rustdoc::private_intra_doc_links)]

/// Meta information about this crate, how it is built, what principles dictates its evolution and
/// how one can contribute to it.
pub mod meta_contributing;

/// An introduction to the Polkadot SDK. Read this module to learn about the structure of the SDK,
/// the tools that are provided as a part of it, and to gain a high level understanding of each.
pub mod polkadot_sdk;
/// Reference documents covering in-depth topics across the Polkadot SDK. It is suggested to read
/// these on-demand, while you are going through the [`tutorial`] or other content.
pub mod reference_docs;
/// The main polkadot-sdk tutorial, targeted toward those who wish to build parachains FRAME and
/// Cumulus.
pub mod tutorial;
