// Copyright (c) 2026 Ludovic Marie and the Horizon contributors
// SPDX-License-Identifier: MIT; see LICENSE in this directory.
// Original module for the Android static bundle; not adapted from another project.
// A frame of the sweep: a bounded slice of the blocks around the player, so
// the highlight follows them without a stall when it is first turned on.
pub(super) fn run(game: &mut crate::game::Game<'_>, _elapsed: std::time::Duration) {
    use crate::game::official_xray as xray;
    // Open screens keep marking: the world is drawn behind them, and a sweep
    // would restart on every inventory visit otherwise.
    if !game.native_on(xray::ID) {
        xray::clear();
        return;
    }
    let settings = xray::settings(|key| game.native_setting(xray::ID, key));
    let Some(player) = game.native_player() else {
        xray::clear();
        return;
    };
    let center = [
        player.x.floor() as i32,
        player.y.floor() as i32,
        player.z.floor() as i32,
    ];
    // Names are `Arc<str>` in the block table: comparing them allocates
    // nothing, which a few thousand reads a frame needs.
    let facts = game.res.facts();
    xray::scan(center, player.dimension, settings, |pos| {
        let state = game.world.block(pos[0], pos[1], pos[2]);
        xray::ore_color(&facts.get(state).name, settings.rare_only)
    });
}
