#!/usr/bin/env bash
# bench_stats.sh — Thermal-aware aggregate stats for benchmark profile CSV files.
#
# Usage:
#   scripts/bench_stats.sh [path.csv ...]
#   scripts/bench_stats.sh                      # newest 2 CSVs under profiles/
#
# Applies a canonical filter to every CSV so A/B comparisons never get polluted
# by warmup, hitches, or thermal throttling:
#
#   t > 5s                   — skip initial chunk-streaming spike
#   frame_ms < 50            — drop single-frame hitches
#   on_battery == 0          — battery runs have a different power envelope
#   gpu_power_w > 25 OR       — on AC, GPU power under ~25W means the OS is
#   (no thermal cols)          actively throttling to protect the SoC
#
# The last clause only applies when the CSV actually has the post-2026-04-19
# thermal columns; older CSVs fall back to the warmup+hitch filter so you can
# still diff against pre-thermal baselines.

set -euo pipefail

PROFILES_DIR="$(cd "$(dirname "$0")/.." && pwd)/profiles"

# If no args: pick the two newest CSVs so `make benchmark-compare` Just Works.
if [ "$#" -eq 0 ]; then
    # shellcheck disable=SC2207
    files=($(ls -t "$PROFILES_DIR"/*.csv 2>/dev/null | head -2))
    if [ "${#files[@]}" -eq 0 ]; then
        echo "No CSVs under $PROFILES_DIR — run 'make benchmark-normal' first." >&2
        exit 1
    fi
else
    files=("$@")
fi

# awk does all the filtering + aggregation. Column indices from stats.rs:
#   3=fps  4=frame_ms  32=chunkload  33=upload  35=metadata  36=render
#   49=gpu_mhz  50=cpu_power_w  51=gpu_power_w  52=cpu_temp_c  53=gpu_temp_c  54=on_battery
for f in "${files[@]}"; do
    echo "=== $(basename "$f") ==="
    awk -F, '
        NR == 1 {
            # Detect schema era by column count:
            # 46 = original, no thermal or battery columns
            # 53 = thermal columns added, no on_battery yet
            # 54 = full schema (thermal + on_battery)
            has_thermal = (NF >= 53)
            has_battery = (NF >= 54)
            next
        }
        $2 == "" { next }
        # Universal filters
        $2 < 5 { skipped_warmup++; next }
        $4+0 > 50 { skipped_hitch++; next }
        # Battery filter — battery runs are a different power envelope
        has_battery && $54+0 != 0 { skipped_battery++; next }
        # Throttle filter — gpu_power_w < 25W on AC means the OS is clamping
        has_thermal && $51+0 > 0 && $51+0 < 25 { skipped_throttle++; next }
        {
            n++
            fps_sum += $3
            frame_sum += $4
            render_sum += $36
            meta_sum += $35
            upload_sum += $33
            cl_sum += $32
            if ($3 > fps_max) fps_max = $3
            if (fps_min == 0 || $3 < fps_min) fps_min = $3
            if (has_thermal) {
                gpu_mhz_sum += $49
                cpu_w_sum += $50
                gpu_w_sum += $51
                if ($52+0 > cpu_t_max) cpu_t_max = $52
                if ($53+0 > gpu_t_max) gpu_t_max = $53
                cpu_t_sum += $52
                gpu_t_sum += $53
                had_thermal_rows++
            }
        }
        END {
            if (n == 0) {
                print "  (no samples passed the filter)"
                exit
            }
            printf "  samples kept = %d", n
            dropped = skipped_warmup + skipped_hitch + skipped_battery + skipped_throttle
            if (dropped > 0) {
                printf " (dropped: warmup=%d hitch=%d battery=%d throttle=%d)",
                    skipped_warmup+0, skipped_hitch+0, skipped_battery+0, skipped_throttle+0
            }
            printf "\n"
            printf "  FPS avg=%.1f  min=%d  max=%d\n", fps_sum/n, fps_min, fps_max
            printf "  frame=%.2fms  render=%.2fms  meta=%.3fms  upload=%.3fms  chunkload=%.3fms\n",
                frame_sum/n, render_sum/n, meta_sum/n, upload_sum/n, cl_sum/n
            if (had_thermal_rows > 0) {
                printf "  thermals: GPU %.0fMHz avg  CPU %.1fW  GPU %.1fW  |  CPU %.1f°C avg (max %.0f)  GPU %.1f°C avg (max %.0f)\n",
                    gpu_mhz_sum/n, cpu_w_sum/n, gpu_w_sum/n,
                    cpu_t_sum/n, cpu_t_max+0, gpu_t_sum/n, gpu_t_max+0
            } else {
                printf "  thermals: (no thermal columns in this CSV)\n"
            }
        }
    ' "$f"
done
