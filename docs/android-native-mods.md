# Android native mods

Native mods change the game's own Rust code: they are horizon-mixins bundles, the
`.hnmod` files of the desktop launcher. A phone can neither compile nor load native
code, so on Android they are built into the APK. Put them in `mods/android-native/`
and build the APK; `tools/android_build.py` (also in CI) writes them into the source
for Cargo and takes them out again afterwards.

## Make a mod

1. `python3 tools/android_native_mods.py new <id>` makes `mods/android-native/<id>/`
   from the template, `_example/`, with your id (lower-case letters, digits, `_`).
   The template is a working mod: a row in **Options → Mods**, a settings page with
   one button, a line on the HUD and the chat command `/<id>`. Folders whose name
   starts with `_` or `.` are not built, so the template itself is not.
2. Write the mod there, then check it: `python3 tools/android_native_mods.py check`.
3. Commit and push to `android`. **Android dev APKs** builds the APK with the mod
   (see [development APKs](android-dev-builds.md) for the one-time signing setup);
   **Android source checks** runs the check. Locally, any `android_build.py` build
   includes it.
4. Install the APK. The mod has a row in **Options → Mods**, where it is turned on
   and off.

A packed `.hnmod` (from the registry, a GitHub release, the desktop launcher, or the
one imported on the phone into `horizon-native-mods/`) works as it is: put the file
in `mods/android-native/`.

## The format

It is horizon-mixins' format: `horizon-mixin.toml` has `[mod]` (`id`, `version`,
`priority`), `[[module]]`s and `[[injection]]`s. An injection's `helper` is a `.rs`
file holding one private function; its parameters are the `bindings`, in order,
with the binding's `ty`; `value` is a parameter of the target, a borrow (`&x`,
`&mut x`) or a reborrow (`&*x`, `&mut *self`). Android supports:

- `HEAD`: the helper runs first in the target and returns nothing. With
  `cancellable = true` it returns `ControlFlow<R>` (`R` is the target's result);
  `Break(value)` returns `value` from the target, `Continue(())` goes on.
- `OVERWRITE`: the helper is the target's new body and returns its result. It
  cannot share the target with other injections.
- `hook` with `hook_version`: a hook point of `native-hooks.json`, the stable entry
  points of `crates/horizon-client/src/game/native_hooks.rs` (`tick`, `hud`, `key`,
  `wheel`, `command`, `mods`, `settings`...). Or `target`: the path of any function
  with a body, such as
  `horizon_client::game::native_hooks::Game<'_>::native_wheel_event`.
- `[[module]]`: one of the mod's files becomes `mod <name>` at the end of a
  workspace `file` (`pub(crate)` unless `visibility = "pub"`): the mod's own code.
- `data/`: the mod's data files, read with `horizon_client::native_data!("<id>",
  "<file>")`.

Injections on one function run by `priority` (highest first), then by mod and
injection id. `BEFORE_CALL`, `RETURN`, the `MODIFY_*` kinds, `WRAP_OPERATION`,
`AFTER_*`, selectors, `$` bindings, `[[bridge]]`, `[[state]]`, `[[crate]]` and
`[[post_build]]` are not supported on Android yet; `check` says so, and says what
else is wrong (an unknown hook with the closest names, a helper that does not fit
its bindings, a module name already taken, an id of the app's own Android mods:
`minimap`, `recipes`, `smartf3`, `xray`, `zoom`).

## Writing the code

Hook points' helpers run in `crate::game`, so they reach the game as
`crate::game::...`. The template keeps its code in its module and calls it from
the helpers; the module reaches the game through the hook points' helpers
(`native_on`, `native_setting`, `native_set_setting`, `native_playing`,
`native_text`, `native_fill`, `native_chat`... in `native_hooks.rs`). `Game` is
private to `crate::game`: functions that take it are `pub(super)`.

A mod lists itself with the `mods` hook (`NativeMod`), and checks
`native_on(<id>)` before it acts: that is its switch in **Options → Mods**.

## How a build applies them

`tools/android_build.py` runs `tools/android_native_mods.py`:

1. If an interrupted build left mods in the source, it restores the source.
2. It checks every mod against this source.
3. It writes them in: each injection becomes a block at the start of the target's
   body (or the whole body for `OVERWRITE`) between `// >>> android-native-mods`
   and `// <<< android-native-mods`; modules become `#[path]` declarations; data
   goes to `native-data/<id>/`, a `.hnmod`'s files to
   `.local/android-native-mods/bundles/<id>/`. The originals are kept in
   `.local/android-native-mods/`.
4. Cargo builds; then the original source is put back, also when the build fails.

`--no-native-mods` builds without them. Run one build at a time in a checkout: the
mods are in its source while Cargo runs. By hand:

```sh
python3 tools/android_native_mods.py check   # validate only; writes nothing
python3 tools/android_native_mods.py apply   # write them in (to read the code, or for cargo)
python3 tools/android_native_mods.py clean   # put the original source back
```

`apply` then `cargo check -p horizon-client --no-default-features --bin horizon-client`
type-checks a mod on the computer without the NDK; run `clean` afterwards. A file edited while
the mods are applied is never overwritten: `apply` and `clean` stop and name it.

## Limits

- A mod is part of the APK: changing it means building and installing a new one.
  An installed APK cannot load native mods; `.hnmod`s imported on the phone are
  only kept.
- The code must build for Android, for every ABI built.
- Mods are built against this source, whatever `horizon_revision` they name: hook
  points (`hook`) stay put from one revision to the next, `target`s may not.
- A native mod can do anything the game can: build only mods you trust.
