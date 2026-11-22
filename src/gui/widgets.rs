use egui::{
    self, Align, Color32, CornerRadius, CursorIcon, Layout, Painter, Pos2, Rect, Response,
    RichText, Sense, Shape, Stroke, TextStyle, Ui, Vec2, WidgetInfo, WidgetType, pos2, vec2,
};

use super::theme;

pub const FIELD_WIDTH: f32 = 78.0;
pub const FIELD_HEIGHT: f32 = 20.0;
const FIELD_PADDING: f32 = 3.0;
/// Fixed width for the unit label inside a numeric field. Reserving the same
/// slot for `CPS`, `MS`, `%`, `X` and `Y` keeps every field box and slider end
/// aligned instead of shifting with the suffix width.
const SUFFIX_SLOT: f32 = 20.0;
const INPUT_GAP: f32 = 3.0;
const INPUT_WIDTH: f32 = FIELD_WIDTH + INPUT_GAP + SUFFIX_SLOT;

/// A small pin that toggles the always-on-top window level.
pub fn pin_button(ui: &mut Ui, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("pin_hover"), response.hovered(), 0.16);

    let painter = ui.painter();
    if active || hover > 0.01 {
        let alpha = if active {
            30
        } else {
            (12.0 * hover).round() as u8
        };
        painter.rect_filled(
            rect,
            CornerRadius::same(7),
            theme::alpha(theme::accent(), alpha),
        );
    }

    let color = if active {
        theme::accent()
    } else if hover > 0.5 {
        theme::text()
    } else {
        theme::muted()
    };
    let center = rect.center();
    let head = center + vec2(0.0, -4.0);
    painter.circle_filled(head, 2.8, color);
    painter.add(egui::Shape::convex_polygon(
        vec![
            head + vec2(-2.6, 1.2),
            head + vec2(2.6, 1.2),
            center + vec2(0.0, 2.4),
        ],
        color,
        Stroke::NONE,
    ));
    painter.line_segment(
        [center + vec2(0.0, 2.0), center + vec2(0.0, 7.0)],
        Stroke::new(1.4_f32, color),
    );

    response
}

pub fn power_button(ui: &mut Ui, active: bool, busy: bool, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let response = if busy {
        response
    } else {
        response.on_hover_cursor(CursorIcon::PointingHand)
    };

    let ctx = ui.ctx().clone();
    let hover_t =
        ctx.animate_bool_with_time(response.id.with("hover"), response.hovered() && !busy, 0.16);
    let active_t = ctx.animate_bool_with_time(response.id.with("active"), active, 0.20);
    let time = ctx.input(|input| input.time) as f32;

    let center = rect.center();
    let radius = size * 0.40;
    let outer = size * 0.06;
    let painter = ui.painter();

    if hover_t > 0.01 {
        painter.circle_filled(
            center,
            radius,
            theme::alpha(theme::text(), (5.0 * hover_t) as u8),
        );
    }
    if active_t > 0.01 {
        painter.circle_filled(
            center,
            radius,
            theme::alpha(theme::accent(), (255.0 * active_t) as u8),
        );
        painter.circle_stroke(
            center,
            radius + outer,
            Stroke::new(
                1.0_f32,
                theme::alpha(theme::accent(), (46.0 * active_t) as u8),
            ),
        );
    }

    let ring = theme::mix(
        theme::text(),
        theme::mix(theme::accent(), theme::text(), 0.3),
        active_t,
    );
    painter.circle_stroke(center, radius, Stroke::new(1.2 + hover_t * 0.4, ring));

    let glyph = theme::mix(theme::text(), theme::on_accent(), active_t);
    paint_power_glyph(
        painter,
        center,
        radius * 0.36,
        glyph,
        (size * 0.021).max(1.8),
    );

    if busy {
        arc(
            painter,
            center,
            radius + outer + size * 0.03,
            time * 3.2,
            time * 3.2 + 1.5,
            Stroke::new(1.5_f32, theme::accent()),
        );
    }

    response
}

pub fn paint_power_glyph(painter: &Painter, center: Pos2, radius: f32, color: Color32, width: f32) {
    let stroke = Stroke::new(width, color);
    arc(painter, center, radius, -0.87, 4.01, stroke);
    painter.line_segment(
        [
            center + vec2(0.0, -radius * 1.24),
            center + vec2(0.0, -radius * 0.18),
        ],
        stroke,
    );
}

pub fn tabs<T: Copy + PartialEq>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    selected: &mut T,
    options: &[(T, &str)],
) -> bool {
    segmented_impl(ui, id_salt, selected, options, 30.0, true)
}

pub fn segmented<T: Copy + PartialEq>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    selected: &mut T,
    options: &[(T, &str)],
) -> bool {
    segmented_impl(ui, id_salt, selected, options, 24.0, false)
}

fn segmented_impl<T: Copy + PartialEq>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash,
    selected: &mut T,
    options: &[(T, &str)],
    height: f32,
    emphasize: bool,
) -> bool {
    if options.is_empty() {
        return false;
    }

    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let response = ui
        .interact(rect, ui.make_persistent_id(id_salt), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    let count = options.len();
    let segment_width = rect.width() / count as f32;

    let pointer = ui.ctx().pointer_hover_pos();
    let hovered = pointer.and_then(|position| {
        if rect.contains(position) {
            Some(
                ((position.x - rect.left()) / segment_width)
                    .floor()
                    .clamp(0.0, (count - 1) as f32) as usize,
            )
        } else {
            None
        }
    });

    let mut clicked = None;
    if response.clicked() {
        if let Some(position) = response.interact_pointer_pos() {
            if rect.contains(position) {
                clicked = Some(
                    ((position.x - rect.left()) / segment_width)
                        .floor()
                        .clamp(0.0, (count - 1) as f32) as usize,
                );
            }
        }
    }

    let before = *selected;
    let glass = theme::glass();
    let radius = CornerRadius::same(if glass {
        if emphasize { 12 } else { 10 }
    } else if emphasize {
        9
    } else {
        7
    });
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        radius,
        if glass {
            theme::alpha(Color32::WHITE, 86)
        } else {
            theme::input()
        },
    );
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(
            1.0_f32,
            if glass {
                theme::alpha(Color32::WHITE, 170)
            } else {
                theme::line()
            },
        ),
        egui::StrokeKind::Inside,
    );

    for (index, (value, label)) in options.iter().enumerate() {
        let segment = Rect::from_min_size(
            pos2(rect.left() + index as f32 * segment_width, rect.top() + 2.0),
            vec2(segment_width, rect.height() - 4.0),
        );
        let is_selected = *value == before;
        let is_hovered = hovered == Some(index);
        let segment_radius = CornerRadius::same(if glass {
            if emphasize { 10 } else { 8 }
        } else if emphasize {
            7
        } else {
            5
        });

        if glass {
            if is_selected {
                // iOS-style floating white capsule: a soft shadow and no
                // accent outline.
                painter.rect_filled(
                    segment.translate(vec2(0.0, 1.0)),
                    segment_radius,
                    theme::alpha(Color32::BLACK, 26),
                );
                painter.rect_filled(segment, segment_radius, theme::alpha(Color32::WHITE, 235));
            } else if is_hovered {
                painter.rect_filled(segment, segment_radius, theme::alpha(Color32::WHITE, 80));
            }
        } else {
            if is_selected || is_hovered {
                painter.rect_filled(
                    segment.shrink2(vec2(1.0, 0.0)),
                    segment_radius,
                    theme::alpha(
                        theme::accent(),
                        if is_selected {
                            if emphasize { 40 } else { 30 }
                        } else {
                            12
                        },
                    ),
                );
            }
            if is_selected {
                painter.rect_stroke(
                    segment.shrink2(vec2(1.0, 0.0)),
                    segment_radius,
                    Stroke::new(1.0_f32, theme::accent()),
                    egui::StrokeKind::Inside,
                );
            }
        }

        let color = if is_selected || is_hovered {
            theme::text()
        } else {
            theme::muted()
        };
        let font = theme::mono_medium(if emphasize { 10.0 } else { 9.0 });
        let galley = painter.layout_no_wrap((*label).to_owned(), font, color);
        let text_pos = pos2(
            segment.center().x - galley.size().x * 0.5,
            segment.center().y - galley.size().y * 0.5,
        );
        painter.galley(text_pos, galley, color);
    }

    if let Some(index) = clicked {
        *selected = options[index].0;
    }

    *selected != before
}

pub fn parameter_slider(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    logarithmic: bool,
    step: f64,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 18.0),
        Sense::click_and_drag(),
    );
    let mut response = response.on_hover_cursor(CursorIcon::ResizeHorizontal);

    let before = *value;
    if response.dragged() || response.clicked() {
        if let Some(position) = response.interact_pointer_pos() {
            let left = rect.left() + 8.0;
            let right = rect.right() - 8.0;
            let t = ((position.x - left) / (right - left).max(1.0)).clamp(0.0, 1.0);
            let mut next = from_norm(t, &range, logarithmic);
            if step > 0.0 {
                next = (next / step).round() * step;
            }
            *value = next.clamp(*range.start(), *range.end());
        }
    }

    if (*value - before).abs() > f64::EPSILON {
        response.mark_changed();
    }

    let t = to_norm(*value, &range, logarithmic);
    let left = rect.left() + 8.0;
    let right = rect.right() - 8.0;
    let center_y = rect.center().y;
    let handle_x = left + t * (right - left);

    let hot = response.hovered() || response.dragged();
    let painter = ui.painter();
    if hot {
        painter.circle_filled(
            pos2(handle_x, center_y),
            10.0,
            theme::alpha(theme::accent(), 24),
        );
    }

    let track = Rect::from_min_max(pos2(left, center_y - 2.0), pos2(right, center_y + 2.0));
    painter.rect_filled(track, CornerRadius::same(2), theme::line());

    if handle_x > left + 0.5 {
        let fill = Rect::from_min_max(pos2(left, center_y - 2.0), pos2(handle_x, center_y + 2.0));
        painter.rect_filled(fill, CornerRadius::same(2), theme::accent());
    }

    let handle = pos2(handle_x, center_y);
    let radius = if hot { 6.5 } else { 5.5 };
    if theme::glass() {
        // Frosted white thumb with a blue ring and a soft contact shadow.
        painter.circle_filled(
            handle + vec2(0.0, 1.0),
            radius + 0.5,
            theme::alpha(Color32::BLACK, 28),
        );
        painter.circle_filled(handle, radius, Color32::WHITE);
        painter.circle_stroke(handle, radius, Stroke::new(2.0_f32, theme::accent()));
    } else {
        painter.circle_filled(
            handle,
            radius,
            if hot { theme::accent() } else { theme::bg() },
        );
        painter.circle_stroke(handle, radius, Stroke::new(2.0_f32, theme::accent()));
    }

    response
}

fn paint_field_fill(ui: &Ui, rect: Rect) {
    ui.painter()
        .rect_filled(rect, CornerRadius::same(7), theme::field());
}

fn paint_field_border(ui: &Ui, rect: Rect, hovered: bool, focused: bool) {
    let (color, width) = if focused {
        (theme::accent(), 1.6)
    } else if hovered {
        (theme::alpha(theme::accent(), 150), 1.2)
    } else {
        (theme::line(), 1.0_f32)
    };
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(7),
        Stroke::new(width, color),
        egui::StrokeKind::Inside,
    );
}

/// Total width of a numeric field plus its unit label, so the slider row can
/// reserve enough space without clipping the field. The suffix slot is fixed,
/// which is what makes all slider tracks and field boxes line up.
pub fn numeric_row_width() -> f32 {
    INPUT_WIDTH
}

fn configure_field_child(ui: &mut Ui, rect: Rect) -> Ui {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    child.style_mut().override_text_style = Some(TextStyle::Monospace);
    child.style_mut().override_font_id = Some(theme::mono_medium(11.5));
    child.spacing_mut().interact_size =
        Vec2::new(FIELD_WIDTH - 2.0 * FIELD_PADDING, FIELD_HEIGHT - 2.0);
    child.spacing_mut().button_padding = Vec2::new(FIELD_PADDING, 1.0);
    child.visuals_mut().extreme_bg_color = if theme::glass() {
        Color32::TRANSPARENT
    } else {
        theme::panel()
    };
    child.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    child.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
    child.visuals_mut().widgets.hovered.weak_bg_fill = Color32::TRANSPARENT;
    child.visuals_mut().widgets.hovered.bg_stroke = Stroke::NONE;
    child.visuals_mut().widgets.active.weak_bg_fill = theme::alpha(theme::accent(), 14);
    child.visuals_mut().widgets.active.bg_stroke = Stroke::NONE;
    child
}

fn add_field(ui: &mut Ui, suffix: &str, value: impl egui::widgets::Widget) -> bool {
    // One rectangle for the whole input: value on the left, unit on the right.
    // Its width never depends on the suffix, so every row shares the same
    // right edge as the segmented controls.
    let (rect, _) = ui.allocate_exact_size(Vec2::new(INPUT_WIDTH, FIELD_HEIGHT), Sense::hover());
    paint_field_fill(ui, rect);

    let value_rect = Rect::from_min_size(rect.min, Vec2::new(FIELD_WIDTH, rect.height()));
    let mut child = configure_field_child(ui, value_rect);
    let response = child.add_sized(
        Vec2::new(FIELD_WIDTH - 2.0 * FIELD_PADDING, FIELD_HEIGHT - 2.0),
        value,
    );

    let galley =
        ui.painter()
            .layout_no_wrap(suffix.to_owned(), theme::mono_medium(8.5), theme::dim());
    ui.painter().galley(
        pos2(
            rect.right() - INPUT_GAP - galley.size().x,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        theme::dim(),
    );

    // Paint the border last so it stays crisp over the field content, and give
    // idle fields a visible outline instead of blending into the panel.
    let hovered = response.hovered() || ui.rect_contains_pointer(rect);
    paint_field_border(ui, rect, hovered, response.has_focus());
    response.changed()
}

pub fn numeric_field(
    ui: &mut Ui,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    speed: f64,
    decimals: usize,
    suffix: &str,
) -> bool {
    add_field(
        ui,
        suffix,
        egui::DragValue::new(value)
            .range(range)
            .speed(speed)
            .max_decimals(decimals),
    )
}

pub fn integer_field(
    ui: &mut Ui,
    value: &mut u64,
    range: std::ops::RangeInclusive<u64>,
    speed: f64,
    suffix: &str,
) -> bool {
    add_field(
        ui,
        suffix,
        egui::DragValue::new(value)
            .range(range)
            .speed(speed)
            .max_decimals(0),
    )
}

/// Signed whole-number field used by the fixed-point coordinates.
pub fn coordinate_field(ui: &mut Ui, value: &mut i32, suffix: &str) -> bool {
    add_field(
        ui,
        suffix,
        egui::DragValue::new(value).speed(1.0).max_decimals(0),
    )
}

pub fn checkbox(ui: &mut Ui, checked: &mut bool, label: &str) -> Response {
    let box_size = 16.0;
    let gap = 7.0;
    let font = theme::sans(12.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), theme::text());
    let height = FIELD_HEIGHT.max(galley.size().y + 4.0);
    let size = Vec2::new(box_size + gap + galley.size().x, height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let enabled = ui.is_enabled();
    let mut response = if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };

    let mut changed = enabled && response.clicked();
    if enabled && response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Space)) {
        changed = true;
    }
    if changed {
        *checked = !*checked;
    }

    let hovered = enabled && response.hovered();
    let box_rect = Rect::from_min_size(
        pos2(rect.left(), rect.center().y - box_size * 0.5),
        Vec2::splat(box_size),
    );
    let painter = ui.painter();
    let fill = if *checked {
        theme::accent()
    } else if hovered {
        theme::alpha(theme::accent(), 18)
    } else if theme::glass() {
        theme::alpha(Color32::WHITE, 210)
    } else {
        theme::input()
    };
    let stroke = if *checked || hovered {
        theme::accent()
    } else if theme::glass() {
        theme::line()
    } else {
        theme::muted()
    };
    let box_radius = CornerRadius::same(if theme::glass() { 5 } else { 4 });
    painter.rect_filled(box_rect, box_radius, fill);
    painter.rect_stroke(
        box_rect,
        box_radius,
        Stroke::new(1.5_f32, stroke),
        egui::StrokeKind::Inside,
    );

    if *checked {
        let check = Stroke::new(2.0_f32, theme::on_accent());
        painter.line_segment(
            [
                pos2(box_rect.left() + 3.8, box_rect.center().y),
                pos2(box_rect.left() + 6.8, box_rect.bottom() - 4.3),
            ],
            check,
        );
        painter.line_segment(
            [
                pos2(box_rect.left() + 6.8, box_rect.bottom() - 4.3),
                pos2(box_rect.right() - 3.5, box_rect.top() + 4.5),
            ],
            check,
        );
    }

    let color = if enabled { theme::text() } else { theme::dim() };
    let galley = painter.layout_no_wrap(label.to_owned(), font, color);
    painter.galley(
        pos2(
            box_rect.right() + gap,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );

    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, enabled, *checked, label));

    if changed {
        response.mark_changed();
    }
    response
}

pub fn notice_line(ui: &mut Ui, tone: Color32, title: &str, text: &str) -> Response {
    let full = format!("{title}: {text}");
    ui.horizontal(|ui| {
        let (dot, _) = ui.allocate_exact_size(Vec2::splat(6.0), Sense::hover());
        ui.painter().circle_filled(dot.center(), 3.0, tone);
        ui.label(
            RichText::new(title)
                .font(theme::mono_medium(8.5))
                .color(tone),
        );

        let width = ui.available_width().max(40.0);
        let response = ui
            .allocate_ui_with_layout(
                Vec2::new(width, 18.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(text)
                                .font(theme::sans(10.5))
                                .color(theme::text()),
                        )
                        .truncate(),
                    )
                },
            )
            .inner;
        response.on_hover_text(full)
    })
    .inner
}

pub fn pipette_button(ui: &mut Ui, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(26.0, 24.0), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let accent = theme::accent();
    let color = if active {
        accent
    } else if response.hovered() {
        theme::text()
    } else {
        theme::muted()
    };

    if active || response.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(6),
            theme::alpha(accent, if active { 24 } else { 10 }),
        );
    }

    // A small eyedropper: round bulb at the top right, diagonal stem and a
    // wider tip at the bottom left.
    let center = rect.center();
    let painter = ui.painter();
    let bulb = pos2(center.x + 4.0, center.y - 4.0);
    painter.circle_filled(bulb, 3.0, color);
    painter.line_segment(
        [
            pos2(center.x + 2.0, center.y - 2.0),
            pos2(center.x - 5.0, center.y + 5.0),
        ],
        Stroke::new(1.8_f32, color),
    );
    painter.line_segment(
        [
            pos2(center.x - 5.0, center.y + 5.0),
            pos2(center.x - 7.0, center.y + 7.0),
        ],
        Stroke::new(2.8_f32, color),
    );

    response
}

pub fn text_button(ui: &mut Ui, label: &str, idle: Color32, hover: Color32) -> Response {
    let enabled = ui.is_enabled();
    let idle = if enabled { idle } else { theme::dim() };
    let prepared = ui
        .painter()
        .layout_no_wrap(label.to_owned(), theme::sans_medium(10.5), idle);
    let size = Vec2::new(prepared.size().x + 20.0, 24.0);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let response = if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };

    let color = if enabled && response.hovered() {
        hover
    } else {
        idle
    };
    let painter = ui.painter();
    let glass = theme::glass();
    let radius = CornerRadius::same(if glass { 11 } else { 6 });
    if enabled {
        let (fill, stroke) = if glass {
            if response.hovered() {
                (
                    theme::alpha(hover, 34),
                    Stroke::new(1.0_f32, theme::alpha(hover, 170)),
                )
            } else {
                (
                    theme::alpha(Color32::WHITE, 165),
                    Stroke::new(1.0_f32, theme::alpha(Color32::BLACK, 22)),
                )
            }
        } else if response.hovered() {
            (
                theme::alpha(hover, 16),
                Stroke::new(1.0_f32, theme::alpha(hover, 190)),
            )
        } else {
            (
                theme::alpha(theme::input(), 210),
                Stroke::new(1.0_f32, theme::line()),
            )
        };
        painter.rect_filled(rect, radius, fill);
        painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
    }
    let galley = painter.layout_no_wrap(label.to_owned(), theme::sans_medium(10.5), color);
    painter.galley(
        pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );

    response
}

pub fn solid_button(ui: &mut Ui, label: &str, fill: Color32, text: Color32) -> Response {
    let button = egui::Button::new(
        RichText::new(label)
            .font(theme::sans_medium(11.5))
            .color(text),
    )
    .fill(fill)
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(8))
    .min_size(Vec2::new(0.0, 32.0));
    ui.add(button)
}

fn arc(painter: &Painter, center: Pos2, radius: f32, start: f32, end: f32, stroke: Stroke) {
    const STEPS: usize = 48;
    let points = (0..=STEPS)
        .map(|index| {
            let t = index as f32 / STEPS as f32;
            let angle = start + (end - start) * t;
            center + vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    painter.add(Shape::line(points, stroke));
}

fn to_norm(value: f64, range: &std::ops::RangeInclusive<f64>, logarithmic: bool) -> f32 {
    let start = *range.start();
    let end = *range.end();
    if end <= start {
        return 0.0;
    }
    let value = value.clamp(start, end);

    let t = if logarithmic && start > 0.0 {
        (value / start).ln() / (end / start).ln()
    } else {
        (value - start) / (end - start)
    };

    t.clamp(0.0, 1.0) as f32
}

fn from_norm(t: f32, range: &std::ops::RangeInclusive<f64>, logarithmic: bool) -> f64 {
    let start = *range.start();
    let end = *range.end();
    let t = t.clamp(0.0, 1.0) as f64;

    if logarithmic && start > 0.0 {
        start * (end / start).powf(t)
    } else {
        start + (end - start) * t
    }
}
