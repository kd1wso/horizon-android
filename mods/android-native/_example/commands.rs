// The commands this mod owns, so the chat box completes and colours them.
fn own_commands(commands: &mut Vec<&'static str>) {
    commands.push(crate::game::android_example::ID);
}
