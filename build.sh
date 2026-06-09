#!/bin/bash
set -e

echo "Building shlack..."
echo "===================================="

# Build in release mode for maximum performance
cargo build --release

echo ""
echo "Build complete!"
echo "Run with: ./target/release/shlack"
