#!/bin/sh

prompt() {
    while true; do
        echo "$1 [y/N]"
        read yn
        case $yn in
            [Yy]* ) return 0;;  # Yes, return 0 (true)
            [Nn]* ) return 1;;  # No, return 1 (false)
            "" ) return 1;;     # Default to no if user just presses Enter
            * ) echo "Please answer yes or no.";;
        esac
    done
}

cat <<EOF

 Welcome to the

     , __       _   _                          ____  ____  _  __
    /|/  \     | | | |           |            / ___||  _ \| |/ /
     |___/ __  | | | |   __,   __|   __ _|_   \___ \| | | | ' / 
     |    /  \_|/  |/_) /  |  /  |  /  \_|     ___) | |_| | . \ 
     |    \__/ |__/| \_/\_/|_/\_/|_/\__/ |_/  |____/|____/|_|\_\ 
                                                                    quickstart!

🚀 We will be setting up an example template and its environment for you to experiment with.
EOF

# MacOS

os_name=$(uname -s)

if [ "$os_name" = "Darwin" ]; then
    echo "🍎 Detected macOS. Installing dependencies via Homebrew."

    # Check if brew is installed
    if command -v brew >/dev/null 2>&1; then
        echo "\n✅︎ Homebrew already installed."
    else
        if prompt "\n🍺 Homebrew is not installed. Install it?"; then
            echo "🍺 Installing Homebrew."
            /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"
        else
            echo "Aborting."
            exit 1
        fi
    fi

    if prompt "\n⚙️ Install cmake, openssl and protobuf?"; then
        brew update
        brew install cmake openssl protobuf
    else
        echo "⚙️ Assuming cmake, openssl and protobuf are present."
    fi
elif [ "$os_name" = "Linux" ]; then
    echo "Running on Linux"
else
    echo "Unknown operating system. Aborting."
    exit 1
fi

# Check if rustup is installed
if command -v rustc >/dev/null 2>&1; then
    echo "\n✅︎ Rust already installed."
else
    if prompt "\n🦀 Rust is not installed. Install it?"; then
        echo "🦀 Installing via rustup."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    else
        echo "Aborting."
        exit 1
    fi
fi

# Ensure that we have wasm support
if prompt "\n🦀 Setup the Rust environment (e.g. WASM support)?"; then
    echo "🦀 Setting up Rust environment."
    rustup default stable
    rustup update
    rustup target add wasm32-unknown-unknown
    rustup component add rust-src

    rustup update nightly
    rustup target add wasm32-unknown-unknown --toolchain nightly
fi

if [ -d "minimal-template" ]; then
    echo "\n✅︎ minimal-template directory already exists. -> Entering."
else
    echo "\n↓ Let's grab the minimal template from github."
    git clone https://github.com/paritytech/polkadot-sdk-minimal-template.git minimal-template
fi
cd minimal-template

echo "\n⚙️ And finally, let's compile and get the node up and running."
cargo run --release -- --dev