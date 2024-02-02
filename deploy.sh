#!/bin/bash

echo "---------------------------------------------------------------------------------"
echo Updating current apt packages.
echo "---------------------------------------------------------------------------------"
sudo apt update
sudo apt upgrade -y
echo "---------------------------------------------------------------------------------"
echo Installing required apt packages.
echo "---------------------------------------------------------------------------------"
sudo apt install gcc pkg-config libasound2-dev libssl-dev git -y
echo "---------------------------------------------------------------------------------"
echo Installing rust.
echo "---------------------------------------------------------------------------------"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
echo "---------------------------------------------------------------------------------"
echo Getting release version.
RELEASE_VERSION=$(git describe --tag | sed s/v// | sed -E s/[\.]/\\\\\./g)
echo "---------------------------------------------------------------------------------"
echo Updating version in toml file.
sed -i -E '0,/^version.*$/{s/^version.*$/version = "${RELEASE_VERSION}"/}' Cargo.toml
echo "---------------------------------------------------------------------------------"
echo Building portal software for host architecture.
echo "---------------------------------------------------------------------------------"
cargo build --release
if [ $? -eq 0 ]; then
    echo "---------------------------------------------------------------------------------"
    echo Portal software successfully compiled for host architecture.
    echo "---------------------------------------------------------------------------------"
else
    echo "---------------------------------------------------------------------------------"
    echo Unable to compile portal software for host architecture.
    echo "---------------------------------------------------------------------------------"
    exit 1
fi;