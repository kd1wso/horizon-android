// A click on the mod's settings page.
fn click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if mod_id == crate::game::android_veinminer::ID {
        crate::game::android_veinminer::click(game, button);
    }
}
