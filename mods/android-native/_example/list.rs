// The mod's row in Options > Mods.
fn list(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    mods.push(crate::game::android_example::row());
}
