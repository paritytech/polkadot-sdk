# Release Checklist

These steps assume that you've checked out the Litep2p repository and are in the root directory of it.

We also assume that ongoing work done is being merged directly to the `master` branch.

1. Ensure that everything you'd like to see released is on the `master` branch.

2. Create a release branch off `master`, for example `release-v0.3.0`. Decide how far the version needs to be bumped based
   on the changes to date. If unsure what to bump the version to (e.g. is it a major, minor or patch release), check with the
   Parity Networking team.

3. Bump the crate version in the root `Cargo.toml` to whatever was decided in step 2 (basically a find and replace from old version
   to new version in this file should do the trick).

4. Ensure the `Cargo.lock` file is up to date.

    ```bash
    cargo generate-lockfile
    ```

5. Update `CHANGELOG.md` to reflect the difference between this release and the last. If you're unsure of
   what to add, check with the Networking team. See the `CHANGELOG.md` file for details of the format it follows.

   First, if there have been any significant changes, add a description of those changes to the top of the
   changelog entry for this release.

   Next, mention any merged PRs between releases.

6. Commit any of the above changes to the release branch and open a PR in GitHub with a base of `master`.

7. Once the branch has been reviewed and passes CI, merge it.

8. Now, we're ready to publish the release to crates.io.

    1. Checkout `master`, ensuring we're looking at that latest merge (`git pull`).

        ```bash
        git checkout master && git pull
        ```

    2. Perform a final sanity check that everything looks ok.

        ```bash
        cargo test --all-targets --all-features
        ```

    3. Run the following command to publish the crate on crates.io:

        ```bash
        cargo publish
        ```

9. If the release was successful, tag the commit that we released in the `master` branch with the
   version that we just released, for example:

    ```bash
    git tag -s v0.3.0 # use the version number you've just published to crates.io, not this one
    git push --tags
    ```

   Once this is pushed, go along to [the releases page on GitHub](https://github.com/paritytech/litep2p/releases)
   and draft a new release which points to the tag you just pushed to `master` above. Copy the changelog comments
   for the current release into the release description.
