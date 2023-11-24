# Processes

The following processes are necessary to actualize our releases. Each process has a *Cadence* on which it must execute and an *Responsible* that is responsible for autonomously doing so and reporting back any error in the RelEng<sup>1</sup> channel.

## Mainline Release

Cadence: every two weeks. Responsible: Release Team.

This process aims to release the `stable` branch as a *Mainline* release every two weeks. It needs to start three days before the proposed release deadline to allow leeway for unexpected issues. This process should eventually be automated.

### Steps

1. [ ] Check if process [Clobbering](#clobbering) needs to happen and do so first, if that is the case.
1. [ ] Check out the latest commit of `stable`.
2. [ ] Verify all CI checks of that commit.
3. [ ] Announce that commit as cutoff *Mainline* for a release in the General<sup>2</sup> chat.
4. [ ] Bump the semver of all crates <!-- FAIL-CI: We need some better process here on how to do it exactly -->
5. [ ] Abort the release process and announce so in General if there are no bumps needed.
6. [ ] Create a merge request to `stable` with the proposed SemVer bumps.
7. [ ] Announce this merge request in the *General* channel to quickly gather reviews.
8. [ ] Merge it into `stable`.
9. [ ] Verify all CI checks.
10. [ ] Announce the intent to do a *Mainline* release from the resulting commit hash in RelEng.
11. [ ] <!-- The release team has internal checklists for QA i think, should we mention this? -->
12. [ ] Release all crates to crates.io using [parity-publish](https://github.com/paritytech/parity-publish).

## Nightly Release

Cadence: every day at 00:00 UTC+1. Responsible: Release Team

This process aims to release the `unstable` branch as a *Nightly* release every day. The process can start at 00:00 UTC+1 and should automatically do the following steps.

1. [ ] Check out the latest commit of branch `unstable`.
3. [ ] Compare this commit to the latest `nightly*` tag. Announce that the process was aborted in the RelEng chat since there were no changes.
4. [ ] Verify all CI checks of that commit.
5. [ ] Set the version of all crate to `major.0.0-nightlyYYMMDD` where `major` is the last released `major` version of that crate plus one.
6. [ ] Tag this commit as `nightlyYYMMDD`.
7. [ ] Announce the intent to do a *Nightly* release from that tag in the RelEng chat.
8. [ ] Release all crates to crates.io using [parity-publish](https://github.com/paritytech/parity-publish). <!-- FAIL-CI: I think Morgan fixed that tool so it would only release crates that had changes, or that had one of their transitive dependencies changes. That would help, since otherwise we always push 400 crates or so. -->

## Clobbering

Cadence: every 6th release (~3 months). Responsible: Release Team

This process aims to bring branch `stable` in sync with the latest audited commit of `unstable`. It is not done via a Merge Request but rather by just copying files. It should be automated.

The following script is provided to do the clobbering.

```bash
git checkout polkadot-sdk -b audited
mv polkadot-sdk polkadot-sdk-audited
git checkout polkadot-sdk -b stable
cp -r polkadot-sdk-audited/* polkadot-sdk
cd polkadot-sdk
git commit -a -m "Clobber with latest audited"
git push
```

# Footnotes

1: `RelEng`: The *RelEng: Polkadot Release Coordination* Matrix channel.  
2: `General`: The *General* Matrix channel.
