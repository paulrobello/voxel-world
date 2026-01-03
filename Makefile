# Voxel World Makefile

# Vulkan environment variables for macOS
# Keep both the main Homebrew lib dir and the vulkan-loader keg to avoid search misses.
export DYLD_LIBRARY_PATH := /opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib
export DYLD_FALLBACK_LIBRARY_PATH := /opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib
export VK_ICD_FILENAMES := /opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json
export CMAKE_POLICY_VERSION_MINIMUM := 3.5

.PHONY: build build-release build-debug run run-release run-debug profile run-profile clean test check fmt lint checkall sprite-gen

# Default target
all: build-release

# Build targets
build: build-release

build-release:
	cargo build --release

build-debug:
	cargo build

# Run targets (pass CLI args via `make run ARGS="--flag"`).
ARGS ?=

run: run-release

run-release: build-release
	./target/release/voxel_world $(ARGS)

run-debug: build-debug
	@echo "DYLD_LIBRARY_PATH=$(DYLD_LIBRARY_PATH)"
	@echo "DYLD_FALLBACK_LIBRARY_PATH=$(DYLD_FALLBACK_LIBRARY_PATH)"
	@echo "VK_ICD_FILENAMES=$(VK_ICD_FILENAMES)"
	DYLD_LIBRARY_PATH=$(DYLD_LIBRARY_PATH) DYLD_FALLBACK_LIBRARY_PATH=$(DYLD_FALLBACK_LIBRARY_PATH) VK_ICD_FILENAMES=$(VK_ICD_FILENAMES) RUST_BACKTRACE=1 ./target/debug/voxel_world $(ARGS)

# Profiling target (writes profile.csv in cwd)
profile: run-profile

run-profile: build-release
	./target/release/voxel_world --verbose --profile-log profile.csv --debug-interval 120 --view-distance 8 --fly-mode $(ARGS)

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

# Generate palette/hotbar sprites to textures/rendered/ and exit
sprite-gen: build-release
	./target/release/voxel_world --generate-sprites $(ARGS)
