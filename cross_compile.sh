#!/bin/bash

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