// The mod's settings page: its buttons' labels.
fn settings(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    if mod_id == crate::game::android_example::ID {
        buttons.extend(crate::game::android_example::buttons(game));
    }
}
