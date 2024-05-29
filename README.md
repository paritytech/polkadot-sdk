
<div align="center">

![SDK Logo](./docs/images/Polkadot_Logo_Horizontal_Pink_White.png#gh-dark-mode-only)
![SDK Logo](./docs/images/Polkadot_Logo_Horizontal_Pink_Black.png#gh-light-mode-only)

# Polkadot SDK

![GitHub stars](https://img.shields.io/github/stars/paritytech/polkadot-sdk)&nbsp;&nbsp;![GitHub
forks](https://img.shields.io/github/forks/paritytech/polkadot-sdk)

<!-- markdownlint-disable-next-line MD013 -->
[![StackExchange](https://img.shields.io/badge/StackExchange-Community%20&%20Support-222222?logo=stackexchange)](https://substrate.stackexchange.com/)&nbsp;&nbsp;![GitHub contributors](https://img.shields.io/github/contributors/paritytech/polkadot-sdk)&nbsp;&nbsp;![GitHub commit activity](https://img.shields.io/github/commit-activity/m/paritytech/polkadot-sdk)

![GitHub lines of code](https://tokei.rs/b1/github/paritytech/polkadot-sdk)&nbsp;&nbsp;
![GitHub last commit](https://img.shields.io/github/last-commit/paritytech/polkadot-sdk)

> The Polkadot SDK repository provides all the components needed to start building on the
> [Polkadot](https://polkadot.network) network, a multi-chain blockchain platform that enables
> different blockchains to interoperate and share information in a secure and scalable way.

</div>

## 📚 Documentation

* [🦀 rust-docs]([paritytech.github.io/](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/index.html))
 * [Introduction](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/polkadot_sdk/index.html)
   to each component of the Polkadot SDK: Substrate, FRAME, Cumulus, and XCM
* Other Resources:
  * [Polkadot Wiki -> Build](https://wiki.polkadot.network/docs/build-guide)

## 🚀 Releases

> [!NOTE]
> Our release process is still Work-In-Progress and may not yet reflect the aspired outline
> here.

The Polkadot-SDK has two release channels: `stable` and `nightly`. Production software is advised to
only use `stable`. `nightly` is meant for tinkerers to try out the latest features. The detailed
release process is described in [RELEASE.md](docs/RELEASE.md).

### 😌 Stable

`stable` releases have a support duration of **three months**. In this period, the release will not
have any breaking changes. It will receive bug fixes, security fixes, performance fixes and new
non-breaking features on a **two week** cadence.

### 🤠 Nightly

`nightly` releases are released every night from the `master` branch, potentially with breaking
changes. They have pre-release version numbers in the format `major.0.0-nightlyYYMMDD`.

## 🔐 Security

The security policy and procedures can be found in
[docs/contributor/SECURITY.md](./docs/contributor/SECURITY.md).

## 🤍 Contributing & Code of Conduct

Ensure you follow our [contribution guidelines](./docs/contributor/CONTRIBUTING.md). In every
interaction and contribution, this project adheres to the [Contributor Covenant Code of
Conduct](./docs/contributor/CODE_OF_CONDUCT.md).

### 👾 Ready to Contribute?

Take a look at the issues labeled with [`mentor`](https://github.com/paritytech/polkadot-sdk/labels/C1-mentor)
(or alternatively [this](https://mentor.tasty.limo/) page, created by one of the maintainers) label to get started!
We always recognize valuable contributions by proposing an on-chain tip to the Polkadot network as a token of our
appreciation.

## Polkadot Fellowship

Development in this repo usually goes hand in hand with the `fellowship` organization. In short,
this repository provides all the SDK pieces needed to build both Polkadot and its parachains. But,
the actual Polkadot runtime lives in the `fellowship/runtimes` repository. Read more about the
fellowship, this separation, the RFC process
[here](https://polkadot-fellows.github.io/dashboard/).

## History

This repository is the amalgamation of 3 separate repositories that used to make up Polkadot SDK,
namely Substrate, Polkadot and Cumulus. Read more about the merge and its history
[here](https://polkadot-public.notion.site/Polkadot-SDK-FAQ-fbc4cecc2c46443fb37b9eeec2f0d85f).
