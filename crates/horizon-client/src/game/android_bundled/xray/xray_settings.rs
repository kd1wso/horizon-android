// Copyright (c) 2026 Ludovic Marie and the Horizon contributors
// SPDX-License-Identifier: MIT; see LICENSE in this directory.
// Original module for the Android static bundle; not adapted from another project.
// X-Ray's settings page: marking, its radius and which ores it marks.
pub(super) fn run(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    use crate::game::official_xray as xray;
    if mod_id != xray::ID {
        return;
    }
    let game = &*game;
    let settings = xray::settings(|key| game.native_setting(xray::ID, key));
    buttons.extend(settings.labels());
}
