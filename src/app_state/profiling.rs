/// Auto-profile feature being tested
///
/// IMPORTANT: When adding new features here, also update:
/// 1. CSV header in `src/app/stats.rs` (line ~83)
/// 2. CSV data row in `src/app/stats.rs` (line ~96-127)
/// 3. State transition logic in `src/app/update.rs` (lines ~66-109)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoProfileFeature {
    Baseline,              // Initial 5s with all features ON
    AO,                    // Testing enable_ao
    Shadows,               // Testing enable_shadows
    ModelShadows,          // Testing enable_model_shadows
    PointLights,           // Testing enable_point_lights
    LightCullRadius,       // Testing light_cull_radius (16 vs 128)
    MaxActiveLights,       // Testing max_active_lights (8 vs 256)
    Minimap,               // Testing show_minimap
    MinimapSkipDecorative, // Testing minimap.skip_decorative (ground clutter only, not leaves)
    HideGroundCover,       // Testing hide_ground_cover (skip vegetation in main view)
    Flying,                // Auto-fly streaming test (30s)
    Done,                  // All tests complete
}

/// Duration of the flying phase in seconds
pub const FLYING_PHASE_DURATION_SECS: u64 = 30;

impl AutoProfileFeature {
    pub fn next(self) -> Self {
        match self {
            Self::Baseline => Self::AO,
            Self::AO => Self::Shadows,
            Self::Shadows => Self::ModelShadows,
            Self::ModelShadows => Self::PointLights,
            Self::PointLights => Self::LightCullRadius,
            Self::LightCullRadius => Self::MaxActiveLights,
            Self::MaxActiveLights => Self::Minimap,
            Self::Minimap => Self::MinimapSkipDecorative,
            Self::MinimapSkipDecorative => Self::HideGroundCover,
            Self::HideGroundCover => Self::Flying,
            Self::Flying => Self::Done,
            Self::Done => Self::Done,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Baseline => "Baseline (all ON)",
            Self::AO => "AO",
            Self::Shadows => "Shadows",
            Self::ModelShadows => "ModelShadows",
            Self::PointLights => "PointLights",
            Self::LightCullRadius => "LightCullRadius",
            Self::MaxActiveLights => "MaxActiveLights",
            Self::Minimap => "Minimap",
            Self::MinimapSkipDecorative => "MinimapSkipDecorative",
            Self::HideGroundCover => "HideGroundCover",
            Self::Flying => "Flying (streaming)",
            Self::Done => "Done",
        }
    }

    /// Returns the duration for this phase in seconds
    pub fn duration_secs(&self) -> u64 {
        match self {
            Self::Flying => FLYING_PHASE_DURATION_SECS,
            _ => 5, // Default 5 seconds for toggle tests
        }
    }
}
