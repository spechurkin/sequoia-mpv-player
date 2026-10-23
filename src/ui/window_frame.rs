use eframe::egui::{self, pos2, CursorIcon, Rect, Sense};
use egui::viewport::ResizeDirection;

const EDGE: f32 = 6.;
const CORNER: f32 = 16.;
const CAPTION_WIDTH: f32 = 46.;
const CAPTION_HEIGHT: f32 = 32.;

pub fn caption_buttons(ui: &mut egui::Ui, close_tooltip: &str) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.;
        let widgets = &mut ui.visuals_mut().widgets;
        for visuals in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
        ] {
            visuals.expansion = 0.;
        }
        let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        for (icon, tooltip, command) in [
            (
                super::Icon::Close,
                close_tooltip,
                egui::ViewportCommand::Close,
            ),
            (
                if maximized {
                    super::Icon::Restore
                } else {
                    super::Icon::Maximize
                },
                sequoia::i18n::t("text.maximize_restore"),
                egui::ViewportCommand::Maximized(!maximized),
            ),
            (
                super::Icon::Minimize,
                sequoia::i18n::t("text.minimize"),
                egui::ViewportCommand::Minimized(true),
            ),
        ] {
            let pixels = (18. * ui.ctx().pixels_per_point()).ceil() as u32;
            let texture = super::icons::texture(ui.ctx(), icon, pixels);
            let image = egui::Image::new(&texture)
                .fit_to_exact_size(egui::Vec2::splat(18.))
                .tint(super::INK);
            if ui
                .add(
                    egui::Button::new(image)
                        .frame_when_inactive(false)
                        .corner_radius(0)
                        .min_size(egui::vec2(CAPTION_WIDTH, CAPTION_HEIGHT)),
                )
                .on_hover_text(tooltip)
                .clicked()
            {
                ui.ctx().send_viewport_cmd(command);
            }
        }
    })
        .response
}

fn resize_regions(rect: Rect) -> Vec<(Rect, ResizeDirection, CursorIcon)> {
    let (l, r, t, b) = (rect.left(), rect.right(), rect.top(), rect.bottom());
    let region = |x1, y1, x2, y2| Rect::from_min_max(pos2(x1, y1), pos2(x2, y2));
    [
        (
            region(l, t, l + CORNER, t + EDGE),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNwSe,
        ),
        (
            region(r - CORNER, t, r, t + EDGE),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNeSw,
        ),
        (
            region(l, b - EDGE, l + CORNER, b),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeNeSw,
        ),
        (
            region(r - CORNER, b - EDGE, r, b),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeNwSe,
        ),
        (
            region(l + CORNER, t, r - CORNER, t + EDGE),
            ResizeDirection::North,
            CursorIcon::ResizeVertical,
        ),
        (
            region(l + CORNER, b - EDGE, r - CORNER, b),
            ResizeDirection::South,
            CursorIcon::ResizeVertical,
        ),
        (
            region(l, t + CORNER, l + EDGE, b - CORNER),
            ResizeDirection::West,
            CursorIcon::ResizeHorizontal,
        ),
        (
            region(r - EDGE, t + CORNER, r, b - CORNER),
            ResizeDirection::East,
            CursorIcon::ResizeHorizontal,
        ),
        (
            region(l, t + EDGE, l + EDGE, t + CORNER),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNwSe,
        ),
        (
            region(r - EDGE, t + EDGE, r, t + CORNER),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNeSw,
        ),
        (
            region(l, b - CORNER, l + EDGE, b - EDGE),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeNeSw,
        ),
        (
            region(r - EDGE, b - CORNER, r, b - EDGE),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeNwSe,
        ),
    ]
        .into_iter()
        .filter_map(|(mut region, direction, cursor)| {
            let caption =
                Rect::from_min_max(pos2(r - 3. * CAPTION_WIDTH, t), pos2(r, t + CAPTION_HEIGHT));
            // Caption buttons keep their full hit area, including the screen corner.
            if region.intersects(caption) {
                if region.left() >= caption.left() {
                    region.min.y = region.min.y.max(caption.bottom());
                } else {
                    region.max.x = region.max.x.min(caption.left());
                }
            }
            region.is_positive().then_some((region, direction, cursor))
        })
        .collect()
}

pub fn resize_handles(ctx: &egui::Context) {
    if ctx.input(|i| {
        let viewport = i.viewport();
        viewport.fullscreen.unwrap_or(false) || viewport.maximized.unwrap_or(false)
    }) {
        return;
    }
    for (index, (rect, direction, cursor)) in
        resize_regions(ctx.screen_rect()).into_iter().enumerate()
    {
        egui::Area::new(egui::Id::new(("window-resize", index)))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .constrain(false)
            .movable(false)
            .show(ctx, |ui| {
                let (_, response) = ui.allocate_exact_size(rect.size(), Sense::drag());
                let response = response.on_hover_cursor(cursor);
                // Start the native sizing loop on press, before egui's drag threshold.
                if response.is_pointer_button_down_on()
                    && ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_buttons_reach_window_edges_and_receive_corner_clicks() {
        for size in [[960., 580.], [1100., 720.], [1440., 900.]] {
            for maximized in [false, true] {
                let ctx = egui::Context::default();
                let screen = Rect::from_min_size(pos2(0., 0.), size.into());
                let mut input = egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                };
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .maximized = Some(maximized);
                let mut draw = |ctx: &egui::Context| {
                    egui::TopBottomPanel::top("title")
                        .frame(egui::Frame::new().inner_margin(egui::Margin {
                            left: 8,
                            ..Default::default()
                        }))
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Видео");
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let response = caption_buttons(ui, "Закрыть");
                                        assert_eq!(response.rect.right_top(), screen.right_top());
                                        assert_eq!(
                                            response.rect.size(),
                                            egui::vec2(3. * CAPTION_WIDTH, CAPTION_HEIGHT)
                                        );
                                    },
                                );
                            });
                        });
                    resize_handles(ctx);
                };
                for _ in 0..3 {
                    let _ = ctx.run(input.clone(), &mut draw);
                }
                let corner = screen.right_top() + egui::vec2(-0.5, 0.5);
                input.events = vec![
                    egui::Event::PointerMoved(corner),
                    egui::Event::PointerButton {
                        pos: corner,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ];
                let output = ctx.run(input.clone(), &mut draw);
                assert!(
                    !output.viewport_output[&egui::ViewportId::ROOT]
                        .commands
                        .iter()
                        .any(|command| matches!(command, egui::ViewportCommand::BeginResize(_)))
                );
                input.events = vec![egui::Event::PointerButton {
                    pos: corner,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }];
                let output = ctx.run(input, draw);
                assert!(
                    output.viewport_output[&egui::ViewportId::ROOT]
                        .commands
                        .iter()
                        .any(|command| matches!(command, egui::ViewportCommand::Close))
                );
            }
        }
    }

    #[test]
    fn handles_follow_window_size_without_covering_content() {
        let ctx = egui::Context::default();
        for size in [[1100., 720.], [1440., 900.], [960., 580.], [1100., 720.]] {
            let screen = Rect::from_min_size(pos2(0., 0.), size.into());
            let mut input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            for _ in 0..3 {
                let _ = ctx.run(input.clone(), resize_handles);
            }
            for (index, (rect, _, _)) in resize_regions(screen).into_iter().enumerate() {
                let area =
                    egui::AreaState::load(&ctx, egui::Id::new(("window-resize", index))).unwrap();
                assert_eq!(area.rect(), rect);
            }
            input.events = vec![
                egui::Event::PointerMoved(screen.center()),
                egui::Event::PointerButton {
                    pos: screen.center(),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ];
            let output = ctx.run(input, resize_handles);
            assert!(
                !output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::BeginResize(_)))
            );
            let mut release = egui::RawInput::default();
            release.events.push(egui::Event::PointerButton {
                pos: screen.center(),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            });
            let _ = ctx.run(release, resize_handles);
        }
    }

    #[test]
    fn every_edge_starts_native_resize_and_maximized_windows_ignore_it() {
        let screen = Rect::from_min_size(pos2(0., 0.), egui::vec2(1100., 720.));
        for (rect, direction, cursor) in resize_regions(screen) {
            let ctx = egui::Context::default();
            let mut input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            for _ in 0..3 {
                let _ = ctx.run(input.clone(), resize_handles);
            }
            input.events = vec![egui::Event::PointerMoved(rect.center())];
            let output = ctx.run(input.clone(), resize_handles);
            assert_eq!(output.platform_output.cursor_icon, cursor);
            input.events.push(egui::Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            });
            let output = ctx.run(input.clone(), resize_handles);
            assert!(output.viewport_output[&egui::ViewportId::ROOT].commands.iter().any(
                |command| matches!(command, egui::ViewportCommand::BeginResize(actual) if *actual == direction)
            ), "missing resize for {direction:?}");

            for fullscreen in [false, true] {
                let ctx = egui::Context::default();
                let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
                viewport.maximized = Some(!fullscreen);
                viewport.fullscreen = Some(fullscreen);
                let output = ctx.run(input.clone(), resize_handles);
                assert!(
                    !output.viewport_output[&egui::ViewportId::ROOT]
                        .commands
                        .iter()
                        .any(|command| matches!(command, egui::ViewportCommand::BeginResize(_)))
                );
            }
        }
    }
}
