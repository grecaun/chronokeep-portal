#!/bin/bash

readonly TARGET_ARCH=armv7-unknown-linux-gnueabihf

cargo build --release --target=${TARGET_ARCH} --features static_ssl