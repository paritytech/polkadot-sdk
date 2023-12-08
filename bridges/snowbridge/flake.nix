{
    description = "Snowbridge flake";

    inputs = {
        nixpkgs.url = "nixpkgs/nixos-unstable";
        flake-utils.url  = "github:numtide/flake-utils";
        foundry.url = "github:shazow/foundry.nix/monthly";
    };

    outputs = { self, nixpkgs, flake-utils, foundry }:

    let
        supportedSystems = [ "aarch64-darwin" "x86_64-darwin" "x86_64-linux" ];
        overlays = [ foundry.overlay ];
    in

    flake-utils.lib.eachSystem supportedSystems (system:
        let
            pkgs = import nixpkgs { inherit system overlays; };
            cwd = builtins.toString ./.;
        in

        with pkgs;
        {
            devShells.default = pkgs.mkShell {
                buildInputs = [
                    cacert
                    curl
                    direnv
                    git
                    jq
                    moreutils
                    typos
                    ripgrep
                    tree
                    # ps for zombienet, required in pure shells on Linux
                    ps

                    # typescript
                    nodePackages.pnpm
                    nodejs-18_x

                    # ethereum
                    foundry-bin
                    go-ethereum
                    # gnupg for forge install
                    gnupg

                    # relayer
                    go
                    gotools
                    gopls
                    go-outline
                    gocode
                    gopkgs
                    gocode-gomod
                    godef
                    golint
                    mage
                    revive
                    delve

                    # parachain
                    clang
                    gcc
                    libiconv
                    protobuf
                    # NOTE: when upgrading rustup, check for a command to install the version in the toolchain file:
                    # https://github.com/rust-lang/rustup/issues/2686
                    rustup

                    cowsay
                ];

                shellHook = ''
                    # set HOME for direnv:
                    # direnv needs config, cache & data dirs (DIRENV_CONFIG, XDG_CACHE_HOME & XDG_DATA_HOME
                    # respectively) that can be automatically set when HOME is available
                    export HOME=~

                    export GOCACHE=$PWD/gocache
                    export GOPATH=$PWD/go
                    export PATH=$GOPATH/bin:$PATH

                    export CARGO_HOME=$PWD/.cargo
                    export RUSTUP_HOME=$PWD/.rustup
                    export RUST_NIGHTLY_VERSION=nightly-2023-05-23
                    export PATH=$CARGO_HOME/bin:$PATH

                    eval "$(direnv hook bash)"

                    # LIBCLANG_PATH points rocksdb to a clang.so on Linux
                    export LIBCLANG_PATH="$(readlink -f ${pkgs.clang}/resource-root/include | xargs dirname | xargs dirname | xargs dirname)"

                    cowsay "Development Environment Ready"
                '';
            };
        }
    );

    nixConfig = {};
}
