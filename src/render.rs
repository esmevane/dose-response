

use color::{self, Color};
use engine::Draw;
use formula;
use game;
use graphics;
use item;
use monster;
use player::{Bonus, CauseOfDeath, Mind};
use point::{Point, SquareArea};
use rect::Rectangle;
use state::{Side, State};
use std::borrow::Cow;
use std::collections::HashMap;

use std::time::Duration;
use util;
use world::Chunk;


pub fn render_game(state: &State, dt: Duration, fps: i32, drawcalls: &mut Vec<Draw>) {
    if state.player.alive() {
        let fade = formula::mind_fade_value(state.player.mind);
        if fade > 0.0 {
            // TODO: animate the fade from the previous value?
            drawcalls.push(Draw::Fade(fade, Color { r: 0, g: 0, b: 0 }));
        }
    }

    if let Some(ref animation) = state.screen_fading {
        use animation::ScreenFadePhase;
        let fade = match animation.phase {
            ScreenFadePhase::FadeOut => animation.timer.percentage_remaining(),
            ScreenFadePhase::Wait => 0.0,
            ScreenFadePhase::FadeIn => animation.timer.percentage_elapsed(),
            ScreenFadePhase::Done => 1.0,
        };
        drawcalls.push(Draw::Fade(fade, animation.color));
    }

    let mut bonus = state.player.bonus;
    // TODO: setting this as a bonus is a hack. Pass it to all renderers
    // directly instead.
    if state.endgame_screen_visible {
        bonus = Bonus::UncoverMap;
    }
    if state.cheating {
        bonus = Bonus::UncoverMap;
    }
    let radius = formula::exploration_radius(state.player.mind);

    let player_pos = state.player.pos;
    let in_fov = |pos| player_pos.distance(pos) < (radius as f32);
    let screen_left_top_corner = state.screen_position_in_world - (state.map_size / 2);
    let display_area = Rectangle::center(state.screen_position_in_world, state.map_size / 2);
    let screen_coords_from_world = |pos| pos - screen_left_top_corner;

    let total_time_ms = util::num_milliseconds(state.clock) as i64;
    let world_size = state.world_size;

    let player_will_is_max = state.player.will.is_max();
    let player_will = *state.player.will;
    // NOTE: this is here to appease the borrow checker. If we
    // borrowed the state here as immutable, we wouln't need it.
    let show_intoxication_effect = state.player.alive() && state.player.mind.is_high();



    // NOTE: render the cells on the map. That means world geometry and items.
    for (world_pos, cell) in
        state
            .world
            .chunks(display_area)
            .flat_map(Chunk::cells)
            .filter(|&(pos, _)| display_area.contains(pos))
    {
        let display_pos = screen_coords_from_world(world_pos);

        // Render the tile
        let mut rendered_tile = cell.tile;

        if show_intoxication_effect {
            // TODO: try to move this calculation of this loop and see
            // what it does to our speed.
            let pos_x: i64 = (world_pos.x + world_size.x) as i64;
            let pos_y: i64 = (world_pos.y + world_size.y) as i64;
            assert!(pos_x >= 0);
            assert!(pos_y >= 0);
            let half_cycle_ms = 700 + ((pos_x * pos_y) % 100) * 5;
            let progress_ms = total_time_ms % half_cycle_ms;
            let forwards = (total_time_ms / half_cycle_ms) % 2 == 0;
            let progress = progress_ms as f32 / half_cycle_ms as f32;
            assert!(progress >= 0.0);
            assert!(progress <= 1.0);

            rendered_tile.fg_color = if forwards {
                graphics::fade_color(color::high, color::high_to, progress)
            } else {
                graphics::fade_color(color::high_to, color::high, progress)
            };
        }

        if in_fov(world_pos) {
            graphics::draw(drawcalls, dt, display_pos, &rendered_tile);
        } else if cell.explored || bonus == Bonus::UncoverMap {
            graphics::draw(drawcalls, dt, display_pos, &rendered_tile);
            drawcalls.push(Draw::Background(display_pos, color::dim_background));
        } else {
            // It's not visible. Do nothing.
        }

        // Render the irresistible background of a dose
        for item in cell.items.iter() {
            if item.is_dose() && !player_will_is_max {
                let resist_radius = formula::player_resist_radius(item.irresistible, player_will);
                for point in SquareArea::new(world_pos, resist_radius) {
                    if in_fov(point) {
                        let screen_coords = screen_coords_from_world(point);
                        drawcalls.push(Draw::Background(screen_coords, color::dose_background));
                    }
                }
            }
        }

        // Render the items
        if in_fov(world_pos) || cell.explored || bonus == Bonus::SeeMonstersAndItems ||
            bonus == Bonus::UncoverMap
        {
            for item in cell.items.iter() {
                graphics::draw(drawcalls, dt, display_pos, item);
            }
        }
    }

    if let Some(ref animation) = state.explosion_animation {
        drawcalls.extend(animation.tiles().map(|(world_pos, color, _)| {
            Draw::Background(screen_coords_from_world(world_pos), color)
        }));
    }

    // NOTE: render monsters
    for monster in state.world.monsters(display_area) {
        let visible = monster.position.distance(state.player.pos) < (radius as f32);
        if visible || bonus == Bonus::UncoverMap || bonus == Bonus::SeeMonstersAndItems {
            use graphics::Render;
            let display_pos = screen_coords_from_world(monster.position);
            if let Some(trail_pos) = monster.trail {
                if state.cheating {
                    let trail_pos = screen_coords_from_world(trail_pos);
                    let (glyph, color, _) = monster.render(dt);
                    // TODO: show a fading animation of the trail colour
                    let color = color::Color {
                        r: color.r.saturating_sub(55),
                        g: color.g.saturating_sub(55),
                        b: color.b.saturating_sub(55),
                    };
                    drawcalls.push(Draw::Char(trail_pos, glyph, color));
                }
            }

            if state.cheating {
                for &point in &monster.path {
                    let path_pos = screen_coords_from_world(point);
                    let (_, color, _) = monster.render(dt);
                    drawcalls.push(Draw::Background(path_pos, color));
                }
            }

            let (glyph, mut color, _) = monster.render(dt);
            if monster.kind == monster::Kind::Npc && state.player.mind.is_high() {
                color = color::npc_dim;
            }
            drawcalls.push(Draw::Char(display_pos, glyph, color))
        }
    }

    // NOTE: render the player
    {
        let display_pos = screen_coords_from_world(state.player.pos);
        graphics::draw(drawcalls, dt, display_pos, &state.player);
    }

    render_panel(
        state.map_size.x,
        state.panel_width,
        state.display_size,
        &state,
        dt,
        drawcalls,
        fps,
    );
    if state.show_keboard_movement_hints {
        render_controls_help(state.map_size, drawcalls);
    }

    if state.endgame_screen_visible {
        render_endgame_screen(state, drawcalls);
    }

    let mouse_inside_map = state.mouse.tile_pos >= (0, 0) && state.mouse.tile_pos < state.map_size;
    if mouse_inside_map && state.mouse.right {
        render_monster_info(state, drawcalls);
    }
}


fn render_endgame_screen(state: &State, drawcalls: &mut Vec<Draw>) {
    use self::CauseOfDeath::*;
    let cause_of_death = formula::cause_of_death(&state.player);
    let endgame_reason_text = if state.side == Side::Victory {
        // TODO: remove Side entirely for now.
        assert!(state.player.alive());
        assert!(cause_of_death.is_none());
        "You won!"
    } else {
        "You lost:"
    };

    let perpetrator = state.player.perpetrator.as_ref();

    let endgame_description = match (cause_of_death, perpetrator) {
        (Some(Exhausted), None) => "Exhausted".into(),
        (Some(Exhausted), Some(monster)) => format!("Exhausted because of {}", monster.glyph()),
        (Some(Overdosed), _) => "Overdosed".into(),
        (Some(LostWill), Some(monster)) => format!("Lost all Will due to {}", monster.glyph()),
        (Some(LostWill), None) => unreachable!(),
        (Some(Killed), Some(monster)) => format!("Defeated by {}", monster.glyph()),
        (Some(Killed), None) => unreachable!(),
        (None, _) => "".into(),  // Victory
    };

    let doses_in_inventory = state
        .player
        .inventory
        .iter()
        .filter(|item| item.is_dose())
        .count();

    let turns_text = format!("Turns: {}", state.turn);
    let carrying_doses_text = format!("Carrying {} doses", doses_in_inventory);
    let high_streak_text = format!(
        "Longest High streak: {} turns",
        state.player.longest_high_streak
    );
    let keyboard_text = "[N] New Game    [Q] Quit";

    let longest_text = [
        endgame_reason_text,
        &endgame_description,
        &turns_text,
        &carrying_doses_text,
        &high_streak_text,
        keyboard_text,
    ].iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap() as i32;
    let lines_count = 7;

    let rect_dimensions = Point {
        // NOTE: 1 tile padding, which is why we have the `+ 2`.
        x: longest_text + 2,
        // NOTE: each line has an empty line below so we just have `+
        // 1` for the top padding.
        y: lines_count * 2 + 1,
    };
    let rect_start = Point {
        x: (state.display_size.x - rect_dimensions.x) / 2,
        y: 7,
    };

    fn centered_text_pos(container_width: i32, text: &str) -> i32 {
        (container_width - text.chars().count() as i32) / 2
    }

    drawcalls.push(Draw::Rectangle(
        rect_start,
        rect_dimensions,
        color::background,
    ));

    drawcalls.push(Draw::Text(
        rect_start +
            (
                centered_text_pos(
                    rect_dimensions.x,
                    &endgame_reason_text,
                ),
                1,
            ),
        endgame_reason_text.into(),
        color::gui_text,
    ));

    drawcalls.push(Draw::Text(
        rect_start +
            (
                centered_text_pos(
                    rect_dimensions.x,
                    &endgame_description,
                ),
                2,
            ),
        endgame_description.into(),
        color::gui_text,
    ));

    drawcalls.push(Draw::Text(
        rect_start +
            (centered_text_pos(rect_dimensions.x, &turns_text), 5),
        turns_text.into(),
        color::gui_text,
    ));
    drawcalls.push(Draw::Text(
        rect_start +
            (
                centered_text_pos(
                    rect_dimensions.x,
                    &carrying_doses_text,
                ),
                7,
            ),
        carrying_doses_text.into(),
        color::gui_text,
    ));
    drawcalls.push(Draw::Text(
        rect_start +
            (
                centered_text_pos(rect_dimensions.x, &high_streak_text),
                9,
            ),
        high_streak_text.into(),
        color::gui_text,
    ));
    drawcalls.push(Draw::Text(
        rect_start +
            (
                centered_text_pos(rect_dimensions.x, &keyboard_text),
                13,
            ),
        keyboard_text.into(),
        color::gui_text,
    ));
}


fn render_panel(
    x: i32,
    width: i32,
    display_size: Point,
    state: &State,
    dt: Duration,
    drawcalls: &mut Vec<Draw>,
    fps: i32,
) {
    let fg = color::gui_text;
    let bg = color::dim_background;

    {
        let height = display_size.y;
        drawcalls.push(Draw::Rectangle(
            Point { x: x, y: 0 },
            Point {
                x: width,
                y: height,
            },
            bg,
        ));
    }

    let player = &state.player;

    let (mind_str, mind_val_percent) = match player.mind {
        Mind::Withdrawal(val) => ("Withdrawal", val.percent()),
        Mind::Sober(val) => ("Sober", val.percent()),
        Mind::High(val) => ("High", val.percent()),
    };

    let mut lines: Vec<Cow<'static, str>> = vec![
        mind_str.into(),
        "".into(), // NOTE: placeholder for the Mind state percentage bar
        "".into(),
        format!("Will: {}", *player.will).into(),
    ];

    if player.inventory.len() > 0 {
        lines.push("".into());
        lines.push("Inventory:".into());

        let mut item_counts = HashMap::new();
        for item in player.inventory.iter() {
            let count = item_counts.entry(item.kind).or_insert(0);
            *count += 1;
        }

        for kind in item::Kind::iter() {
            if let Some(count) = item_counts.get(&kind) {
                lines.push(
                    format!("[{}] {:?}: {}", game::inventory_key(kind), kind, count).into(),
                );
            }
        }
    }

    lines.push("".into());

    if player.will.is_max() {
        lines.push(
            format!("Sobriety: {}", player.sobriety_counter.percent()).into(),
        );
    }

    if !player.bonuses.is_empty() {
        lines.push("Bonus:".into());
        for bonus in &player.bonuses {
            lines.push(format!("* {:?}", bonus).into());
        }
    }

    if state.cheating {
        lines.push("CHEATING".into());
        lines.push("".into());
    }

    if player.alive() {
        if *player.stun > 0 {
            lines.push(format!("Stunned({})", *player.stun).into());
        }
        if *player.panic > 0 {
            lines.push(format!("Panicking({})", *player.panic).into());
        }
    } else {
        lines.push("Dead".into());
    }

    if state.cheating {
        if state.mouse.tile_pos >= (0, 0) && state.mouse.tile_pos < state.display_size {
            lines.push(format!("Mouse: {}", state.mouse.tile_pos).into())
        }

        lines.push("Time stats:".into());
        for frame_stat in state.stats.last_frames(25) {
            lines.push(
                format!(
                    "upd: {}, dc: {}",
                    util::num_milliseconds(frame_stat.update),
                    util::num_milliseconds(frame_stat.drawcalls)
                ).into(),
            );
        }
        lines.push(
            format!(
                "longest upd: {}",
                util::num_milliseconds(state.stats.longest_update())
            ).into(),
        );
        lines.push(
            format!(
                "longest dc: {}",
                util::num_milliseconds(state.stats.longest_drawcalls())
            ).into(),
        );
    }


    for (y, line) in lines.into_iter().enumerate() {
        drawcalls.push(Draw::Text(
            Point {
                x: x + 1,
                y: y as i32,
            },
            line.into(),
            fg,
        ));
    }

    let max_val = match player.mind {
        Mind::Withdrawal(val) => val.max(),
        Mind::Sober(val) => val.max(),
        Mind::High(val) => val.max(),
    };
    let mut bar_width = width - 2;
    if max_val < bar_width {
        bar_width = max_val;
    }

    graphics::progress_bar(
        drawcalls,
        mind_val_percent,
        (x + 1, 1).into(),
        bar_width,
        color::gui_progress_bar_fg,
        color::gui_progress_bar_bg,
    );

    let bottom = display_size.y - 1;

    if state.cheating {
        drawcalls.push(Draw::Text(
            Point {
                x: x + 1,
                y: bottom - 1,
            },
            format!("dt: {}ms", util::num_milliseconds(dt)).into(),
            fg,
        ));
        drawcalls.push(Draw::Text(
            Point {
                x: x + 1,
                y: bottom,
            },
            format!("FPS: {}", fps).into(),
            fg,
        ));
    }

}


fn render_monster_info(state: &State, drawcalls: &mut Vec<Draw>) {
    let screen_left_top_corner = state.screen_position_in_world - (state.map_size / 2);
    let mouse_world_pos = screen_left_top_corner + state.mouse.tile_pos;
    // TODO: world.monster_on_pos is mutable, let's add an immutable version
    let monster_area = Rectangle::from_point_and_size(
        mouse_world_pos, (1, 1).into());
    let mut debug_text = None;
    for monster in state.world.monsters(monster_area) {
        if monster.position == mouse_world_pos {
            debug_text = Some(format!("{:#?}", monster));
        }
    }
    if mouse_world_pos == state.player.pos {
        debug_text = Some(format!("{:#?}", state.player));
    }

    if let Some(debug_text) = debug_text {
        let height = debug_text.lines().count();
        let width = debug_text.lines().map(|s| s.chars().count()).max().unwrap();
        drawcalls.push(Draw::Rectangle(
            (0, 0).into(),
            (width as i32, height as i32).into(),
            color::background,
        ));
        for (index, line) in debug_text.lines().enumerate() {
            drawcalls.push(Draw::Text(
                Point {
                    x: 0,
                    y: 0 + index as i32,
                },
                line.to_string().into(),
                color::gui_text
            ));
        }
    }
}


fn render_controls_help(map_size: Point, drawcalls: &mut Vec<Draw>) {
    fn rect_dim(lines: &[&str]) -> (i32, i32) {
        (
            lines.iter().map(|l| l.len() as i32).max().unwrap(),
            lines.len() as i32,
        )
    }

    fn draw_rect(lines: &[&'static str], start: Point, w: i32, h: i32, drawcalls: &mut Vec<Draw>) {
        drawcalls.push(Draw::Rectangle(
            start,
            Point::new(w, h),
            color::dim_background,
        ));
        for (index, &line) in lines.iter().enumerate() {
            drawcalls.push(Draw::Text(
                start + Point::new(0, index as i32),
                line.into(),
                color::gui_text,
            ));
        }
    };

    let padding = 3;

    let lines = ["Up", "Num 8", "or: K"];
    let (width, height) = rect_dim(&lines);
    let start = Point {
        x: (map_size.x - width) / 2,
        y: padding,
    };
    draw_rect(&lines, start, width, height, drawcalls);

    let lines = ["Down", "Num 2", "or: J"];
    let (width, height) = rect_dim(&lines);
    let start = Point {
        x: (map_size.x - width) / 2,
        y: map_size.y - height - padding,
    };
    draw_rect(&lines, start, width, height, drawcalls);

    let lines = ["Left", "Num 4", "or: H"];
    let (width, height) = rect_dim(&lines);
    let start = Point {
        x: padding,
        y: (map_size.y - height) / 2,
    };
    draw_rect(&lines, start, width, height, drawcalls);

    let lines = ["Right", "Num 6", "or: L"];
    let (width, height) = rect_dim(&lines);
    let start = Point {
        x: map_size.x - width - padding,
        y: (map_size.y - height) / 2,
    };
    draw_rect(&lines, start, width, height, drawcalls);

    let lines = ["Shift+Right", "Num 7", "or: Y"];
    let (width, height) = rect_dim(&lines);
    let start = Point {
        x: padding,
        y: padding,
    };
    draw_rect(&lines, start, width, height, drawcalls);

    let lines = ["Shift+Right", "Num 9", "or: U"];
    let (width, height) = rect_dim(&lines);
    let start = Point {
        x: map_size.x - width - padding,
        y: padding,
    };
    draw_rect(&lines, start, width, height, drawcalls);

    let lines = ["Ctrl+Left", "Num 1", "or: N"];
    let (width, height) = rect_dim(&lines);
    let start = Point {
        x: padding,
        y: map_size.y - height - padding,
    };
    draw_rect(&lines, start, width, height, drawcalls);

    let lines = ["Ctrl+Right", "Num 3", "or: M"];
    let (width, height) = rect_dim(&lines);
    let start = Point {
        x: map_size.x - width - padding,
        y: map_size.y - height - padding,
    };
    draw_rect(&lines, start, width, height, drawcalls);
}
