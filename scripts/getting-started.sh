#!/usr/bin/env sh

set -e

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

prompt_default_yes() {
    while true; do
        echo "$1 [Y/n]"
        read yn
        case $yn in
            [Yy]* ) return 0;;  # Yes, return 0 (true)
            [Nn]* ) return 1;;  # No, return 1 (false)
            "" ) return 0;;     # Default to yes if user just presses Enter
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

⚡ We will be setting up an example template and its environment for you to experiment with.
EOF

# Determine OS
os_name=$(uname -s)
if [ "$os_name" = "Darwin" ]; then
    echo "🍎 Detected macOS. Installing dependencies via Homebrew."

    # Check if brew is installed
    if command -v brew >/dev/null 2>&1; then
        echo "\n✅︎🍺 Homebrew already installed."
    else
        if prompt_default_yes "\n🍺 Homebrew is not installed. Install it?"; then
            echo "🍺 Installing Homebrew."
            /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"
        else
            echo "❌ Cannot continue without homebrew. Aborting."
            exit 1
        fi
    fi

    brew update
    if command -v git >/dev/null 2>&1; then
        echo "\n✅︎🍺 git already installed."
    else
        if prompt_default_yes "\n🍺 git seems to be missing but we will need it; install git?"; then
            brew install git
        else
            echo "❌ Cannot continue without git. Aborting."
            exit 1
        fi
    fi

    if prompt "\n🍺 Install cmake, openssl and protobuf?"; then
        brew install cmake openssl protobuf
    else
        echo "🍺 Assuming cmake, openssl and protobuf are present."
    fi
elif [ "$os_name" = "Linux" ]; then
    # find the distro name in the release files
    distro=$( cat /etc/*-release | tr '[:upper:]' '[:lower:]' | grep -Poi '(debian|ubuntu|arch|fedora|opensuse)' | uniq | head -n 1 )

    if [ "$distro" = "ubuntu" ]; then
        echo "\n🐧 Detected Ubuntu. Using apt to install dependencies."
        sudo apt install --assume-yes git clang curl libssl-dev protobuf-compiler
    elif [ "$distro" = "debian" ]; then
        echo "\n🐧 Detected Debian. Using apt to install dependencies."
        sudo apt install --assume-yes git clang curl libssl-dev llvm libudev-dev make protobuf-compiler
    elif [ "$distro" = "arch" ]; then
        echo "\n🐧 Detected Arch Linux. Using pacman to install dependencies."
        pacman -Syu --needed --noconfirm curl git clang make protobuf
    elif [ "$distro" = "fedora" ]; then
        echo "\n🐧 Detected Fedora. Using dnf to install dependencies."
        sudo dnf update
        sudo dnf install clang curl git openssl-devel make protobuf-compiler
    elif [ "$distro" = "opensuse" ]; then
        echo "\n🐧 Detected openSUSE. Using zypper to install dependencies."
        sudo zypper install clang curl git openssl-devel llvm-devel libudev-devel make protobuf
    else
        if prompt "\n🐧 Unknown Linux distribution. Unable to install dependencies. Continue anyway?"; then
            echo "\n🐧 Proceeding with unknown linux distribution..."
        else
            exit 1
        fi
    fi
else
    echo "❌ Unknown operating system. Aborting."
    exit 1
fi

# Check if rust is installed
if command -v rustc >/dev/null 2>&1; then
    echo "\n✅︎🦀 Rust already installed."
else
    if prompt_default_yes "\n🦀 Rust is not installed. Install it?"; then
        echo "🦀 Installing via rustup."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    else
        echo "Aborting."
        exit 1
    fi
fi

# Ensure that we have wasm support
if prompt_default_yes "\n🦀 Setup the Rust environment (e.g. WASM support)?"; then
    echo "🦀 Setting up Rust environment."
    rustup default stable
    rustup update
    rustup target add wasm32-unknown-unknown
    rustup component add rust-src
fi

if [ -d "minimal-template" ]; then
    echo "\n✅︎ minimal-template directory already exists. -> Entering."
else
    echo "\n↓ Let's grab the minimal template from github."
    git clone https://github.com/paritytech/polkadot-sdk-minimal-template.git minimal-template
fi
cd minimal-template

echo "\n⚙️ Let's compile the node."
cargo build --release

if prompt_default_yes "\n🚀 Everything ready to go, let's run the node?"; then
    cargo run --release -- --dev
fi
