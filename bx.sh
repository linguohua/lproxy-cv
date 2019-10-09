#!/bin/bash

export LC_ALL=C

# This variable is required by the OpenWrt SDK tools
export STAGING_DIR=/home/abc/openwrt-sdk-18.06.3-x86-64_gcc-7.3.0_musl.Linux-x86_64/staging_dir

TOOLCHAIN_DIR=$STAGING_DIR/toolchain-x86_64_gcc-7.3.0_musl
TARGET_DIR=$STAGING_DIR/target-x86_64_musl

# These two variables are required by the Rust OpenSSL wrapper
export OPENSSL_LIB_DIR=$TARGET_DIR/usr/lib
export OPENSSL_INCLUDE_DIR=$TARGET_DIR/usr/include

# Make sure the toolchain is in PATH
export PATH=$TOOLCHAIN_DIR/bin:$PATH

export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=$TOOLCHAIN_DIR/bin/x86_64-openwrt-linux-gcc
export TARGET_CC=$TOOLCHAIN_DIR/bin/x86_64-openwrt-linux-gcc
export HOST_CC=gcc
export X86_64_UNKNOWN_LINUX_MUSL_OPENSSL_DIR=$TARGET_DIR/usr/
export PKG_CONFIG_ALLOW_CROSS=1

#cargo build --target x86_64-unknown-linux-musl --release

RUSTFLAGS='-C link-arg=-s' cargo build --target x86_64-unknown-linux-musl --release

