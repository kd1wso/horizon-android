// The mod's own code: crate::game::android_example. The helpers (the other .rs files) call it
// from the hook points; it reaches the game through the native hooks' helpers
// (crates/horizon-client/src/game/native_hooks.rs: native_setting, native_text, native_chat...).
// `pub(super)`: seen all through crate::game, where the helpers run; Game is private to it.
use crate::game::Game;
use crate::game::native_hooks::NativeMod;
use horizon_client::extras::Corner;
use horizon_client::extras_hud::Corners;
use horizon_client::hud::HudBatch;

/// The mod's id: its row in Options > Mods, its settings' folder and its chat command.
pub(super) const ID: &str = "example";
const TITLE: &str = "Example";
const VERSION: &str = "0.1.0";
const GREETING: &str = "greeting";

pub(super) fn row() -> NativeMod {
    NativeMod {
        id: ID.into(),
        title: format!("{TITLE} {VERSION}"),
        description: "A native mod, built into the APK from mods/android-native.".into(),
        keys: Vec::new(),
    }
}

fn greeting(game: &Game<'_>) -> bool {
    game.native_setting(ID, GREETING).as_deref() != Some("off")
}

pub(super) fn buttons(game: &Game<'_>) -> Vec<String> {
    let state = if greeting(game) { "ON" } else { "OFF" };
    vec![format!("Greeting: {state}")]
}

pub(super) fn click(game: &mut Game<'_>, button: usize) {
    if button == 0 {
        let value = if greeting(game) { "off" } else { "on" };
        game.native_set_setting(ID, GREETING, value);
    }
}

pub(super) fn hud(game: &mut Game<'_>, batch: &mut HudBatch, corners: &mut Corners) {
    if !game.native_on(ID) || !game.native_playing() || !greeting(game) {
        return;
    }
    let text = format!("Hello from {TITLE}");
    let width = game.native_text_width(&text) + 4.0;
    let (x, y) = corners.place(Corner::TopLeft, width, 12.0);
    game.native_fill(batch, [x, y, width, 12.0], 0x7800_0000);
    game.native_text(batch, &text, [x + 2.0, y + 2.0], 0x00FF_FF55, true);
}

/// The mod's command in chat (`/` and its id). True when it was this mod's.
pub(super) fn command(game: &mut Game<'_>, command: &str) -> bool {
    if command != ID {
        return false;
    }
    game.native_chat(&format!("{TITLE}: the Android native mod is running."));
    true
}
