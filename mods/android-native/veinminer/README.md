# VeinMiner (Android native mod)

Sneak and break an ore or a log with the right tool: the blocks of the same kind joined
to it (by a face, an edge or a corner) break with it, the ore vein or the tree's trunk.

- **Ores:** the vanilla ore tags (coal, copper, iron, gold, redstone, lapis, diamond,
  emerald), so stone and deepslate ores go together; any other `*_ore` and ancient
  debris take the same block only. **Logs:** the `minecraft:logs` tag, the same log only.
- Only with a tool that gets the block's drops (an iron pickaxe for diamond ore, an axe
  for a log). Each block breaks as the player's own break: Fortune and Silk Touch,
  the tool's wear, experience, hunger, statistics, world protections. The drops and
  the experience come out of the block you broke.
- It stops before the tool would break, and does nothing in creative.
- Server-side: single player and a world opened to LAN (the guests too, with their own
  sneaking); not on someone else's server.

It is on by default: **Disable** / **Enable** on the launcher's **Mods** page or in
**Options → Mods**. Settings (**Options → Mods → VeinMiner**): mode (sneak / always), max blocks
(16–256, 64 by default, the broken block included), ores, logs, the HUD hint.
`/veinminer` in chat says what it does now.

Files: `runtime.rs` is the code (`crate::game::android_veinminer`); `destroy.rs` hooks
`destroy_block`; the other `.rs` files are the Mods row (the launcher's and Options → Mods), the settings page,
the HUD hint and the chat command (see `docs/android-native-mods.md`).
