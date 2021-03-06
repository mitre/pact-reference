#!/bin/bash -xe

cargo clean

mkdir -p ../target/artifacts
cargo build --release
gzip -c ../target/release/pact_mock_server_cli > ../target/artifacts/pact_mock_server_cli-linux-x86_64.gz
