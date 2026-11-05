// Draws over the HUD each frame: the hint while the mod is armed.
fn hud(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    crate::game::android_veinminer::hud(game, batch, corners);
}
