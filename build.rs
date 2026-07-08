/// Build script: generates `shaders/generated_constants.glsl` from Rust source files.
///
/// This eliminates the dual-maintenance burden of keeping GLSL constants in sync
/// with Rust enums/constants.  Cargo re-runs this script whenever the source files
/// change, so the generated file is always fresh before the main compile begins.
///
/// Sources read:
/// - `src/chunk.rs` — BlockType enum, WaterType enum, TINT_PALETTE const, CHUNK_SIZE
/// - `src/constants.rs` — ATLAS_TILE_COUNT, WORLD_CHUNKS_Y, LOADED_CHUNKS_X/Z
/// - `src/render_mode.rs` — RenderMode enum
/// - `src/svt.rs` — BRICK_SIZE
/// - `src/sub_voxel/types.rs` — LightMode enum, CRYSTAL_MODEL_ID, FIRST_CUSTOM_MODEL_ID
///
/// Output: `shaders/generated_constants.glsl`
///
/// **Fail-loud contract:** every constant below is required. If a parser can't
/// locate its source-of-truth declaration (e.g. an enum was renamed, a const was
/// deleted), `main()` panics naming the constant and the file it expected to find
/// it in. We never silently emit a stale or empty value — that is how GLSL/Rust
/// drift sneaks in.
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::Path;

fn main() {
    // Tell Cargo to re-run this build script when these source files change.
    println!("cargo:rerun-if-changed=src/chunk.rs");
    println!("cargo:rerun-if-changed=src/constants.rs");
    println!("cargo:rerun-if-changed=src/render_mode.rs");
    println!("cargo:rerun-if-changed=src/svt.rs");
    println!("cargo:rerun-if-changed=src/sub_voxel/types.rs");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let root = Path::new(&manifest_dir);

    let chunk_src = read_src(root, "src/chunk.rs");
    let constants_src = read_src(root, "src/constants.rs");
    let render_mode_src = read_src(root, "src/render_mode.rs");
    let svt_src = read_src(root, "src/svt.rs");
    let types_src = read_src(root, "src/sub_voxel/types.rs");

    // Every parse below is required: a None / empty result panics with a label
    // the next dev can act on, rather than emitting a silent default.
    let block_variants = require_variants(
        parse_enum_variants(&chunk_src, "BlockType"),
        "BlockType",
        "src/chunk.rs",
    );
    let water_variants = require_variants(
        parse_enum_variants(&chunk_src, "WaterType"),
        "WaterType",
        "src/chunk.rs",
    );
    let render_mode_variants = require_variants(
        parse_enum_variants(&render_mode_src, "RenderMode"),
        "RenderMode",
        "src/render_mode.rs",
    );
    let light_mode_variants = require_variants(
        parse_enum_variants(&types_src, "LightMode"),
        "LightMode",
        "src/sub_voxel/types.rs",
    );

    let atlas_tile_count = require_const(
        parse_u32_const(&constants_src, "ATLAS_TILE_COUNT"),
        "ATLAS_TILE_COUNT",
        "src/constants.rs",
    );
    let chunk_size = require_const(
        parse_usize_const(&chunk_src, "CHUNK_SIZE"),
        "CHUNK_SIZE",
        "src/chunk.rs",
    );
    let brick_size = require_const(
        parse_usize_const(&svt_src, "BRICK_SIZE"),
        "BRICK_SIZE",
        "src/svt.rs",
    );
    let world_chunks_y = require_const(
        parse_usize_const(&constants_src, "WORLD_CHUNKS_Y"),
        "WORLD_CHUNKS_Y",
        "src/constants.rs",
    );
    let loaded_chunks_x = require_const(
        parse_usize_const(&constants_src, "LOADED_CHUNKS_X"),
        "LOADED_CHUNKS_X",
        "src/constants.rs",
    );
    let loaded_chunks_z = require_const(
        parse_usize_const(&constants_src, "LOADED_CHUNKS_Z"),
        "LOADED_CHUNKS_Z",
        "src/constants.rs",
    );
    let crystal_model_id = require_const(
        parse_usize_const(&types_src, "CRYSTAL_MODEL_ID"),
        "CRYSTAL_MODEL_ID",
        "src/sub_voxel/types.rs",
    );
    let first_custom_model_id = require_const(
        parse_usize_const(&types_src, "FIRST_CUSTOM_MODEL_ID"),
        "FIRST_CUSTOM_MODEL_ID",
        "src/sub_voxel/types.rs",
    );
    let tint_palette = parse_tint_palette(&chunk_src);
    if tint_palette.is_empty() {
        panic!(
            "build.rs: could not parse TINT_PALETTE from src/chunk.rs — expected `pub const TINT_PALETTE:` with a non-empty array"
        );
    }

    let glsl = generate_glsl(
        &block_variants,
        &water_variants,
        &light_mode_variants,
        atlas_tile_count,
        &render_mode_variants,
        chunk_size,
        brick_size,
        loaded_chunks_x,
        world_chunks_y,
        loaded_chunks_z,
        crystal_model_id,
        first_custom_model_id,
        &tint_palette,
    );

    let out_path = root.join("shaders/generated_constants.glsl");
    // Only write if the content has changed to avoid spurious recompiles.
    let existing = fs::read_to_string(&out_path).unwrap_or_default();
    if existing != glsl {
        fs::write(&out_path, &glsl)
            .expect("build.rs: failed to write shaders/generated_constants.glsl");
    }
}

/// Read a source file relative to the crate root, panicking with the path on failure.
fn read_src(root: &Path, rel: &str) -> String {
    fs::read_to_string(root.join(rel))
        .unwrap_or_else(|e| panic!("build.rs: failed to read {rel}: {e}"))
}

/// Assert a parsed enum yielded at least one variant, else name what's missing.
fn require_variants(
    variants: Vec<(String, u64)>,
    enum_name: &str,
    file: &str,
) -> Vec<(String, u64)> {
    if variants.is_empty() {
        panic!(
            "build.rs: could not parse any variants from `{enum_name}` in {file} \
             — has the enum been renamed, or do its variants no longer carry explicit `= <int>` discriminants?"
        );
    }
    variants
}

/// Unwrap a required parsed constant, naming it and its expected file on miss.
fn require_const<T>(opt: Option<T>, name: &str, file: &str) -> T {
    opt.unwrap_or_else(|| {
        panic!(
            "build.rs: required constant `{name}` not found in {file} \
             — has it been renamed, deleted, or had its type changed?"
        )
    })
}

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

/// Parse variants of a C-style enum with explicit integer discriminants.
///
/// Recognises lines of the form `VariantName = <integer>,` inside the body of
/// `pub enum <enum_name> { ... }`. Returns `(variant_name, discriminant)` pairs
/// in declaration order. Returns an empty vec when the enum declaration itself
/// is missing — the caller decides whether that is fatal (see `require_variants`).
///
/// Used for `BlockType`, `WaterType`, `RenderMode`, and `LightMode`, which all
/// share the `#[repr(uN)]` + explicit-discriminant layout.
fn parse_enum_variants(src: &str, enum_name: &str) -> Vec<(String, u64)> {
    let needle = format!("pub enum {enum_name} {{");
    let enum_start = match src.find(&needle) {
        Some(pos) => pos,
        None => return Vec::new(),
    };

    let body = extract_brace_body(&src[enum_start..]);
    let mut variants: Vec<(String, u64)> = Vec::new();

    for line in body.lines() {
        let trimmed = line.trim();
        // Skip empty lines and doc-comment / attribute lines.
        if trimmed.is_empty()
            || trimmed.starts_with("//")
            || trimmed.starts_with('#')
            || trimmed.starts_with("/*")
        {
            continue;
        }

        // Strip trailing comma and optional inline comment.
        let without_comment = if let Some(pos) = trimmed.find("//") {
            trimmed[..pos].trim()
        } else {
            trimmed
        };
        let without_comma = without_comment.trim_end_matches(',').trim();

        // We only care about lines with an explicit `= <number>`.
        if let Some(eq_pos) = without_comma.find('=') {
            let variant_name = without_comma[..eq_pos].trim().to_string();
            let discriminant_str = without_comma[eq_pos + 1..].trim();
            if let Ok(discriminant) = discriminant_str.parse::<u64>()
                && variant_name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase())
            {
                variants.push((variant_name, discriminant));
            }
        }
    }

    variants
}

/// Extract the contents of the first `{…}` block starting from `src`.
fn extract_brace_body(src: &str) -> &str {
    let open = match src.find('{') {
        Some(p) => p + 1,
        None => return "",
    };
    let mut depth = 1usize;
    let bytes = src.as_bytes();
    let mut i = open;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    &src[open..i - 1]
}

/// Generic `pub const NAME: usize = <N>` / `const NAME: usize = <N>` /
/// `pub const NAME: i32 = <N>` parser. Returns `None` when the constant is
/// not found or the RHS isn't a plain non-negative integer literal. Inline
/// `//` trailing comments are stripped so `= 16; // blah` parses correctly.
fn parse_usize_const(src: &str, name: &str) -> Option<usize> {
    for line in src.lines() {
        let trimmed = line.trim();
        let with_pub = format!("pub const {}:", name);
        let without_pub = format!("const {}:", name);
        if !(trimmed.starts_with(&with_pub) || trimmed.starts_with(&without_pub)) {
            continue;
        }
        // Strip a trailing `// ...` comment before pulling off the `;`.
        let without_comment = if let Some(pos) = trimmed.find("//") {
            trimmed[..pos].trim()
        } else {
            trimmed
        };
        let eq_pos = without_comment.find('=')?;
        let rhs = without_comment[eq_pos + 1..]
            .trim()
            .trim_end_matches(';')
            .trim();
        if let Ok(n) = rhs.parse::<i64>()
            && n >= 0
        {
            return Some(n as usize);
        }
    }
    None
}

/// Parse the `TINT_PALETTE` const array from `src/chunk.rs`. Returns a vec of
/// `(r, g, b)` triples in declaration order. Empty vec on failure so the
/// generator can skip emitting a partial array rather than lie about colors.
fn parse_tint_palette(src: &str) -> Vec<(f32, f32, f32)> {
    let needle = "pub const TINT_PALETTE:";
    let Some(start) = src.find(needle) else {
        return Vec::new();
    };
    let from = &src[start..];
    // The type annotation contains its own `[...]` so we anchor on `=` and
    // only look at the value side.
    let Some(eq_pos) = from.find('=') else {
        return Vec::new();
    };
    let value_side = &from[eq_pos + 1..];
    let Some(value_open) = value_side.find('[') else {
        return Vec::new();
    };
    // Find the matching ']' at bracket-depth zero.
    let bytes = value_side.as_bytes();
    let mut depth = 1i32;
    let mut i = value_open + 1;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    let body = &value_side[value_open + 1..i - 1];

    let mut out = Vec::new();
    // Each entry looks like `[0.95, 0.95, 0.95],` possibly with trailing comment.
    let mut inner = body;
    while !inner.is_empty() {
        let Some(lb) = inner.find('[') else { break };
        let Some(rb) = inner[lb..].find(']') else {
            break;
        };
        let triple = &inner[lb + 1..lb + rb];
        let parts: Vec<f32> = triple
            .split(',')
            .filter_map(|s| s.trim().parse::<f32>().ok())
            .collect();
        if parts.len() >= 3 {
            out.push((parts[0], parts[1], parts[2]));
        }
        inner = &inner[lb + rb + 1..];
    }
    out
}

/// Extract `<NAME>: <int-type> = <N>` from a Rust source as a `u32`.
///
/// Matches `pub const NAME:` or `const NAME:` regardless of the integer type
/// annotation (u8/u16/u32/usize/i32/etc.) and parses the RHS literal. Returns
/// `None` when the declaration is absent or the RHS isn't a plain non-negative
/// integer literal — `main()` treats that as fatal for required constants.
fn parse_u32_const(src: &str, name: &str) -> Option<u32> {
    let parsed = parse_usize_const(src, name)?;
    if parsed <= u32::MAX as usize {
        Some(parsed as u32)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Code generator
// ---------------------------------------------------------------------------

/// Convert a PascalCase Rust identifier to a SCREAMING_SNAKE_CASE GLSL name.
///
/// Examples:
///   `Air`              → `AIR`
///   `GlowMushroom`     → `GLOW_MUSHROOM`
///   `MossyCobblestone` → `MOSSY_COBBLESTONE`
///   `TintedGlass`      → `TINTED_GLASS`
///   `BrickDebug`       → `BRICK_DEBUG`
fn pascal_to_screaming_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 8);
    let chars: Vec<char> = name.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_uppercase() && i > 0 {
            // Insert underscore before an uppercase letter unless the previous
            // character was already uppercase AND the next is lowercase
            // (handles acronyms like "UV" → "UV", not "U_V").
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

#[allow(clippy::too_many_arguments)]
fn generate_glsl(
    block_variants: &[(String, u64)],
    water_variants: &[(String, u64)],
    light_mode_variants: &[(String, u64)],
    atlas_tile_count: u32,
    render_mode_variants: &[(String, u64)],
    chunk_size: usize,
    brick_size: usize,
    chunks_x: usize,
    chunks_y: usize,
    chunks_z: usize,
    crystal_model_id: usize,
    first_custom_model_id: usize,
    tint_palette: &[(f32, f32, f32)],
) -> String {
    let mut out = String::new();

    writeln!(out, "// AUTO-GENERATED by build.rs — DO NOT EDIT MANUALLY").unwrap();
    writeln!(
        out,
        "// Source of truth: src/chunk.rs, src/constants.rs, src/render_mode.rs, src/svt.rs,"
    )
    .unwrap();
    writeln!(out, "//                     src/sub_voxel/types.rs").unwrap();
    writeln!(out).unwrap();

    // --- BlockType constants ---
    writeln!(
        out,
        "// Block types (generated from src/chunk.rs BlockType enum)"
    )
    .unwrap();
    for (name, disc) in block_variants {
        let glsl_name = pascal_to_screaming_snake(name);
        writeln!(out, "#define BLOCK_{glsl_name} {disc}u").unwrap();
    }
    writeln!(out).unwrap();

    // --- WaterType constants ---
    writeln!(
        out,
        "// Water types (generated from src/chunk.rs WaterType enum)"
    )
    .unwrap();
    for (name, disc) in water_variants {
        let glsl_name = pascal_to_screaming_snake(name);
        writeln!(out, "#define WATER_TYPE_{glsl_name} {disc}u").unwrap();
    }
    writeln!(out).unwrap();

    // --- LightMode constants ---
    writeln!(
        out,
        "// Light animation modes (generated from src/sub_voxel/types.rs LightMode enum)"
    )
    .unwrap();
    for (name, disc) in light_mode_variants {
        let glsl_name = pascal_to_screaming_snake(name);
        writeln!(out, "#define LIGHT_MODE_{glsl_name} {disc}u").unwrap();
    }
    writeln!(out).unwrap();

    // --- Sub-voxel model ID constants ---
    writeln!(
        out,
        "// Sub-voxel model IDs (generated from src/sub_voxel/types.rs)"
    )
    .unwrap();
    writeln!(out, "#define CRYSTAL_MODEL_ID {crystal_model_id}u").unwrap();
    writeln!(
        out,
        "#define FIRST_CUSTOM_MODEL_ID {first_custom_model_id}u"
    )
    .unwrap();
    writeln!(out).unwrap();

    // --- ATLAS_TILE_COUNT ---
    writeln!(
        out,
        "// Texture atlas tile count (generated from src/constants.rs)"
    )
    .unwrap();
    writeln!(out, "const float ATLAS_TILE_COUNT = {atlas_tile_count}.0;").unwrap();
    writeln!(out, "const float ATLAS_TILE_SIZE = 1.0 / ATLAS_TILE_COUNT;").unwrap();
    writeln!(out).unwrap();

    // --- World/chunk dimensions (generated) ---
    writeln!(
        out,
        "// World/chunk dimensions (generated from src/chunk.rs + src/constants.rs + src/svt.rs)"
    )
    .unwrap();
    writeln!(out, "const uint CHUNK_SIZE = {chunk_size}u;").unwrap();
    writeln!(out, "const uint CHUNKS_X = {chunks_x}u;").unwrap();
    writeln!(out, "const uint CHUNKS_Y = {chunks_y}u;").unwrap();
    writeln!(out, "const uint CHUNKS_Z = {chunks_z}u;").unwrap();
    writeln!(out, "const uint BRICK_SIZE = {brick_size}u;").unwrap();
    let bricks_per_axis = (chunk_size / brick_size.max(1)) as u32;
    let bricks_per_chunk = bricks_per_axis * bricks_per_axis * bricks_per_axis;
    writeln!(out, "const uint BRICKS_PER_AXIS = {bricks_per_axis}u;").unwrap();
    writeln!(out, "const uint BRICKS_PER_CHUNK = {bricks_per_chunk}u;").unwrap();
    writeln!(out).unwrap();

    // --- TINT_PALETTE (32 colors) ---
    // `main()` rejects an empty palette before reaching here, so this branch
    // is about output shape, not optionality.
    if !tint_palette.is_empty() {
        writeln!(
            out,
            "// Tint palette (generated from src/chunk.rs TINT_PALETTE)"
        )
        .unwrap();
        writeln!(
            out,
            "const vec3 TINT_PALETTE[{}] = vec3[{}](",
            tint_palette.len(),
            tint_palette.len()
        )
        .unwrap();
        for (i, (r, g, b)) in tint_palette.iter().enumerate() {
            let comma = if i + 1 < tint_palette.len() { "," } else { "" };
            writeln!(out, "    vec3({:.4}, {:.4}, {:.4}){}", r, g, b, comma).unwrap();
        }
        writeln!(out, ");").unwrap();
        writeln!(out).unwrap();
    }

    // --- RenderMode constants ---
    writeln!(
        out,
        "// Render modes (generated from src/render_mode.rs RenderMode enum)"
    )
    .unwrap();
    for (name, disc) in render_mode_variants {
        let glsl_name = pascal_to_screaming_snake(name);
        writeln!(out, "#define RENDER_MODE_{glsl_name} {disc}u").unwrap();
    }
    writeln!(out).unwrap();

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pascal_to_screaming_snake() {
        assert_eq!(pascal_to_screaming_snake("Air"), "AIR");
        assert_eq!(pascal_to_screaming_snake("GlowMushroom"), "GLOW_MUSHROOM");
        assert_eq!(
            pascal_to_screaming_snake("MossyCobblestone"),
            "MOSSY_COBBLESTONE"
        );
        assert_eq!(pascal_to_screaming_snake("TintedGlass"), "TINTED_GLASS");
        assert_eq!(pascal_to_screaming_snake("BrickDebug"), "BRICK_DEBUG");
        assert_eq!(pascal_to_screaming_snake("UV"), "UV");
        assert_eq!(pascal_to_screaming_snake("BirchLog"), "BIRCH_LOG");
        assert_eq!(
            pascal_to_screaming_snake("DecorativeStone"),
            "DECORATIVE_STONE"
        );
        assert_eq!(pascal_to_screaming_snake("PackedIce"), "PACKED_ICE");
        assert_eq!(pascal_to_screaming_snake("CoarseDirt"), "COARSE_DIRT");
        assert_eq!(pascal_to_screaming_snake("RootedDirt"), "ROOTED_DIRT");
    }

    #[test]
    fn test_parse_enum_variants_block_type() {
        let sample = r#"
pub enum BlockType {
    #[default]
    Air = 0,
    Stone = 1,
    /// Some doc
    Dirt = 2,
    GlowMushroom = 21,
}
"#;
        let variants = parse_enum_variants(sample, "BlockType");
        assert_eq!(variants.len(), 4);
        assert_eq!(variants[0], ("Air".to_string(), 0));
        assert_eq!(variants[1], ("Stone".to_string(), 1));
        assert_eq!(variants[2], ("Dirt".to_string(), 2));
        assert_eq!(variants[3], ("GlowMushroom".to_string(), 21));
    }

    #[test]
    fn test_parse_enum_variants_missing_enum_is_empty() {
        // When the enum declaration is absent the parser returns an empty vec;
        // `require_variants` is what escalates that to a panic in `main()`.
        let sample = "pub enum SomethingElse { A = 0 }";
        assert!(parse_enum_variants(sample, "BlockType").is_empty());
    }

    #[test]
    fn test_parse_u32_const() {
        // Works across integer type annotations (usize / u32 / u8 / i32).
        let src = "pub const ATLAS_TILE_COUNT: usize = 45;\n";
        assert_eq!(parse_u32_const(src, "ATLAS_TILE_COUNT"), Some(45));
        let src = "pub const CRYSTAL_MODEL_ID: u8 = 99;\n";
        assert_eq!(parse_u32_const(src, "CRYSTAL_MODEL_ID"), Some(99));
        assert_eq!(parse_u32_const(src, "NOT_THERE"), None);
    }

    #[test]
    fn test_parse_usize_const() {
        let src = r#"
pub const CHUNK_SIZE: usize = 32;
pub const WORLD_CHUNKS_Y: i32 = 16;
const BRICK_SIZE: usize = 8;
"#;
        assert_eq!(parse_usize_const(src, "CHUNK_SIZE"), Some(32));
        assert_eq!(parse_usize_const(src, "WORLD_CHUNKS_Y"), Some(16));
        assert_eq!(parse_usize_const(src, "BRICK_SIZE"), Some(8));
        assert_eq!(parse_usize_const(src, "NOT_THERE"), None);
    }

    #[test]
    fn test_parse_tint_palette() {
        let src = r#"
pub const TINT_PALETTE: [[f32; 3]; 3] = [
    [1.0, 0.2, 0.2],   // red
    [0.5, 0.5, 0.5],   // gray
    [0.0, 0.0, 1.0],   // blue
];
"#;
        let p = parse_tint_palette(src);
        assert_eq!(p.len(), 3);
        assert!((p[0].0 - 1.0).abs() < 1e-6);
        assert!((p[1].1 - 0.5).abs() < 1e-6);
        assert!((p[2].2 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_parse_enum_variants_render_mode() {
        let sample = r#"
pub enum RenderMode {
    Coord = 0,
    Steps = 1,
    #[default]
    Textured = 2,
}
"#;
        let variants = parse_enum_variants(sample, "RenderMode");
        assert_eq!(variants.len(), 3);
        assert_eq!(variants[0], ("Coord".to_string(), 0));
        assert_eq!(variants[1], ("Steps".to_string(), 1));
        assert_eq!(variants[2], ("Textured".to_string(), 2));
    }

    /// Round-trip: every variant the parser extracts must appear in the
    /// generated GLSL with the same discriminant. This is the local mirror of
    /// the cross-check that `tests/build_codegen.rs` runs against the live
    /// source files; together they catch both parser drift and generator drift.
    #[test]
    fn test_generate_glsl_round_trip() {
        let blocks = vec![
            ("Air".to_string(), 0),
            ("Stone".to_string(), 1),
            ("GlowMushroom".to_string(), 21),
        ];
        let waters = vec![
            ("Ocean".to_string(), 0),
            ("Lake".to_string(), 1),
            ("River".to_string(), 2),
            ("Swamp".to_string(), 3),
            ("Spring".to_string(), 4),
        ];
        let lights = vec![("Steady".to_string(), 0), ("Arc".to_string(), 9)];
        let modes = vec![("Textured".to_string(), 2)];

        let glsl = generate_glsl(
            &blocks,
            &waters,
            &lights,
            45,
            &modes,
            32,
            8,
            16,
            16,
            16,
            99,
            176,
            &[(1.0, 0.0, 0.0)],
        );

        // Block defines
        assert!(glsl.contains("#define BLOCK_AIR 0u"));
        assert!(glsl.contains("#define BLOCK_STONE 1u"));
        assert!(glsl.contains("#define BLOCK_GLOW_MUSHROOM 21u"));
        // Water defines
        assert!(glsl.contains("#define WATER_TYPE_OCEAN 0u"));
        assert!(glsl.contains("#define WATER_TYPE_SPRING 4u"));
        // Light defines
        assert!(glsl.contains("#define LIGHT_MODE_STEADY 0u"));
        assert!(glsl.contains("#define LIGHT_MODE_ARC 9u"));
        // Model IDs
        assert!(glsl.contains("#define CRYSTAL_MODEL_ID 99u"));
        assert!(glsl.contains("#define FIRST_CUSTOM_MODEL_ID 176u"));
        // Sanity: ensure no stray `const uint` collision with the #define form.
        assert!(!glsl.contains("const uint WATER_TYPE"));
        assert!(!glsl.contains("const uint LIGHT_MODE"));
    }
}
