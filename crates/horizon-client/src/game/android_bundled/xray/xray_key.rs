// Copyright (c) 2026 Ludovic Marie and the Horizon contributors
// SPDX-License-Identifier: MIT; see LICENSE in this directory.
// Original module for the Android static bundle; not adapted from another project.
// The X-Ray key went down: switch marking on or off and keep it.
pub(super) fn run(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    use crate::game::official_xray as xray;
    // Turned off in Options > Mods: nothing until the next build leaves it out.
    if !pressed || !game.native_on(xray::ID) || !game.native_playing() {
        return;
    }
    if key != game.native_mod_key(xray::ID, xray::KEY, xray::DEFAULT_KEY) {
        return;
    }
    let settings = xray::toggle(|name| game.native_setting(xray::ID, name));
    for (name, value) in settings.stored() {
        game.native_set_setting(xray::ID, name, &value);
    }
}
