# Env setup

Special files for setting up an environment to work with the template:

- `rust-toolchain.toml` when working with `rustup`.
- `flake.nix` when working with `nix`.

These files will be copied by the installer script to the main directory. They are
put into this special directory to not interfere with the normal CI.
