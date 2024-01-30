#!/bin/bash

echo "---------------------------------------------------------------------------------"
echo Adding architectures.
echo "---------------------------------------------------------------------------------"
sudo dpkg --add-architecture arm64
sudo dpkg --add-architecture armhf
echo Setting up apt sources and updating current apt packages.
echo "---------------------------------------------------------------------------------"
echo deb [arch=armhf,arm64] http://ports.ubuntu.com/ jammy main restricted | sudo tee /etc/apt/sources.list.d/arm-cross-compile-sources.list
echo deb [arch=armhf,arm64] http://ports.ubuntu.com/ jammy-updates main restricted | sudo tee -a /etc/apt/sources.list.d/arm-cross-compile-sources.list
echo deb [arch=armhf,arm64] http://ports.ubuntu.com/ jammy universe | sudo tee -a /etc/apt/sources.list.d/arm-cross-compile-sources.list
echo deb [arch=armhf,arm64] http://ports.ubuntu.com/ jammy-updates universe | sudo tee -a /etc/apt/sources.list.d/arm-cross-compile-sources.list
echo deb [arch=armhf,arm64] http://ports.ubuntu.com/ jammy multiverse | sudo tee -a /etc/apt/sources.list.d/arm-cross-compile-sources.list
echo deb [arch=armhf,arm64] http://ports.ubuntu.com/ jammy-updates multiverse | sudo tee -a /etc/apt/sources.list.d/arm-cross-compile-sources.list
echo deb [arch=armhf,arm64] http://ports.ubuntu.com/ jammy-backports main restricted universe multiverse | sudo tee -a /etc/apt/sources.list.d/arm-cross-compile-sources.list
echo deb [arch=amd64] http://archive.ubuntu.com/ubuntu jammy main restricted universe multiverse | sudo tee /etc/apt/sources.list
echo deb [arch=amd64] http://archive.ubuntu.com/ubuntu/ jammy-updates main restricted universe multiverse | sudo tee -a /etc/apt/sources.list
echo deb [arch=amd64] http://security.ubuntu.com/ubuntu/ jammy-security main restricted universe multiverse | sudo tee -a /etc/apt/sources.list
echo deb [arch=amd64] http://archive.ubuntu.com/ubuntu/ jammy-backports main restricted universe multiverse | sudo tee -a /etc/apt/sources.list
sudo apt update
sudo apt upgrade -y
echo "---------------------------------------------------------------------------------"
echo Installing required apt packages.
echo "---------------------------------------------------------------------------------"
sudo apt install gcc pkg-config libasound2-dev libssl-dev git gcc-arm-linux-gnueabihf libasound2-dev:armhf libasound2-dev:arm64 -y
echo "---------------------------------------------------------------------------------"
echo Installing rust.
echo "---------------------------------------------------------------------------------"
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
echo "---------------------------------------------------------------------------------"
echo Adding rust target
rustup target add armv7-unknown-linux-gnueabihf
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
fi;
echo Setting environment variables to cross compile for armv7-unknown-linux-gnueabihf.
echo "---------------------------------------------------------------------------------"
export PKG_CONFIG_LIBDIR=/usr/lib/arm-linux-gnueabihf/pkgconfig
export PKG_CONFIG_ALLOW_CROSS=1
echo Building software for armv7-unknown-linux-gnueabihf.
echo "---------------------------------------------------------------------------------"
cargo build --release --target=armv7-unknown-linux-gnueabihf
if [ $? -eq 0 ]; then
    echo "---------------------------------------------------------------------------------"
    echo Portal software successfully cross compiled for armv7-unknown-linux-gnueabihf.
    echo "--------------------------------------------------------------------------------"
else
    echo "---------------------------------------------------------------------------------"
    echo Unable to cross compile portal software for armv7-unknown-linux-gnueabihf.
    echo "---------------------------------------------------------------------------------"
fi;