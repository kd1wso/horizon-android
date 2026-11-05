// A click on the mod's settings page.
fn click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    if mod_id == crate::game::android_example::ID {
        crate::game::android_example::click(game, button);
    }
}
