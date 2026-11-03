// Copyright (c) 2026 Ludovic Marie and the Horizon contributors
// SPDX-License-Identifier: MIT; see LICENSE in this directory.
// Original module for the Android static bundle; not adapted from another project.
// X-Ray's row in Options > Mods, with its key.
pub(super) fn run(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use crate::game::official_xray as xray;
    mods.push(crate::game::native_hooks::NativeMod {
        id: xray::ID.into(),
        title: format!("X-Ray {}", xray::VERSION),
        description: xray::DESCRIPTION.into(),
        keys: vec![[
            xray::KEY.into(),
            "X-Ray key".into(),
            xray::DEFAULT_KEY.into(),
        ]],
    });
}
