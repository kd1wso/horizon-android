// Copyright (c) 2026 Ludovic Marie and the Horizon contributors
// SPDX-License-Identifier: MIT; see LICENSE in this directory.
// Original module for the Android static bundle; not adapted from another project.
// The markers' geometry: translucent cubes in the see-through batch, so ores
// are visible through the terrain. The camera-facing faces are given in both
// windings because the pipeline culls back faces and one of them survives.
use horizon_client::{hud::HudBatch, mesher::Vertex};

/// The opacity of the fill (0x38, 22%): a vein's overlapping cubes stay
/// readable, and the terrain shows through them.
const FILL: u32 = 0x3800_0000;

pub(super) fn run(
    game: &crate::game::Game<'_>,
    camera: glam::DVec3,
    see_through: &mut Vec<Vertex>,
) {
    use crate::game::official_xray as xray;
    // A 1x1 batch's quad is the atlas's white pixel: the UV a filled shape
    // takes, as the game's own gizmos do.
    let mut fill = HudBatch::new([1, 1]);
    fill.fill(&game.gui.atlas, 0.0, 0.0, 1.0, 1.0, 0xFFFF_FFFF);
    let Some(white) = fill.vertices.first().copied() else {
        return;
    };
    let mut quads = Vec::new();
    xray::with_markers(|markers| {
        // The nearest markers are kept; drawing them last lets the farther
        // ones blend first, as translucent geometry reads best.
        for marker in markers.iter().take(xray::DRAWN).rev() {
            quads.clear();
            xray::cube_quads(marker.pos, camera.to_array(), &mut quads);
            let color = (marker.color & 0x00FF_FFFF) | FILL;
            for quad in &quads {
                for corner in quad {
                    see_through.push(Vertex {
                        pos: *corner,
                        uv: white.uv,
                        layer: 0,
                        color,
                        // Full bright, as the game's own always-on-top fills.
                        light: 0xF0F0,
                        normal: [0; 4],
                        glint_uv: [0.0, 0.0],
                    });
                }
            }
        }
    });
}
