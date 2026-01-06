# Voxel World Makefile

# Vulkan environment variables for macOS
# Keep both the main Homebrew lib dir and the vulkan-loader keg to avoid search misses.
export DYLD_LIBRARY_PATH := /opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib
export DYLD_FALLBACK_LIBRARY_PATH := /opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib
export VK_ICD_FILENAMES := /opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json
export CMAKE_POLICY_VERSION_MINIMUM := 3.5

.PHONY: build build-release build-debug run run-release run-debug profile run-profile auto-profile-flat auto-profile-normal clean test check fmt lint checkall sprite-gen run-p1 run-p2 reset reset-p1 reset-p2 new-flat

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

# Profiling target (writes timestamped csv to profiles/)
profile: run-profile

run-profile: build-release
	./target/release/voxel_world --verbose --profile --debug-interval 120 --view-distance 8 --fly-mode $(ARGS)

# Auto-profile: automated 45s test cycling through each feature flag
# Use auto-profile-flat or auto-profile-normal for clean world tests
auto-profile-flat: reset build-release
	./target/release/voxel_world --auto-profile --world-gen flat --fly-mode $(ARGS)

auto-profile-normal: reset build-release
	./target/release/voxel_world --auto-profile --world-gen normal --fly-mode $(ARGS)

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

# Reset default data (worlds, prefs, profiles)
reset:
	rm -rf worlds user_prefs.json profiles

# Create fresh flat world
new-flat: reset build-release
	./target/release/voxel_world --world-gen flat $(ARGS)

# Multi-instance targets (isolated data directories)
run-p1: build-release
	./target/release/voxel_world --data-dir data_p1 $(ARGS)

run-p2: build-release
	./target/release/voxel_world --data-dir data_p2 $(ARGS)

reset-p1:
	rm -rf data_p1

reset-p2:
	rm -rf data_p2
