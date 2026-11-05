# Android built-in shader packs

Shader packs built into the Android APK. Each folder here that holds a `shaders/` folder (the
files of a classic Iris/OptiFine pack's ZIP) goes into the APK as `<folder>.zip`, and each
`.zip` here goes in as it is; names starting with `_` or `.` are skipped. On the phone the
launcher puts them in the profile's `shaderpacks/`: **Installed** lists them (marked
"built in") beside the game's Shader Packs page, and they are enabled there like any other
pack. Each version is put there once: a pack the player deletes stays deleted until an APK
brings a new version of it, and a file of the same name that the player put there is never
replaced.

`python3 tools/android_shader_packs.py check` lists what the next APK carries, and
`tools/android_build.py` writes the ZIPs, locally and in CI. See
[docs/android-content.md](../../docs/android-content.md#built-in-shader-packs).

| Pack | From | Licence |
|---|---|---|
| [Lux_v1.2_Horizon_Android](Lux_v1.2_Horizon_Android/ANDROID.txt) | [GBanan/Lux-1.2-Shaders-For-Horizon](https://github.com/GBanan/Lux-1.2-Shaders-For-Horizon), with defaults for phones | GPL-2.0 (`LICENSE`) and BSL's terms (`AGREEMENT.txt`) |
