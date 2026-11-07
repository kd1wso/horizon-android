# JEI for Horizon on Android

This is the `jei-0.1.0.hnmod` source bundle supplied for this fork, built into
Android APKs with `tools/android_native_mods.py`. No `.hnmod` compilation on the
phone is needed. Its search tokenizer credits Just Enough Items by mezz; the
upstream MIT notice is in `THIRD_PARTY_NOTICES.md`.

Enable JEI on the launcher's **Mods** page or in **Options > Mods**. With a world
open, the list appears beside supported inventory/container screens when there
is room. Tap an item to view recipes; the search field filters items. JEI uses
R/U for recipes/uses when a keyboard is present; the mod's options also allow
creative-mode item giving and English/Spanish text.

Android's built-in **Recipes** mod shares the same panel. If both are enabled,
Recipes has priority; disable Recipes to show JEI. This Android adaptation
also clears JEI's panel and viewer when it is disabled or the world closes.
Device touch layout and behaviour must be tested on an actual phone.
