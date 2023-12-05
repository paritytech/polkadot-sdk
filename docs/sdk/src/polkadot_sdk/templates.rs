//! # Templates
//!
//! ### Internal
//!
//! The following templates are maintained as a part of the `polkadot-sdk` repository:
//!
//! - classic [`substrate-node-template`]: is a white-labeled substrate-based blockchain with a
//!   moderate amount of features. It can act as a great starting point for those who want to learn
//!   Substrate/FRAME and want to have a template that is already doing something.
//! - [`substrate-minimal-template`]: Same as the above, but it contains the least amount of code in
//!   both the node and runtime. It is a great starting point for those who want to deeply learn
//!   Substrate and FRAME.
//! - classic [`cumulus-parachain-template`], which is the de-facto parachain template shipped with
//!   Cumulus. It is the parachain-enabled version of [`substrate-node-template`].
//!
//! ### External Templates
//!
//! Noteworthy templates outside of this repository.
//!
//! - [`extended-parachain-template`](https://github.com/paritytech/extended-parachain-template): A
//!   parachain template that contains more built-in functionality such as assets and NFTs.
//! - [`frontier-parachain-template`](https://github.com/paritytech/frontier-parachain-template): A
//!   parachain template for launching EVM-compatible parachains.
//!
//! [`substrate-node-template`]: https://github.com/paritytech/polkadot-sdk/blob/master/substrate/bin/node-template/
//! [`substrate-minimal-template`]: https://github.com/paritytech/polkadot-sdk/blob/master/substrate/bin/minimal/
//! [`cumulus-parachain-template`]: https://github.com/paritytech/polkadot-sdk/blob/master/cumulus/parachain-template/

// TODO: in general, we need to make a deliberate choice here of moving a few key templates to this
// repo (nothing stays in `substrate-developer-hub`) and the everything else should be community
// maintained. https://github.com/paritytech/polkadot-sdk-docs/issues/67

// TODO: we should rename `substrate-node-template` to `substrate-basic-template`,
// `substrate-blockchain-template`. `node` is confusing in the name.
// `substrate-blockchain-template` and `cumulus-parachain-template` go well together 🤝. https://github.com/paritytech/polkadot-sdk-docs/issues/67

// NOTE: a super important detail that I am looking forward to here is
// <https://github.com/paritytech/polkadot-sdk/issues/62#issuecomment-1691523754> and
// <https://github.com/paritytech/polkadot-sdk/issues/5>. Meaning that I would not spend time on
// teaching someone too much detail about the ugly thing we call "node" nowadays. In the future, I
// am sure we will either have a better "node-builder" code that can actually be tested, or an
// "omni-node" that can run (almost) any wasm file. We should already build tutorials in this
// direction IMO. This also affects all the templates. If we have a good neat runtime file, which we
// are moving toward, and a good node-builder, we don't need all of these damn templates. These
// templates are only there because the boilerplate is super horrible atm.
