//! The map screen's layout: at every UI scale, no panel of the HUD covers
//! another or leaves the screen, and the windows fit on the screen.

use crate::bot::Bot;

fn game() -> crate::game::Lucky {
    crate::new_game().unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_hud_never_overlaps_at_any_ui_scale() {
    let mut bot = Bot::new(game(), 4);
    assert!(bot.new_game(1, "normal"), "{:?}", bot.log);
    for scale in [0.9, 0.95, 1.0, 1.05, 1.1, 1.15] {
        bot.sim.game_mut().settings.ui_scale = scale;
        let rects = bot.g().hud_layout(true);
        let screen = duckforge::prelude::Rect::new(0.0, 0.0, crate::ui::W, crate::ui::H);
        for (name, r) in &rects {
            assert!(r.w > 0.0 && r.h > 0.0, "{name} at {scale}: empty");
            assert!(r.x >= -0.5 && r.y >= -0.5 && r.right() <= screen.w + 0.5 && r.bottom() <= screen.h + 0.5, "{name} at UI scale {scale} leaves the screen: {r:?}");
        }
        for (i, (a, ra)) in rects.iter().enumerate() {
            for (b, rb) in &rects[i + 1..] {
                // The top bar is under everything at its edge: the others start below it.
                let overlap = ra.x < rb.right() - 0.5 && rb.x < ra.right() - 0.5 && ra.y < rb.bottom() - 0.5 && rb.y < ra.bottom() - 0.5;
                assert!(!overlap, "at UI scale {scale}, {a} {ra:?} overlaps {b} {rb:?}");
            }
        }
        // The windows (and the pause menu) fit on the screen, scaled about the middle.
        for r in [crate::game::windows_rect()] {
            let s = scale;
            let c = duckforge::prelude::vec2(crate::ui::W / 2.0, crate::ui::H / 2.0);
            let (x0, y0) = (c.x + (r.x - c.x) * s, c.y + (r.y - c.y) * s);
            let (x1, y1) = (c.x + (r.right() - c.x) * s, c.y + (r.bottom() - c.y) * s);
            assert!(x0 >= 0.0 && y0 >= 0.0 && x1 <= crate::ui::W && y1 <= crate::ui::H, "a window at UI scale {scale} leaves the screen");
        }
    }
}
