/// Auto-profile feature being tested
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoProfileFeature {
    Baseline,              // Initial 5s with all features ON
    AO,                    // Testing enable_ao
    Shadows,               // Testing enable_shadows
    ModelShadows,          // Testing enable_model_shadows
    PointLights,           // Testing enable_point_lights
    Minimap,               // Testing show_minimap
    MinimapSkipDecorative, // Testing minimap.skip_decorative
    Done,                  // All tests complete
}

impl AutoProfileFeature {
    pub fn next(self) -> Self {
        match self {
            Self::Baseline => Self::AO,
            Self::AO => Self::Shadows,
            Self::Shadows => Self::ModelShadows,
            Self::ModelShadows => Self::PointLights,
            Self::PointLights => Self::Minimap,
            Self::Minimap => Self::MinimapSkipDecorative,
            Self::MinimapSkipDecorative => Self::Done,
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
            Self::Minimap => "Minimap",
            Self::MinimapSkipDecorative => "MinimapSkipDecorative",
            Self::Done => "Done",
        }
    }
}
