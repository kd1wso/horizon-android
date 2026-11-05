# Android native mods

Native mods built into the Android APK: folders with a `horizon-mixin.toml`, or
`.hnmod` files. Folders starting with `_` are not built; `_example/` is the
template. `python3 tools/android_native_mods.py new <id>` starts a mod from it,
`python3 tools/android_native_mods.py check` validates them, and the next APK has
them. See [docs/android-native-mods.md](../../docs/android-native-mods.md).
