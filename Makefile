# Voxel World Makefile

# Vulkan environment variables for macOS
# Keep both the main Homebrew lib dir and the vulkan-loader keg to avoid search misses.
export DYLD_LIBRARY_PATH := /opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib
export DYLD_FALLBACK_LIBRARY_PATH := /opt/homebrew/lib:/opt/homebrew/opt/vulkan-loader/lib
export VK_ICD_FILENAMES := /opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json
export CMAKE_POLICY_VERSION_MINIMUM := 3.5
export SHADERC_LIB_DIR := /opt/homebrew/lib

.PHONY: build build-release build-debug run run-release run-debug profile run-profile auto-profile-flat auto-profile-normal clean test check fmt lint checkall sprite-gen run-p1 run-p2 reset reset-world reset-profiles reset-p1 reset-p2 new-flat new-normal run-cap-exit

# Default target
all: build-release

# Build targets
build: build-release

build-release:
	cargo build --release

build-debug:
	cargo build

# Run targets (pass CLI args via `make run ARGS="--flag"`).
SEED ?= 314159
ARGS ?=

run: run-release

run-no-build:
	./target/release/voxel-world --seed $(SEED) $(ARGS)

run-release: build-release
	./target/release/voxel-world --seed $(SEED) $(ARGS)

run-debug: build-debug
	@echo "DYLD_LIBRARY_PATH=$(DYLD_LIBRARY_PATH)"
	@echo "DYLD_FALLBACK_LIBRARY_PATH=$(DYLD_FALLBACK_LIBRARY_PATH)"
	@echo "VK_ICD_FILENAMES=$(VK_ICD_FILENAMES)"
	DYLD_LIBRARY_PATH=$(DYLD_LIBRARY_PATH) DYLD_FALLBACK_LIBRARY_PATH=$(DYLD_FALLBACK_LIBRARY_PATH) VK_ICD_FILENAMES=$(VK_ICD_FILENAMES) RUST_BACKTRACE=1 ./target/debug/voxel-world --seed $(SEED) $(ARGS)

# Profiling target (writes timestamped csv to profiles/)
profile: run-profile

run-profile: build-release
	./target/release/voxel_world --verbose --profile --debug-interval 120 --view-distance 8 --fly-mode $(ARGS)

# Auto-profile: automated 45s test cycling through each feature flag
# Resets world but preserves profiles for comparison
auto-profile-flat: reset-world build-release
	./target/release/voxel_world --auto-profile --world-gen flat --seed $(SEED) --fly-mode $(ARGS)

auto-profile-normal: reset-world build-release
	./target/release/voxel_world --auto-profile --world-gen normal --seed $(SEED) --fly-mode $(ARGS)

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

# Run game, take screenshot after 3s, exit after 4s (for visual debugging)
# Usage: make run-cap-exit
#        Set up your scene, position camera, wait for capture and auto-exit
#        Screenshot saved to: voxel-world_screen_shot.png
run-cap-exit: build-release
	./target/release/voxel_world --seed $(SEED) --screenshot-delay 3 --exit-delay 4 $(ARGS)

# Reset world data only (preserves profiles for comparison)
reset-world:
	rm -rf worlds user_prefs.json

# Reset profile data only
reset-profiles:
	rm -rf profiles
	@echo "Profiles cleared."

# Reset ALL data (worlds, prefs, profiles) - requires confirmation
reset:
	@echo "This will delete: worlds/, user_prefs.json, profiles/"
	@read -p "Are you sure? [y/N] " confirm && [ "$$confirm" = "y" ] || [ "$$confirm" = "Y" ] || (echo "Aborted." && exit 1)
	rm -rf worlds user_prefs.json profiles
	@echo "Reset complete."

# Create fresh flat world (preserves profiles)
new-flat: reset-world build-release
	./target/release/voxel_world --world-gen flat --seed $(SEED) $(ARGS)

# Create fresh normal world (preserves profiles)
new-normal: reset-world build-release
	./target/release/voxel_world --world-gen normal --seed $(SEED) $(ARGS)

# Multi-instance targets (isolated data directories)
run-p1: build-release
	./target/release/voxel_world --data-dir data_p1 $(ARGS)

run-p2: build-release
	./target/release/voxel_world --data-dir data_p2 $(ARGS)

reset-p1:
	rm -rf data_p1

reset-p2:
	rm -rf data_p2

# Benchmark targets for controlled profiling
# Resets world but preserves profiles for comparison. Use 'make reset' to clear profiles.
# Benchmark flat terrain, straight flight, 60s at 2x speed (flat terrain is simple)
benchmark: reset-world build-release
	./target/release/voxel_world --world-gen benchmark --auto-fly \
		--auto-fly-speed 40 --profile --benchmark-duration 60 --view-distance 8 --seed $(SEED) $(ARGS)

# Benchmark with hills for more realistic GPU load
benchmark-hills: reset-world build-release
	./target/release/voxel_world --world-gen benchmark --benchmark-terrain hills \
		--auto-fly --profile --benchmark-duration 60 --view-distance 8 --seed $(SEED) $(ARGS)

# Benchmark with spiral pattern, 120s
benchmark-spiral: reset-world build-release
	./target/release/voxel_world --world-gen benchmark --auto-fly \
		--auto-fly-pattern spiral --profile --benchmark-duration 120 \
		--view-distance 8 --seed $(SEED) $(ARGS)

# Benchmark normal terrain (real-world streaming test)
benchmark-normal: reset-world build-release
	./target/release/voxel_world --world-gen normal --auto-fly \
		--profile --benchmark-duration 60 --view-distance 8 --seed $(SEED) $(ARGS)

# Stress test at 2x speed
benchmark-stress: reset-world build-release
	./target/release/voxel_world --world-gen benchmark --auto-fly \
		--auto-fly-speed 40 --profile --benchmark-duration 60 \
		--view-distance 8 --seed $(SEED) $(ARGS)

# Quick benchmark with screenshot
benchmark-cap: reset-world build-release
	./target/release/voxel_world --world-gen benchmark --auto-fly \
		--benchmark-duration 30 --screenshot-delay 25 --seed $(SEED) $(ARGS)
