#!/bin/sh

echo "Welcome to the"
echo "\
 , __       _   _                       \
/|/  \     | | | |           |          \
 |___/ __  | | | |   __,   __|   __ _|_ \
 |    /  \_|/  |/_) /  |  /  |  /  \_|  \
 |    \__/ |__/| \_/\_/|_/\_/|_/\__/ |_/\
"
echo "quickstart!"
echo "We will be setting up an example template and its environment for you to experiment with."
# Check if rustup is installed
if command -v rustup >/dev/null 2>&1; then
    echo "✅︎ rustup already installed."
else
    echo "rustup is not installed. -> Installing"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
fi

# should we rustup update?

echo "Let's grab the minimal template from github."
git clone https://github.com/paritytech/polkadot-sdk-minimal-template.git minimal-template
cd minimal-template

echo "And let's compile and get the node up and running."
cargo run --release --dev