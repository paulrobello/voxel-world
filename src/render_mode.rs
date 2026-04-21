/// Render modes for debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum RenderMode {
    Coord = 0,
    Steps = 1,
    #[default]
    Textured = 2,
    Normal = 3,
    UV = 4,
    Depth = 5,
    BrickDebug = 6,
    ShadowDebug = 7,
}

impl RenderMode {
    pub const ALL: &'static [RenderMode] = &[
        RenderMode::Coord,
        RenderMode::Steps,
        RenderMode::Textured,
        RenderMode::Normal,
        RenderMode::UV,
        RenderMode::Depth,
        RenderMode::BrickDebug,
        RenderMode::ShadowDebug,
    ];
}
