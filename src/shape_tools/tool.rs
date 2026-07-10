//! Uniform [`ShapeTool`] trait for placement-tool preview state structs.
//!
//! Every shape-placement tool caches a holographic preview (positions + counts)
//! and tracks an active flag with matching deactivate/clear semantics. This
//! trait captures that shared surface so dispatch / preview-gather / HUD sites
//! can iterate a registry of tools instead of re-encoding the per-struct
//! accessors. Geometry *generation* (`update_preview`) is deliberately NOT part
//! of the trait: it has three incompatible signatures (single-click target /
//! two-click start+target / selection-based) that do not compress without a
//! leaky abstraction.
//!
//! This module is purely additive — it defines the trait and implements it for
//! the preview-producing placement tools. It does not alter any existing field,
//! method body, or signature; every accessor reads an existing `pub` field and
//! every `clear_preview` delegates to (or mirrors) the struct's existing reset.

// The trait's full uniform surface (deactivate / clear_preview / set_active /
// total_blocks / preview_truncated) is the audit-named deliverable; this batch
// wires its first consumer (render.rs preview-gather exercises active() +
// preview_positions()). The remaining methods await incremental consumer wiring
// (a deactivate-all loop, HUD block-count display) — suppress dead_code module-
// wide rather than scattering ~75 per-method attrs across the impls. Mirrors the
// per-method #[allow(dead_code)] already on the inherent deactivate()s in mod.rs.
#![allow(dead_code)]

use nalgebra::Vector3;

/// Uniform interface for a shape-placement tool's preview state.
///
/// See the module docs for the rationale and the list of excluded tools.
pub trait ShapeTool {
    fn active(&self) -> bool;
    fn set_active(&mut self, active: bool);
    fn preview_positions(&self) -> &[Vector3<i32>];
    fn total_blocks(&self) -> usize;
    fn preview_truncated(&self) -> bool;
    fn clear_preview(&mut self);
    /// Deactivate and drop the cached preview. Matches existing per-struct
    /// `deactivate()` bodies (active = false + clear_preview).
    fn deactivate(&mut self) {
        self.set_active(false);
        self.clear_preview();
    }
}

// =============================================================================
// Inline states (defined in super::mod.rs)
// =============================================================================

impl ShapeTool for super::CubeToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::CubeToolState::clear_preview(self);
    }
}

impl ShapeTool for super::CylinderToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::CylinderToolState::clear_preview(self);
    }
}

impl ShapeTool for super::WallToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::WallToolState::clear_preview(self);
    }
    // Inherent deactivate() also clears start_position; the default would leave
    // it set. Override to preserve the existing full-reset semantics.
    fn deactivate(&mut self) {
        super::WallToolState::deactivate(self);
    }
}

impl ShapeTool for super::FloorToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::FloorToolState::clear_preview(self);
    }
    // Inherent deactivate() also clears start_position; override to preserve it.
    fn deactivate(&mut self) {
        super::FloorToolState::deactivate(self);
    }
}

impl ShapeTool for super::CircleToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::CircleToolState::clear_preview(self);
    }
}

impl ShapeTool for super::ConeToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::ConeToolState::clear_preview(self);
    }
}

impl ShapeTool for super::ArchToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::ArchToolState::clear_preview(self);
    }
    // Inherent deactivate() also clears start_position; override to preserve it.
    fn deactivate(&mut self) {
        super::ArchToolState::deactivate(self);
    }
}

impl ShapeTool for super::StairsToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::StairsToolState::clear_preview(self);
    }
    // Inherent deactivate() calls reset(), which also clears start_pos; override
    // to preserve the existing full-reset semantics.
    fn deactivate(&mut self) {
        super::StairsToolState::deactivate(self);
    }
}

impl ShapeTool for super::CloneToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::CloneToolState::clear_preview(self);
    }
}

// =============================================================================
// Re-exported states (defined in their geometry modules)
// =============================================================================

impl ShapeTool for super::SphereToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::SphereToolState::clear_preview(self);
    }
}

impl ShapeTool for super::TorusToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::TorusToolState::clear_preview(self);
    }
}

impl ShapeTool for super::HelixToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::HelixToolState::clear_preview(self);
    }
}

impl ShapeTool for super::PolygonToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::PolygonToolState::clear_preview(self);
    }
}

impl ShapeTool for super::BezierToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    // BezierToolState has no inherent clear_preview. Mirror the preview-only
    // reset inlined in regenerate_preview()/update_preview_with_cursor()
    // (preview_positions + total_blocks + preview_truncated). control_points
    // and control_point_markers are user input / derived and are intentionally
    // NOT dropped here — they are cleared by the full reset in clear() /
    // deactivate() below.
    fn clear_preview(&mut self) {
        self.preview.positions.clear();
        self.total_blocks = 0;
        self.preview.truncated = false;
    }
    // Inherent deactivate() calls clear(), which also drops control_points and
    // control_point_markers; the default would not. Override to preserve the
    // existing full-reset semantics.
    fn deactivate(&mut self) {
        super::BezierToolState::deactivate(self);
    }
}

impl ShapeTool for super::HollowToolState {
    fn active(&self) -> bool {
        self.active
    }
    fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    fn preview_positions(&self) -> &[Vector3<i32>] {
        &self.preview.positions
    }
    fn total_blocks(&self) -> usize {
        self.total_blocks
    }
    fn preview_truncated(&self) -> bool {
        self.preview.truncated
    }
    fn clear_preview(&mut self) {
        super::HollowToolState::clear_preview(self);
    }
    // HollowToolState has no inherent deactivate(); the trait default
    // (active = false + clear_preview) is the correct deactivation.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves the trait wiring and the default `deactivate()` body for the
    /// canonical inline state: `set_active`/`active` round-trip through the
    /// `pub` field, and `deactivate()` drains a seeded preview position.
    #[test]
    fn cube_tool_state_trait_round_trip() {
        let mut tool = super::super::CubeToolState::default();
        // Seed a preview position so clear_preview's effect is observable
        // (default() leaves preview_positions empty, which would be trivial).
        tool.preview.positions.push(Vector3::new(1, 2, 3));

        // Drive entirely through the trait surface via a trait object so every
        // call is unambiguously trait dispatch.
        let t: &mut dyn ShapeTool = &mut tool;
        t.set_active(true);
        assert!(t.active(), "active() must reflect set_active(true)");

        // Default deactivate() == set_active(false) + clear_preview(); the
        // latter delegates to CubeToolState::clear_preview, which must drain
        // the seeded position.
        t.deactivate();
        assert!(!t.active(), "deactivate() must clear the active flag");
        assert!(
            t.preview_positions().is_empty(),
            "deactivate() must drain the preview"
        );
    }
}
