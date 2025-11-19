#!/bin/bash
set -e

# Build release version
cargo build --release

# Ensure output directory is clean
rm -rf output-10m
mkdir -p output-10m

# Run converter
time cargo run --release --bin xml2json -- --source ./bench-10m --destination ./output-10m



