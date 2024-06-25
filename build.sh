#!/bin/bash

# Check if the name argument is provided
if [ -z "$1" ]; then
	echo "Usage: $0 <name>"
	exit 1
fi

NAME=$1

# Build for aarch64-apple-darwin
cargo build --release
if [ $? -ne 0 ]; then
	echo "Failed to build for aarch64-apple-darwin"
	exit 1
fi
mv target/release/${NAME} target/release/${NAME}-aarch64-apple-darwin

# Build for x86_64-apple-darwin
cargo build --target x86_64-apple-darwin --release
if [ $? -ne 0 ]; then
	echo "Failed to build for x86_64-apple-darwin"
	exit 1
fi
mv target/x86_64-apple-darwin/release/${NAME} target/x86_64-apple-darwin/release/${NAME}-x86_64-apple-darwin

# Build for x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu --release
if [ $? -ne 0 ]; then
	echo "Failed to build for x86_64-pc-windows-gnu"
	exit 1
fi
mv target/x86_64-pc-windows-gnu/release/${NAME}.exe target/x86_64-pc-windows-gnu/release/${NAME}-x86_64-pc-windows-gnu.exe

echo "Builds completed successfully."
