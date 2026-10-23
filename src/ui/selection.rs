use super::*;
use std::collections::BTreeSet;

#[derive(Clone)]
struct Marquee {
    start: egui::Pos2,
    base: BTreeSet<usize>,
    dragging: bool,
}

#[derive(Clone)]
pub(super) struct QueueDrag {
    pub rows: Vec<usize>,
}

pub(super) fn queue_drag(
    ui: &egui::Ui,
    response: &egui::Response,
    index: usize,
    selected: &mut BTreeSet<usize>,
) -> Option<(usize, Vec<usize>)> {
    if response.drag_started() {
        if !selected.contains(&index) {
            selected.clear();
            selected.insert(index);
        }
        egui::DragAndDrop::set_payload(ui.ctx(), QueueDrag {
            rows: selected.iter().copied().collect(),
        });
    }
    let after = ui.input(|i| i.pointer.hover_pos().is_some_and(|p| p.y > response.rect.center().y));
    let before = index + usize::from(after);
    if response.dnd_hover_payload::<QueueDrag>().is_some() {
        let y = if after { response.rect.bottom() } else { response.rect.top() };
        ui.painter().hline(response.rect.x_range(), y, Stroke::new(2., ACCENT));
    }
    response.dnd_release_payload::<QueueDrag>().map(|payload| (before, payload.rows.clone()))
}

pub(super) fn click(rows: &mut BTreeSet<usize>, index: usize, mods: egui::Modifiers) {
    if mods.shift {
        let anchor = rows.first().copied().unwrap_or(index);
        rows.extend(anchor.min(index)..=anchor.max(index));
    } else if mods.ctrl || mods.command {
        if !rows.remove(&index) {
            rows.insert(index);
        }
    } else {
        rows.clear();
        rows.insert(index);
    }
}

pub(super) fn marquee(
    ui: &egui::Ui,
    salt: impl std::hash::Hash,
    viewport: egui::Rect,
    rects: &[(usize, egui::Rect)],
    selected: &mut BTreeSet<usize>,
    blank_only: bool,
) {
    let id = ui.id().with(salt);
    let mut state = ui.ctx().data_mut(|d| d.get_temp::<Marquee>(id));
    let (pressed, down, released, pos, mods) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.pointer.interact_pos(),
            i.modifiers,
        )
    });
    let origin = rects.first().map_or(viewport.min, |(_, rect)| rect.min);
    if pressed {
        state = None;
        if let Some(pos) = pos.filter(|p| viewport.contains(*p)) {
            let uncovered = ui.ctx().layer_id_at(pos) == Some(ui.layer_id());
            let on_row = rects.iter().any(|(_, rect)| rect.contains(pos));
            if uncovered && (!blank_only || !on_row) {
                let base = if mods.ctrl || mods.command || mods.shift {
                    selected.clone()
                } else {
                    BTreeSet::new()
                };
                if !on_row {
                    *selected = base.clone();
                }
                state = Some(Marquee {
                    start: (pos - origin.to_vec2()),
                    base,
                    dragging: false,
                });
            }
        }
    }
    if let (Some(drag), Some(pos)) = (&mut state, pos) {
        let start = drag.start + origin.to_vec2();
        drag.dragging |= start.distance(pos) > 4.;
        if drag.dragging && (down || released) {
            let rect = egui::Rect::from_two_pos(start, viewport.clamp(pos));
            *selected = drag.base.clone();
            selected.extend(
                rects
                    .iter()
                    .filter(|(_, row)| rect.intersects(*row))
                    .map(|(i, _)| *i),
            );
            let painter = ui.painter().with_clip_rect(viewport);
            painter.rect_filled(rect, 0., ACCENT.gamma_multiply(0.15));
            painter.rect_stroke(rect, 0., Stroke::new(1., ACCENT), egui::StrokeKind::Inside);
            ui.ctx().request_repaint();
        }
    }
    if !down {
        state = None;
    }
    ui.ctx().data_mut(|d| {
        if let Some(state) = state {
            d.insert_temp(id, state);
        } else {
            d.remove::<Marquee>(id);
        }
    });
}

impl Player {
    pub(super) fn remove_queue_rows(&mut self) {
        if self
            .queue
            .index
            .is_some_and(|i| self.selected_queue.contains(&i))
        {
            self.stop();
        }
        for i in self.selected_queue.iter().rev().copied() {
            self.queue.remove(i);
        }
        self.selected_queue.clear();
        if self.music_target == sequoia::music::Target::Queue {
            self.music_rows.clear();
        }
        self.sync_music_queue();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        selected: &mut BTreeSet<usize>,
        events: Vec<egui::Event>,
        mods: egui::Modifiers,
        blank_only: bool,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::splat(400.),
                )),
                events,
                modifiers: mods,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let rects: Vec<_> = (0..5)
                        .map(|i| {
                            (
                                i,
                                egui::Rect::from_min_size(
                                    egui::pos2(30., 30. + i as f32 * 28.),
                                    Vec2::new(300., 24.),
                                ),
                            )
                        })
                        .collect();
                    marquee(ui, "test", ui.max_rect(), &rects, selected, blank_only);
                });
            },
        );
    }

    fn press(pos: egui::Pos2, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            },
        ]
    }

    #[test]
    fn music_marquee_selects_intersections_and_shrinks_on_reverse_drag() {
        let ctx = egui::Context::default();
        let mut rows = [4].into_iter().collect();
        let mods = egui::Modifiers::NONE;
        frame(&ctx, &mut rows, vec![], mods, true);
        frame(
            &ctx,
            &mut rows,
            press(egui::pos2(15., 25.), mods),
            mods,
            true,
        );
        frame(
            &ctx,
            &mut rows,
            vec![egui::Event::PointerMoved(egui::pos2(200., 105.))],
            mods,
            true,
        );
        assert_eq!(rows, [0, 1, 2].into_iter().collect());
        frame(
            &ctx,
            &mut rows,
            vec![egui::Event::PointerMoved(egui::pos2(200., 60.))],
            mods,
            true,
        );
        assert_eq!(rows, [0, 1].into_iter().collect());
    }

    #[test]
    fn music_row_drag_is_reserved_for_reordering() {
        let ctx = egui::Context::default();
        let mut rows = [4].into_iter().collect();
        let mods = egui::Modifiers::NONE;
        frame(&ctx, &mut rows, vec![], mods, true);
        frame(
            &ctx,
            &mut rows,
            press(egui::pos2(50., 35.), mods),
            mods,
            true,
        );
        frame(
            &ctx,
            &mut rows,
            vec![egui::Event::PointerMoved(egui::pos2(200., 105.))],
            mods,
            true,
        );
        assert_eq!(rows, [4].into_iter().collect());
    }

    #[test]
    fn blank_drag_adds_to_selection_with_ctrl_and_keeps_it_on_release() {
        let ctx = egui::Context::default();
        let mut rows = [4].into_iter().collect();
        let mods = egui::Modifiers::CTRL;
        frame(&ctx, &mut rows, vec![], mods, true);
        frame(
            &ctx,
            &mut rows,
            press(egui::pos2(15., 25.), mods),
            mods,
            true,
        );
        let pos = egui::pos2(200., 105.);
        frame(
            &ctx,
            &mut rows,
            vec![egui::Event::PointerMoved(pos)],
            mods,
            true,
        );
        assert_eq!(rows, [0, 1, 2, 4].into_iter().collect());
        frame(
            &ctx,
            &mut rows,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: mods,
            }],
            mods,
            true,
        );
        assert_eq!(rows, [0, 1, 2, 4].into_iter().collect());
    }

    #[test]
    fn both_queues_pointer_drag_reorders_single_and_multiple_rows_and_preserves_current() {
        for music_mode in [false, true] {
            for (initial, start, end, expected, selection) in [
                (vec![0], 0, Some((3, true)), vec![1, 2, 3, 0, 4], vec![3]),
                (vec![1, 3], 3, Some((0, false)), vec![1, 3, 0, 2, 4], vec![0, 1]),
                (vec![0, 2], 0, None, vec![1, 3, 4, 0, 2], vec![3, 4]),
                (vec![4], 1, Some((3, false)), vec![0, 2, 1, 3, 4], vec![2]),
            ] {
                let ctx = egui::Context::default();
                let mut queue = Queue::default();
                queue.replace((0..5).map(|i| Media {
                    path: PathBuf::from(format!("{i}.mp4")),
                    ..Default::default()
                }).collect(), 2);
                let current = queue.current().cloned();
                let mut selected: BTreeSet<usize> = initial.into_iter().collect();
                let mut rects = Vec::new();
                let mut render = |events| {
                    let _ = ctx.run(egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::splat(400.))),
                        events,
                        ..Default::default()
                    }, |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let mut pending = None;
                            rects.clear();
                            for i in 0..queue.items.len() {
                                let response = if music_mode {
                                    ui.push_id(i, |ui| music::playlist_row(
                                        ui, &queue.items[i], i, selected.contains(&i), queue.index == Some(i),
                                    )).inner
                                } else {
                                    media_row(ui, "Video", "", 300., selected.contains(&i))
                                        .interact(Sense::click_and_drag())
                                };
                                rects.push((i, response.rect));
                                if music_mode {
                                    if let Some((before, payload)) = music::playlist_drag(
                                        ui, &response, i, &mut selected, &queue.items, &sequoia::music::Target::Queue,
                                    ) {
                                        assert_eq!(payload.source, Some(sequoia::music::Target::Queue));
                                        assert_eq!(payload.items.len(), payload.indices.len());
                                        pending = Some((before, payload.indices.clone()));
                                    }
                                } else if let Some(movement) = queue_drag(ui, &response, i, &mut selected) {
                                    pending = Some(movement);
                                }
                            }
                            marquee(ui, "selection", ui.max_rect(), &rects, &mut selected, true);
                            let blank = ui.interact(ui.max_rect(), ui.id().with("append"), Sense::hover());
                            if pending.is_none() {
                                if music_mode {
                                    if let Some(payload) = blank.dnd_release_payload::<sequoia::music::Drag>() {
                                        pending = Some((queue.items.len(), payload.indices.clone()));
                                    }
                                } else if let Some(payload) = blank.dnd_release_payload::<QueueDrag>() {
                                    pending = Some((queue.items.len(), payload.rows.clone()));
                                }
                            }
                            if let Some((before, rows)) = pending {
                                selected = queue.move_rows(&rows, before).into_iter().collect();
                            }
                        });
                    });
                    rects.clone()
                };
                let rects = render(vec![]);
                let origin = rects[start].1.center();
                render(press(origin, egui::Modifiers::NONE));
                render(vec![egui::Event::PointerMoved(origin + Vec2::new(8., 0.))]);
                let dest = end.map_or(egui::pos2(100., 300.), |(i, after)| {
                    let rect = rects[i].1;
                    egui::pos2(rect.center().x, if after { rect.bottom() - 2. } else { rect.top() + 2. })
                });
                render(vec![egui::Event::PointerMoved(dest)]);
                render(vec![egui::Event::PointerButton {
                    pos: dest,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
                assert_eq!(queue.items.iter().map(|m| m.path.clone()).collect::<Vec<_>>(),
                           expected.iter().map(|i| PathBuf::from(format!("{i}.mp4"))).collect::<Vec<_>>());
                assert_eq!(selected, selection.into_iter().collect());
                assert_eq!(queue.current(), current.as_ref());
            }
        }
    }
}
