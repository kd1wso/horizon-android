// A command typed in chat, before the server sees it: `ran` keeps it from going further.
fn on_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    if !*ran
        && game.native_on(crate::game::android_example::ID)
        && crate::game::android_example::command(game, command)
    {
        *ran = true;
    }
}
