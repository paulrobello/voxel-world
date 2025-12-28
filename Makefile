# Voxel Ray Traversal Makefile

# Vulkan environment variables for macOS
export DYLD_LIBRARY_PATH := /opt/homebrew/lib
export VK_ICD_FILENAMES := /opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json

.PHONY: build build-release build-debug run run-release run-debug clean test check fmt lint checkall

# Default target
all: build-release

# Build targets
build: build-release

build-release:
	cargo build --release

build-debug:
	cargo build

# Run targets
run: run-release

run-release: build-release
	./target/release/voxel_ray_traversal

run-debug: build-debug
	RUST_BACKTRACE=1 ./target/debug/voxel_ray_traversal

# Development targets
clean:
	cargo clean

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

check:
	cargo fmt --check
	cargo clippy -- -D warnings

checkall: fmt lint test
	@echo "All checks passed!"
