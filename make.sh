#!/usr/bin/env bash

nix-shell -p rustup --run "rustup component add llvm-tools && cargo install cargo-binutils"
nix-shell -p rustup --run "cargo build --release"
nix-shell -p rustup --run "cargo objcopy --release -- -O binary target/thumbv6m-none-eabi/release/bfk6-fw.bin"
./tools/flash-rs/target/release/flash target/thumbv6m-none-eabi/release/bfk6-fw.bin /dev/ttyUSB0 

