//! # Templates
//!
//! - classic [`substrate-node-template`](https://github.com/substrate-developer-hub/substrate-node-template)
//! - classic [cumulus-parachain-template](https://github.com/substrate-developer-hub/substrate-parachain-template)
//! - [`extended-parachain-template`](https://github.com/paritytech/extended-parachain-template)
//! - [`frontier-parachain-template`](https://github.com/paritytech/frontier-parachain-template)

// NOTE: a super important detail that I am looking forward to here is
// <https://github.com/paritytech/polkadot-sdk/issues/62#issuecomment-1691523754> and
// <https://github.com/paritytech/polkadot-sdk/issues/5>. Meaning that I would not spend time on
// teaching someone too much detail about the ugly thing we call "node" nowadays. In the future, I
// am sure we will either have a better "node-builder" code that can actually be tested, or an
// "omni-node" that can run (almost) any wasm file. We should already build tutorials in this
// direction IMO. This also affects all the templates. If we have a good neat runtime file, which we
// are moving toward, and a good node-builder, we don't need all of these damn templates. These
// templates are only there because the boilerplate is super horrible atm.
