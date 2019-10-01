#!/bin/bash

export LC_ALL=C

# This variable is required by the OpenWrt SDK tools
export STAGING_DIR=/home/abc/openwrt-sdk-18.06.0-ramips-mt7621_gcc-7.3.0_musl.Linux-x86_64/staging_dir

TOOLCHAIN_DIR=$STAGING_DIR/toolchain-mipsel_24kc_gcc-7.3.0_musl
TARGET_DIR=$STAGING_DIR/target-mipsel_24kc_musl

# These two variables are required by the Rust OpenSSL wrapper
export OPENSSL_LIB_DIR=$TARGET_DIR/usr/lib
export OPENSSL_INCLUDE_DIR=$TARGET_DIR/usr/include

# Make sure the toolchain is in PATH
export PATH=$TOOLCHAIN_DIR/bin:$PATH

export CARGO_TARGET_MIPS_UNKNOWN_LINUX_MUSL_LINKER=$TOOLCHAIN_DIR/bin/mipsel-openwrt-linux-gcc
export TARGET_CC=$TOOLCHAIN_DIR/bin/mipsel-openwrt-linux-gcc
export HOST_CC=gcc
export MIPS_UNKNOWN_LINUX_MUSL_OPENSSL_DIR=$TARGET_DIR/usr/
export PKG_CONFIG_ALLOW_CROSS=1

cargo build --target mipsel-unknown-linux-musl --release

