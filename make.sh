#! /usr/bin/env bash
set -euxo pipefail
cd "$(dirname $0)"
mkdir -p data/images
rm -r data/images
cargo run --release -- $@
mkdir -p ddk/OtherResources/images
rm -r ddk/OtherResources/images
cp -r data/images ddk/OtherResources/images
cd ddk
make
