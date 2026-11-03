// Copyright (c) 2026 Ludovic Marie and the Horizon contributors
// SPDX-License-Identifier: MIT; see LICENSE in this directory.
// Original module for the Android static bundle; not adapted from another project.
// A click on X-Ray's settings page: change the setting and keep it.
pub(super) fn run(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    use crate::game::official_xray as xray;
    if mod_id != xray::ID {
        return;
    }
    let mut settings = xray::settings(|key| game.native_setting(xray::ID, key));
    settings.click(button);
    xray::set_settings(settings);
    for (key, value) in settings.stored() {
        game.native_set_setting(xray::ID, key, &value);
    }
}
