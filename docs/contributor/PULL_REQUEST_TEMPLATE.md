

✄ -----------------------------------------------------------------------------

Thank you for your Pull Request! 🙏 Please make sure it follows the contribution guidelines outlined in [this
document](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md) and fill out the
sections below. Once you're ready to submit your PR for review, please delete this section and leave only the text under
the "Description" heading.

# Description

*A concise description of what your PR is doing, and what potential issue it is solving. Use [Github semantic
linking](https://docs.github.com/en/issues/tracking-your-work-with-issues/linking-a-pull-request-to-an-issue#linking-a-pull-request-to-an-issue-using-a-keyword)
to link the PR to an issue that must be closed once this is merged.*

## Integration

*In depth notes about how this PR should be integrated by downstream projects. This part is
mandatory, and should be reviewed by reviewers, if the PR does NOT have the
`R0-no-crate-publish-required` label. In case of a `R0-no-crate-publish-required`, it can be
ignored.*

## Review Notes

*In depth notes about the **implementation** details of your PR. This should be the main guide for reviewers to
understand your approach and effectively review it. If too long, use
[`<details>`](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/details)*.

*Imagine that someone who is depending on the old code wants to integrate your new code and the only information that
they get is this section. It helps to include example usage and default value here, with a `diff` code-block to show
possibly integration.*

*Include your leftover TODOs, if any, here.*

# Checklist

* [ ] My PR includes a detailed description as outlined in the "Description" and its two subsections above.
* [ ] My PR follows the [labeling requirements](
https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md#Process
) of this project (at minimum one label for `T` required)
    * External contributors: Use `/cmd label <label-name>` to add labels
    * Maintainers can also add labels manually
* [ ] I have made corresponding changes to the documentation (if applicable)
* [ ] I have added tests that prove my fix is effective or that my feature works (if applicable)

## Bot Commands

You can use the following bot commands in comments to help manage your PR:

**Labeling (Self-service for contributors):**
* `/cmd label T1-FRAME` - Add a single label
* `/cmd label T1-FRAME R0-no-crate-publish-required` - Add multiple labels
* `/cmd label T6-XCM D2-substantial I5-enhancement` - Add multiple labels at once
* See [label documentation](https://paritytech.github.io/labels/doc_polkadot-sdk.html) for all available labels

**Other useful commands:**
* `/cmd fmt` - Format code (cargo +nightly fmt and taplo)
* `/cmd prdoc` - Generate PR documentation
* `/cmd bench` - Run benchmarks
* `/cmd update-ui` - Update UI tests
* `/cmd --help` - Show help for all available commands

You can remove the "Checklist" section once all have been checked. Thank you for your contribution!

✄ -----------------------------------------------------------------------------
