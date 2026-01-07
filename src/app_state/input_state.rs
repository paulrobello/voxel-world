use std::ops::{Deref, DerefMut};
use winit_input_helper::WinitInputHelper;

pub struct InputState {
    pub helper: WinitInputHelper,
    pub focused: bool,
    /// Deferred cursor grab change (workaround for macOS crash).
    /// true = grab and hide, false = release and show
    pub pending_grab: Option<bool>,
}

impl Deref for InputState {
    type Target = WinitInputHelper;

    fn deref(&self) -> &Self::Target {
        &self.helper
    }
}

impl DerefMut for InputState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.helper
    }
}
