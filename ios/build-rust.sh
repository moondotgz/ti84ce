#!/usr/bin/env bash
# Build the Rust core (crate ti84-ffi) for iOS and package it as
# ios/Ti84Core.xcframework. Requires macOS with Xcode and these rustup targets:
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
set -euo pipefail
cd "$(dirname "$0")/.."

export IPHONEOS_DEPLOYMENT_TARGET=17.0
for target in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  cargo build -p ti84-ffi --release --target "$target"
done

# One fat library for the simulator (Apple silicon + Intel Macs).
mkdir -p target/ios-sim
lipo -create \
  target/aarch64-apple-ios-sim/release/libti84.a \
  target/x86_64-apple-ios/release/libti84.a \
  -output target/ios-sim/libti84.a

rm -rf ios/Ti84Core.xcframework
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libti84.a -headers ffi/include \
  -library target/ios-sim/libti84.a -headers ffi/include \
  -output ios/Ti84Core.xcframework

echo "Built ios/Ti84Core.xcframework"
