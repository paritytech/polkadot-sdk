#!/usr/bin/env sh

set -e

prompt() {
    while true; do
        printf "$1 [y/N]\n"
        read yn
        case $yn in
            [Yy]* ) return 0;;  # Yes, return 0 (true)
            [Nn]* ) return 1;;  # No, return 1 (false)
            "" ) return 1;;     # Default to no if user just presses Enter
            * ) printf "Please answer yes or no.\n";;
        esac
    done
}

prompt_default_yes() {
    while true; do
        printf "$1 [Y/n]\n"
        read yn
        case $yn in
            [Yy]* ) return 0;;  # Yes, return 0 (true)
            [Nn]* ) return 1;;  # No, return 1 (false)
            "" ) return 0;;     # Default to yes if user just presses Enter
            * ) printf "Please answer yes or no.\n";;
        esac
    done
}

clone_and_enter_template() {
    template="$1" # minimal, solochain, or parachain
    if [ -d "${template}-template" ]; then
        printf "\n✅︎ ${template}-template directory already exists. -> Entering.\n"
    else
        printf "\n↓ Let's grab the ${template} template from github.\n"
        git clone --quiet https://github.com/paritytech/polkadot-sdk-${template}-template.git ${template}-template
    fi
    cd ${template}-template
}

cat <<EOF

 Welcome to the

     , __       _   _                          ____  ____  _  __
    /|/  \     | | | |           |            / ___||  _ \| |/ /
     |___/ __  | | | |   __,   __|   __ _|_   \___ \| | | | ' / 
     |    /  \_|/  |/_) /  |  /  |  /  \_|     ___) | |_| | . \ 
     |    \__/ |__/| \_/\_/|_/\_/|_/\__/ |_/  |____/|____/|_|\_\ 
                                                                    quickstart!

⚡ We will help setting up the environment for you to experiment with.
EOF

# Determine OS
os_name=$(uname -s)
if [ "$os_name" = "Darwin" ]; then
    printf "🍎 Detected macOS. Installing dependencies via Homebrew.\n"

    # Check if brew is installed
    if command -v brew >/dev/null 2>&1; then
        printf "\n✅︎🍺 Homebrew already installed.\n"
    else
        if prompt_default_yes "\n🍺 Homebrew is not installed. Install it?\n"; then
            printf "🍺 Installing Homebrew.\n"
            /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"
        else
            printf "❌ Cannot continue without homebrew. Aborting.\n"
            exit 1
        fi
    fi

    brew update
    if command -v git >/dev/null 2>&1; then
        printf "\n✅︎🍺 git already installed.\n"
    else
        if prompt_default_yes "\n🍺 git seems to be missing but we will need it; install git?\n"; then
            brew install git
        else
            printf "❌ Cannot continue without git. Aborting.\n"
            exit 1
        fi
    fi

    if prompt "\n🍺 Install cmake, openssl and protobuf?"; then
        brew install cmake openssl protobuf
    else
        printf "🍺 Assuming cmake, openssl and protobuf are present.\n"
    fi
elif [ "$os_name" = "Linux" ]; then
    # find the distro name in the release files
    distro=$( cat /etc/*-release | tr '[:upper:]' '[:lower:]' | grep -Poi '(debian|ubuntu|arch|fedora|opensuse)' | uniq | head -n 1 )

    if [ "$distro" = "ubuntu" ]; then
        printf "\n🐧 Detected Ubuntu. Using apt to install dependencies.\n"
        sudo apt -qq update
        sudo apt -qq install --assume-yes git clang curl libssl-dev protobuf-compiler make
    elif [ "$distro" = "debian" ]; then
        printf "\n🐧 Detected Debian. Using apt to install dependencies.\n"
        sudo apt -qq update
        sudo apt -qq install --assume-yes git clang curl libssl-dev llvm libudev-dev make protobuf-compiler
    elif [ "$distro" = "arch" ]; then
        printf "\n🐧 Detected Arch Linux. Using pacman to install dependencies.\n"
        pacman -Syu --needed --noconfirm curl git clang make protobuf
    elif [ "$distro" = "fedora" ]; then
        printf "\n🐧 Detected Fedora. Using dnf to install dependencies.\n"
        sudo dnf update --assumeyes
        sudo dnf install --assumeyes clang curl git openssl-devel make protobuf-compiler perl
    elif [ "$distro" = "opensuse" ]; then
        printf "\n🐧 Detected openSUSE. Using zypper to install dependencies.\n"
        sudo zypper install --no-confirm clang gcc gcc-c++ curl git openssl-devel llvm-devel libudev-devel make awk protobuf-devel
    else
        if prompt "\n🐧 Unknown Linux distribution. Unable to install dependencies. Continue anyway?\n"; then
            printf "\n🐧 Proceeding with unknown linux distribution...\n"
        else
            exit 1
        fi
    fi
else
    printf "❌ Unknown operating system. Aborting.\n"
    exit 1
fi

# Check if rust is installed
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
if command -v rustc >/dev/null 2>&1; then
    printf "\n✅︎🦀 Rust already installed.\n"
else
    if prompt_default_yes "\n🦀 Rust is not installed. Install it?"; then
        printf "🦀 Installing via rustup.\n"
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
        . "$HOME/.cargo/env"
    else
        printf "Aborting.\n"
        exit 1
    fi
fi

# Ensure that we have wasm support
if prompt_default_yes "\n🦀 Setup the Rust environment (e.g. WASM support)?"; then
    printf "🦀 Setting up Rust environment.\n"
    rustup default stable
    rustup update
    rustup target add wasm32-unknown-unknown
    rustup component add rust-src
fi

if ! prompt "\nWould you like to start with one of the templates?"; then
    printf "⚡ All done, the environment is ready for hacking.\n"
    exit 0
fi

while true; do
    printf "\nWhich template would you like to start with?\n"
    printf "1) minimal template\n"
    printf "2) parachain template\n"
    printf "3) solochain template\n"
    printf "q) cancel\n"
    read -p "#? " template
    case $template in
        [1]* ) clone_and_enter_template minimal; break;;
        [2]* ) clone_and_enter_template parachain; break;;
        [3]* ) clone_and_enter_template solochain; break;;
        [qQ]* ) printf "Canceling, not using a template.\n"; exit 0;;
        * ) printf "Selection not recognized.\n";;
    esac
done

if ! prompt_default_yes "\n⚙️ Let's compile the node? It might take a while."; then
    printf "⚡ Script finished, you can continue in the ${template}-template directory.\n"
    exit 0
fi

cargo build --release

if prompt_default_yes "\n🚀 Everything ready to go, let's run the node?\n"; then
    cargo run --release -- --dev
fi
