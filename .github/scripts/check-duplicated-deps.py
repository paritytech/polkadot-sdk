#!/usr/bin/env python3

"""
A script to check for duplicated dependencies across [dependencies] and [dev-dependencies]
in all Cargo.toml files in the workspace.

This is useful for CI to enforce clean separation between runtime and dev-only dependencies.

A duplicated dependency is one that appears in both sections with the exact same configuration,
which is usually unnecessary and should be avoided.

# Example

```sh
python3 -m pip install toml
.github/scripts/check-duplicated-deps.py
"""

#!/usr/bin/env python3
import os
import sys

import toml


def find_cargo_toml_files(root='.'):
    cargo_files = []
    for dirpath, dirnames, filenames in os.walk(root):
        if 'target' in dirnames:
            dirnames.remove('target')
        if 'Cargo.toml' in filenames:
            cargo_files.append(os.path.join(dirpath, 'Cargo.toml'))
    return cargo_files

def parse_dependencies(file_path):
    try:
        data = toml.load(file_path)
    except Exception as e:
        print(f"Error parsing {file_path}: {e}", file=sys.stderr)
        return {}, {}

    deps = data.get("dependencies", {})
    dev_deps = data.get("dev-dependencies", {})
    return deps, dev_deps

def format_dep_config(config):
    if isinstance(config, str):
        return {"version": config}
    elif isinstance(config, dict):
        return dict(sorted(config.items()))
    else:
        return {}

def main():
    files = find_cargo_toml_files()
    any_duplicates = False

    for file_path in files:
        deps, dev_deps = parse_dependencies(file_path)

        duplicates = []
        for dep_name, dep_config in deps.items():
            if dep_name in dev_deps:
                config1 = format_dep_config(dep_config)
                config2 = format_dep_config(dev_deps[dep_name])
                if config1 == config2:
                    duplicates.append(dep_name)

        if duplicates:
            any_duplicates = True
            print(f"❌ Duplicated dependencies in {file_path}:")
            for dep in sorted(duplicates):
                print(f"   - {dep}")

    if any_duplicates:
        print("\n🚫 CI check failed due to duplicated dependencies.")
        sys.exit(1)
    else:
        print("✅ No duplicated dependencies found.")

if __name__ == "__main__":
    main()
