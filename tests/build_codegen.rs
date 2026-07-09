//! Cross-check that `shaders/generated_constants.glsl` is in sync with its Rust
//! sources of truth.
//!
//! `build.rs` regenerates the GLSL during `cargo build`, and `cargo test` runs
//! after the build, so by the time this test executes the generated file is
//! fresh. This test independently re-derives the expected values from the Rust
//! sources and asserts the generated GLSL matches — catching drift in either
//! direction (someone editing Rust without rebuilding, or a parser in build.rs
//! that silently disagrees with the source).
//!
//! These tests intentionally do NOT reuse build.rs's parser code (integration
//! tests can't import from build.rs). A shared bug would then pass both. The
//! independent re-derivation here is the safety net.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Locate the crate root regardless of where `cargo test` invoked the test
/// binary from. `CARGO_MANIFEST_DIR` is baked in at compile time.
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Parse every `#define NAME VALUEu` line in `src` into a map.
/// Skips `const` declarations (handled by `parse_glsl_const_uints`).
fn parse_glsl_defines(src: &str) -> HashMap<String, u64> {
    let mut out = HashMap::new();
    for line in src.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("#define ") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let Some(val) = parts.next() else { continue };
        let Some(digits) = val.strip_suffix('u') else {
            continue;
        };
        if let Ok(n) = digits.parse::<u64>() {
            out.insert(name.to_string(), n);
        }
    }
    out
}

/// Parse every `const uint NAME = VALUEu;` line in `src` into a map.
/// Used for the chunk-dimension constants that build.rs emits as `const uint`
/// rather than `#define`.
fn parse_glsl_const_uints(src: &str) -> HashMap<String, u64> {
    let mut out = HashMap::new();
    for line in src.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("const uint ") else {
            continue;
        };
        // rest looks like `NAME = VALUEu;`
        let Some(eq) = rest.find('=') else {
            continue;
        };
        let name = rest[..eq].trim();
        let rhs = rest[eq + 1..].trim().trim_end_matches(';').trim();
        let Some(digits) = rhs.strip_suffix('u') else {
            continue;
        };
        if let Ok(n) = digits.parse::<u64>() {
            out.insert(name.to_string(), n);
        }
    }
    out
}

/// Parse `pub const NAME: <int-type> = <N>;` (with optional trailing `// ...`
/// comment) from a Rust source. Returns the integer value or panics if missing.
fn require_rust_const(src: &str, name: &str) -> u64 {
    for line in src.lines() {
        let t = line.trim();
        let without_comment = if let Some(pos) = t.find("//") {
            &t[..pos]
        } else {
            t
        };
        let with_pub = format!("pub const {}:", name);
        let without_pub = format!("const {}:", name);
        if (without_comment.starts_with(&with_pub) || without_comment.starts_with(&without_pub))
            && let Some(eq) = without_comment.find('=')
        {
            let rhs = without_comment[eq + 1..]
                .trim()
                .trim_end_matches(';')
                .trim();
            if let Ok(n) = rhs.parse::<u64>() {
                return n;
            }
        }
    }
    panic!("cross-check: required Rust const `{name}` not found — has it been renamed or moved?");
}

/// Parse variants of `pub enum <enum_name> { ... }` with explicit `= <int>`
/// discriminants. Returns `(name, discriminant)` pairs in declaration order.
fn parse_rust_enum_variants(src: &str, enum_name: &str) -> Vec<(String, u64)> {
    let needle = format!("pub enum {enum_name} {{");
    let Some(start) = src.find(&needle) else {
        panic!("cross-check: `pub enum {enum_name}` not found in Rust source");
    };
    // Extract brace body.
    let after_open = &src[start..];
    let bo = after_open.find('{').unwrap() + 1;
    let bytes = after_open.as_bytes();
    let mut depth = 1i32;
    let mut i = bo;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    let body = &after_open[bo..i - 1];

    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('#') || t.starts_with("/*") {
            continue;
        }
        let without_comment = if let Some(pos) = t.find("//") {
            &t[..pos]
        } else {
            t
        };
        let without_comma = without_comment.trim_end_matches(',').trim();
        if let Some(eq) = without_comma.find('=') {
            let name = without_comma[..eq].trim();
            let val_str = without_comma[eq + 1..].trim();
            if let (Ok(v), true) = (
                val_str.parse::<u64>(),
                name.chars().next().is_some_and(|c| c.is_uppercase()),
            ) {
                out.push((name.to_string(), v));
            }
        }
    }
    out
}

/// Convert PascalCase → SCREAMING_SNAKE_CASE mirroring build.rs.
fn pascal_to_screaming_snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() && i > 0 {
            let prev_upper = chars[i - 1].is_uppercase();
            let next_lower = chars.get(i + 1).is_some_and(|nc| nc.is_lowercase());
            if !prev_upper || next_lower {
                out.push('_');
            }
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

#[test]
fn generated_file_matches_rust_sources() {
    let root = root();
    let glsl = fs::read_to_string(root.join("shaders/generated_constants.glsl"))
        .unwrap_or_else(|_| {
            panic!(
                "shaders/generated_constants.glsl missing — build.rs should regenerate it during cargo build"
            )
        });
    let defines = parse_glsl_defines(&glsl);

    let chunk_src = fs::read_to_string(root.join("src/chunk.rs")).unwrap();
    let constants_src = fs::read_to_string(root.join("src/constants.rs")).unwrap();
    let render_mode_src = fs::read_to_string(root.join("src/render_mode.rs")).unwrap();
    let svt_src = fs::read_to_string(root.join("src/svt.rs")).unwrap();
    let types_src = fs::read_to_string(root.join("src/sub_voxel/types.rs")).unwrap();

    let mut failures: Vec<String> = Vec::new();

    // --- Enum categories: every variant must be emitted with the right value. ---
    let block_variants = parse_rust_enum_variants(&chunk_src, "BlockType");
    for (name, disc) in &block_variants {
        let glsl_name = format!("BLOCK_{}", pascal_to_screaming_snake(name));
        match defines.get(&glsl_name) {
            Some(v) if *v == *disc => {}
            Some(v) => failures.push(format!(
                "{glsl_name}: GLSL says {v} but Rust BlockType::{name} = {disc}"
            )),
            None => failures.push(format!("{glsl_name}: missing from generated GLSL")),
        }
    }

    let water_variants = parse_rust_enum_variants(&chunk_src, "WaterType");
    for (name, disc) in &water_variants {
        let glsl_name = format!("WATER_TYPE_{}", pascal_to_screaming_snake(name));
        match defines.get(&glsl_name) {
            Some(v) if *v == *disc => {}
            Some(v) => failures.push(format!(
                "{glsl_name}: GLSL says {v} but Rust WaterType::{name} = {disc}"
            )),
            None => failures.push(format!("{glsl_name}: missing from generated GLSL")),
        }
    }

    let light_variants = parse_rust_enum_variants(&types_src, "LightMode");
    for (name, disc) in &light_variants {
        let glsl_name = format!("LIGHT_MODE_{}", pascal_to_screaming_snake(name));
        match defines.get(&glsl_name) {
            Some(v) if *v == *disc => {}
            Some(v) => failures.push(format!(
                "{glsl_name}: GLSL says {v} but Rust LightMode::{name} = {disc}"
            )),
            None => failures.push(format!("{glsl_name}: missing from generated GLSL")),
        }
    }

    let render_variants = parse_rust_enum_variants(&render_mode_src, "RenderMode");
    for (name, disc) in &render_variants {
        let glsl_name = format!("RENDER_MODE_{}", pascal_to_screaming_snake(name));
        match defines.get(&glsl_name) {
            Some(v) if *v == *disc => {}
            Some(v) => failures.push(format!(
                "{glsl_name}: GLSL says {v} but Rust RenderMode::{name} = {disc}"
            )),
            None => failures.push(format!("{glsl_name}: missing from generated GLSL")),
        }
    }

    // --- Model-ID scalars emitted as `#define`. ---
    let const_uints = parse_glsl_const_uints(&glsl);
    let define_scalars: Vec<(&str, u64)> = vec![
        (
            "CRYSTAL_MODEL_ID",
            require_rust_const(&types_src, "CRYSTAL_MODEL_ID"),
        ),
        (
            "FIRST_CUSTOM_MODEL_ID",
            require_rust_const(&types_src, "FIRST_CUSTOM_MODEL_ID"),
        ),
    ];
    for (name, rust_val) in &define_scalars {
        match defines.get(*name) {
            Some(v) if *v == *rust_val => {}
            Some(v) => failures.push(format!(
                "{name}: GLSL says {v} but Rust value is {rust_val}"
            )),
            None => failures.push(format!("{name}: missing from generated GLSL")),
        }
    }

    // --- Chunk-dimension scalars emitted as `const uint`. ---
    // build.rs maps WORLD_CHUNKS_Y → CHUNKS_Y and LOADED_CHUNKS_{X,Z} →
    // CHUNKS_{X,Z}; the Rust-side names differ from the GLSL-side names, so the
    // cross-check has to apply that mapping.
    let const_uint_checks: Vec<(&str, &str, &str, u64)> = vec![
        (
            "CHUNK_SIZE",
            "CHUNK_SIZE",
            "src/chunk.rs",
            require_rust_const(&chunk_src, "CHUNK_SIZE"),
        ),
        (
            "BRICK_SIZE",
            "BRICK_SIZE",
            "src/svt.rs",
            require_rust_const(&svt_src, "BRICK_SIZE"),
        ),
        (
            "CHUNKS_Y",
            "WORLD_CHUNKS_Y",
            "src/constants.rs",
            require_rust_const(&constants_src, "WORLD_CHUNKS_Y"),
        ),
        (
            "CHUNKS_X",
            "LOADED_CHUNKS_X",
            "src/constants.rs",
            require_rust_const(&constants_src, "LOADED_CHUNKS_X"),
        ),
        (
            "CHUNKS_Z",
            "LOADED_CHUNKS_Z",
            "src/constants.rs",
            require_rust_const(&constants_src, "LOADED_CHUNKS_Z"),
        ),
    ];
    for (glsl_name, rust_name, source_label, rust_val) in &const_uint_checks {
        match const_uints.get(*glsl_name) {
            Some(v) if *v == *rust_val => {}
            Some(v) => failures.push(format!(
                "const uint {glsl_name} (= Rust {rust_name}): GLSL says {v} but Rust value is {rust_val}"
            )),
            None => failures.push(format!(
                "const uint {glsl_name} (from {source_label}::{rust_name}): missing from generated GLSL"
            )),
        }
    }

    // ATLAS_TILE_COUNT is emitted as `const float`, not `#define`, so check the
    // source line directly.
    let atlas = require_rust_const(&constants_src, "ATLAS_TILE_COUNT");
    let expected_line = format!("const float ATLAS_TILE_COUNT = {atlas}.0;");
    if !glsl.contains(&expected_line) {
        failures.push(format!(
            "ATLAS_TILE_COUNT: generated GLSL missing expected line `{expected_line}`"
        ));
    }

    // --- Category counts: ensure no variant was silently dropped or added. ---
    let count_checks: Vec<(&str, usize, &str, usize)> = vec![
        (
            "BLOCK_",
            defines.keys().filter(|k| k.starts_with("BLOCK_")).count(),
            "BlockType variants",
            block_variants.len(),
        ),
        (
            "WATER_TYPE_",
            defines
                .keys()
                .filter(|k| k.starts_with("WATER_TYPE_"))
                .count(),
            "WaterType variants",
            water_variants.len(),
        ),
        (
            "LIGHT_MODE_",
            defines
                .keys()
                .filter(|k| k.starts_with("LIGHT_MODE_"))
                .count(),
            "LightMode variants",
            light_variants.len(),
        ),
        (
            "RENDER_MODE_",
            defines
                .keys()
                .filter(|k| k.starts_with("RENDER_MODE_"))
                .count(),
            "RenderMode variants",
            render_variants.len(),
        ),
    ];
    for (prefix, glsl_count, label, rust_count) in count_checks {
        if glsl_count != rust_count {
            failures.push(format!(
                "{prefix}* count mismatch: {glsl_count} in GLSL vs {rust_count} {label} \
                 — a variant was added/removed on one side but not the other"
            ));
        }
    }

    if !failures.is_empty() {
        panic!(
            "build.rs codegen drift detected ({} mismatches):\n  - {}\n\
             If you edited a Rust enum/const, the build should have regenerated the GLSL; \
             if it did not, check that build.rs's parser still matches the source layout.",
            failures.len(),
            failures.join("\n  - ")
        );
    }
}
