#!/usr/bin/env python3

"""
A script to generate READMEs for all public crates,
if they do not already have one.

It relies on functions from the `check-workspace.py` script.

The resulting README is based on a template defined below,
and includes the crate name, description, license,
and optionally - the SDK release version.

# Example

```sh
python3 -m pip install toml
.github/scripts/generate-readmes.py . --sdk-version 1.15.0
```
"""

import os
import toml
import importlib
import argparse

check_workspace = importlib.import_module("check-workspace")

README_TEMPLATE = """<div align="center">

<img src="https://raw.githubusercontent.com/paritytech/polkadot-sdk/master/docs/images/Polkadot_Logo_Horizontal_Pink_BlackOnWhite.png" alt="Polkadot logo" width="200">

# {name}

This crate is part of the [Polkadot SDK](https://github.com/paritytech/polkadot-sdk/).

</div>

## Description

{description}

## Additional Resources

In order to learn about Polkadot SDK, head over to the [Polkadot SDK Developer Documentation](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/index.html).

To learn about Polkadot, visit [polkadot.com](https://polkadot.com/).

## License

This crate is licensed with {license}.
"""

VERSION_TEMPLATE = """
## Version

This version of `{name}` is associated with Polkadot {sdk_version} release.
"""


def generate_readme(member, *, workspace_dir, workspace_license, sdk_version):
    print(f"Loading manifest for: {member}")
    manifest = toml.load(os.path.join(workspace_dir, member, "Cargo.toml"))
    if manifest["package"].get("publish", True) == False:
        print(f"⏩ Skipping un-published crate: {member}")
        return
    if os.path.exists(os.path.join(workspace_dir, member, "README.md")):
        print(f"⏩ Skipping crate with an existing readme: {member}")
        return
    print(f"📝 Generating README for: {member}")

    license = manifest["package"]["license"]
    if isinstance(license, dict):
        if not license.get("workspace", False):
            print(
                f"❌ License for {member} is unexpectedly declared as workspace=false."
            )
            # Skipping this crate as it is not clear what license it should use.
            return
        license = workspace_license

    name = manifest["package"]["name"]
    description = manifest["package"]["description"]
    description = description + "." if not description.endswith(".") else description

    filled_readme = README_TEMPLATE.format(
        name=name, description=description, license=license
    )

    if sdk_version:
        filled_readme += VERSION_TEMPLATE.format(name=name, sdk_version=sdk_version)

    with open(os.path.join(workspace_dir, member, "README.md"), "w") as new_readme:
        new_readme.write(filled_readme)


def parse_args():
    parser = argparse.ArgumentParser(
        description="Generate readmes for published crates."
    )

    parser.add_argument(
        "workspace_dir",
        help="The directory to check",
        metavar="workspace_dir",
        type=str,
        nargs=1,
    )
    parser.add_argument(
        "--sdk-version",
        help="Optional SDK release version",
        metavar="sdk_version",
        type=str,
        nargs=1,
        required=False,
    )

    args = parser.parse_args()
    return (args.workspace_dir[0], args.sdk_version[0] if args.sdk_version else None)


def main():
    (workspace_dir, sdk_version) = parse_args()
    root_manifest = toml.load(os.path.join(workspace_dir, "Cargo.toml"))
    workspace_license = root_manifest["workspace"]["package"]["license"]
    members = check_workspace.get_members(workspace_dir, [])
    for member in members:
        generate_readme(
            member,
            workspace_dir=workspace_dir,
            workspace_license=workspace_license,
            sdk_version=sdk_version,
        )


if __name__ == "__main__":
    main()
