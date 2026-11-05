// A player breaks a block, once the game allowed it: the rest of its vein breaks first.
// A break another mod canceled (they run first) takes no vein.
fn destroy_block(game: &mut crate::game::Game<'_>, pos: [i32; 3], canceled: &bool) {
    if !*canceled {
        crate::game::android_veinminer::destroy_block(game, pos);
    }
}
