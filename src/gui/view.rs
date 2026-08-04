//! View rendering for the iced GUI.

use crate::cleanup_history::{self, DayTotals};
use crate::disclaimer;
use crate::identity;
use crate::log_cleanup;
use crate::prefs::{self, ThemePref};
use crate::system_links;
use crate::telemetry::SettingState;
use crate::temp_cleanup;
use iced::mouse;
use iced::widget::canvas::{self, Cache, Canvas, Frame, Geometry, Path, Stroke, Text};
use iced::widget::{
    button, checkbox, column, container, opaque, row, rule, scrollable, stack, table, text,
    toggler, Button, Column, Row, Space,
};
use iced::{
    alignment, Alignment, Animation, Color, Element, Event, Length, Pixels, Point, Rectangle, Size,
    Theme,
};
use iced::time::Instant;

use super::message::Message;
use super::theme::{
    self, disclaimer_color, elevated_ok, elevated_warn, off_color, on_color, status_err_color,
    status_ok_color, ThemePaint,
};
use super::App;

const SECTION_PAD: u16 = 10;

fn primary_btn<'a>(
    label: impl Into<Element<'a, Message>>,
    msg: Message,
) -> Button<'a, Message> {
    button(label).style(theme::btn_primary).on_press(msg)
}

fn secondary_btn<'a>(
    label: impl Into<Element<'a, Message>>,
    msg: Message,
) -> Button<'a, Message> {
    button(label).style(theme::btn_secondary).on_press(msg)
}

fn danger_btn<'a>(
    label: impl Into<Element<'a, Message>>,
    msg: Message,
) -> Button<'a, Message> {
    button(label).style(theme::btn_danger).on_press(msg)
}

fn secondary_btn_idle<'a>(label: impl Into<Element<'a, Message>>) -> Button<'a, Message> {
    button(label).style(theme::btn_secondary)
}

fn section<'a>(content: Column<'a, Message>) -> Element<'a, Message> {
    container(content.spacing(8).width(Length::Fill))
        .width(Length::Fill)
        .padding(SECTION_PAD)
        .style(|theme: &Theme| container::Style {
            background: Some(iced::Background::Color(match theme {
                Theme::Dark => Color::from_rgb(0.125, 0.145, 0.17),
                _ => Color::from_rgb(0.94, 0.945, 0.95),
            })),
            border: iced::Border {
                color: match theme {
                    Theme::Dark => Color::from_rgb(0.2, 0.23, 0.27),
                    _ => Color::from_rgb(0.82, 0.85, 0.88),
                },
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn small_btn<'a>(label: &'a str, msg: Message) -> Element<'a, Message> {
    secondary_btn(text(label).size(12), msg).into()
}

fn wrap_row<'a>(items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    Row::with_children(items)
        .spacing(6)
        .width(Length::Fill)
        .wrap()
        .into()
}

/// Aligned Target / Count / Size table for cleanup status lists.
#[derive(Clone)]
struct StatRow {
    target: String,
    count: String,
    size: String,
}

fn stats_table<'a>(rows: Vec<StatRow>) -> Element<'a, Message> {
    table(
        [
            table::column(text("Target").size(11), |r: StatRow| {
                text(r.target).size(11).font(iced::Font::MONOSPACE)
            })
            .width(Length::FillPortion(5)),
            table::column(text("Count").size(11), |r: StatRow| {
                text(r.count).size(11).font(iced::Font::MONOSPACE)
            })
            .width(Length::FillPortion(2))
            .align_x(alignment::Horizontal::Right),
            table::column(text("Size").size(11), |r: StatRow| {
                text(r.size).size(11).font(iced::Font::MONOSPACE)
            })
            .width(Length::FillPortion(3))
            .align_x(alignment::Horizontal::Right),
        ],
        rows,
    )
    .width(Length::Fill)
    .padding_x(6)
    .padding_y(4)
    .separator_x(0)
    .separator_y(1)
    .into()
}

/// Aligned Setting / State / Value table for the verify panel.
#[derive(Clone)]
struct VerifyRow {
    setting: String,
    state: String,
    collecting: bool,
    value: String,
}

fn verify_table<'a>(rows: Vec<VerifyRow>, dark: bool) -> Element<'a, Message> {
    table(
        [
            table::column(text("Setting").size(11), |r: VerifyRow| {
                text(r.setting).size(11).font(iced::Font::MONOSPACE)
            })
            .width(Length::FillPortion(3)),
            table::column(text("State").size(11), |r: VerifyRow| {
                let color = if r.collecting {
                    on_color(dark)
                } else {
                    off_color(dark)
                };
                text(r.state).size(11).color(color)
            })
            .width(Length::FillPortion(1))
            .align_x(alignment::Horizontal::Center),
            table::column(text("Value").size(11), |r: VerifyRow| {
                text(r.value).size(11)
            })
            .width(Length::FillPortion(5)),
        ],
        rows,
    )
    .width(Length::Fill)
    .padding_x(6)
    .padding_y(5)
    .separator_x(0)
    .separator_y(1)
    .into()
}

/// Collapsible section with height animation (ease-out), similar to web accordions.
fn accordion<'a>(
    title: &'a str,
    subtitle: Option<Element<'a, Message>>,
    open: bool,
    anim: &Animation<bool>,
    now: Instant,
    max_body_h: f32,
    toggle: Message,
    body: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let t = anim.interpolate(0.0_f32, 1.0_f32, now);
    let animating = anim.is_animating(now);
    let chevron = if open || t > 0.5 { "▴" } else { "▾" };
    let header = button(
        row![
            text(title).size(13),
            Space::new().width(Length::Fill),
            text(chevron).size(13),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill),
    )
    .style(theme::btn_secondary)
    .width(Length::Fill)
    .on_press(toggle);

    let mut col = Column::new().spacing(6).width(Length::Fill).push(header);
    if let Some(sub) = subtitle {
        col = col.push(sub);
    }

    if let Some(body) = body {
        if t > 0.01 || open || animating {
            let panel = if t >= 0.995 && open && !animating {
                container(body).width(Length::Fill)
            } else {
                container(body)
                    .width(Length::Fill)
                    .height(Length::Fixed((max_body_h * t).max(0.5)))
                    .clip(true)
            };
            col = col.push(panel);
        }
    }
    col.into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    if app.show_disclaimer {
        return disclaimer_view(app);
    }

    let body = column![
        header(app),
        scrollable(main_scroll(app))
            .id("main_scroll")
            .height(Length::Fill)
            .width(Length::Fill),
        footer(app),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    // Always use a stack so opening settings/confirm does not remount the
    // scrollable (that was resetting scroll offset under the user's feet).
    let mut layers = vec![body.into()];
    if app.show_settings {
        layers.push(opaque(settings_overlay(app)));
    }
    if app.confirm.is_some() {
        layers.push(opaque(confirm_overlay(app)));
    }
    stack(layers)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn header(app: &App) -> Element<'_, Message> {
    let elev_label = if app.elevated {
        text("Administrator").size(12).color(elevated_ok())
    } else {
        text("Not elevated (read-only)")
            .size(12)
            .color(elevated_warn())
    };

    let mut actions: Vec<Element<Message>> = vec![elev_label.into()];
    if app.tray_enabled {
        actions.push(small_btn("Minimize to tray", Message::MinimizeToTray));
    }
    actions.push(small_btn("Settings", Message::ShowSettings));
    actions.push(danger_btn(text("Quit").size(12), Message::Quit).into());

    let primary = primary_actions(app);

    container(
        column![
            text(identity::PRODUCT_NAME).size(18),
            wrap_row(actions),
            text(
                "GUI helper over the same commands as tluw.exe \
                 (status / disable / enable / set). Each switch is ON = collecting."
            )
            .size(12)
            .color(Color::from_rgb(0.55, 0.55, 0.6)),
            primary,
            rule::horizontal(1),
        ]
        .spacing(6)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding([6, SECTION_PAD])
    .into()
}

fn primary_actions(app: &App) -> Element<'_, Message> {
    let mut items: Vec<Element<Message>> =
        vec![primary_btn("Verify status", Message::Verify).into()];

    if app.elevated {
        items.push(primary_btn("Turn all OFF", Message::TurnAllOff).into());
        items.push(secondary_btn("Turn all ON", Message::TurnAllOn).into());
    } else {
        items.push(primary_btn("Restart as Administrator", Message::RestartElevated).into());
    }

    items.push(
        text("OFF = privacy / blocked    ·    ON = collecting / allowed")
            .size(11)
            .color(Color::from_rgb(0.55, 0.55, 0.6))
            .into(),
    );

    wrap_row(items)
}

fn footer(app: &App) -> Element<'_, Message> {
    let dark = app.is_dark();
    let status_color = if app.status_ok {
        status_ok_color(dark)
    } else {
        status_err_color()
    };

    let mut accept_items: Vec<Element<Message>> = vec![
        text(disclaimer::SHORT)
            .size(11)
            .color(disclaimer_color())
            .into(),
        small_btn("Full disclaimer", Message::ShowDisclaimer),
    ];
    if let Some(rec) = disclaimer::read_record() {
        if !rec.accepted_at.is_empty() {
            accept_items.push(
                text(format!("Accepted: {}", rec.accepted_at))
                    .size(11)
                    .color(Color::from_rgb(0.55, 0.55, 0.6))
                    .into(),
            );
        }
    }

    container(
        column![
            text(&app.status).size(12).color(status_color),
            wrap_row(accept_items),
            text("CLI: tluw status | disable | enable | set <id> on|off | disclaimer")
                .size(11)
                .color(Color::from_rgb(0.55, 0.55, 0.6)),
        ]
        .spacing(4)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding([6, SECTION_PAD])
    .into()
}

fn main_scroll(app: &App) -> Element<'_, Message> {
    let mut col = Column::new().spacing(10).width(Length::Fill);
    col = col.push(dashboard(app));
    // Always reserve the verify accordion header so Verify doesn't shove the page.
    col = col.push(verify_panel(app));
    for (idx, state) in app.settings.iter().enumerate() {
        col = col.push(setting_card(app, idx, state));
    }
    col = col.push(clear_logs_section(app));
    col = col.push(temp_section(app));
    col = col.push(links_section(app));
    container(col.padding(SECTION_PAD))
        .width(Length::Fill)
        .into()
}

fn disclaimer_view(_app: &App) -> Element<'_, Message> {
    let already = disclaimer::is_accepted();
    let actions: Element<Message> = if already {
        row![
            Space::new().width(Length::Fill),
            secondary_btn("Close", Message::CloseDisclaimer),
        ]
        .width(Length::Fill)
        .into()
    } else {
        row![
            Space::new().width(Length::Fill),
            danger_btn("Quit", Message::Quit),
            primary_btn(
                "I understand and accept — continue",
                Message::AcceptDisclaimer
            ),
        ]
        .spacing(8)
        .width(Length::Fill)
        .into()
    };

    container(
        column![
            text("Disclaimer & liability waiver").size(20),
            text(disclaimer::SHORT).size(13).color(disclaimer_color()),
            scrollable(text(disclaimer::FULL).size(11).font(iced::Font::MONOSPACE))
                .height(Length::Fill)
                .width(Length::Fill),
            actions,
        ]
        .spacing(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn settings_overlay(app: &App) -> Element<'_, Message> {
    container(
        container(
            column![
                text("Settings").size(18),
                text(
                    "Preferences that stay out of the way — startup, post-update lockdown, and tray."
                )
                .size(12)
                .color(Color::from_rgb(0.55, 0.55, 0.6)),
                settings_body(app),
                rule::horizontal(1),
                row![
                    Space::new().width(Length::Fill),
                    secondary_btn("Close", Message::CloseSettings),
                ]
                .width(Length::Fill),
            ]
            .spacing(8)
            .width(Length::Fill)
            .padding(16),
        )
        .width(Length::Fixed(440.0))
        .style(|theme: &Theme| container::Style {
            background: Some(iced::Background::Color(match theme {
                Theme::Dark => Color::from_rgb(0.14, 0.16, 0.19),
                _ => Color::WHITE,
            })),
            border: iced::Border {
                color: Color::from_rgb(0.5, 0.5, 0.55),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.45))),
        ..Default::default()
    })
    .into()
}

fn confirm_overlay(app: &App) -> Element<'_, Message> {
    let Some(prompt) = app.confirm.as_ref() else {
        return Space::new().width(Length::Fill).height(Length::Fill).into();
    };

    let action = if prompt.danger {
        danger_btn(text(prompt.action_label.as_str()).size(13), Message::ConfirmAccept)
    } else {
        primary_btn(text(prompt.action_label.as_str()).size(13), Message::ConfirmAccept)
    };

    container(
        container(
            column![
                text(&prompt.title).size(18),
                text(&prompt.body)
                    .size(13)
                    .color(Color::from_rgb(0.55, 0.55, 0.6)),
                rule::horizontal(1),
                row![
                    Space::new().width(Length::Fill),
                    secondary_btn("Cancel", Message::ConfirmCancel),
                    action,
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .width(Length::Fill),
            ]
            .spacing(12)
            .width(Length::Fill)
            .padding(20),
        )
        .width(Length::Fixed(460.0))
        .style(|theme: &Theme| container::Style {
            background: Some(iced::Background::Color(match theme {
                Theme::Dark => Color::from_rgb(0.14, 0.16, 0.19),
                _ => Color::WHITE,
            })),
            border: iced::Border {
                color: match theme {
                    Theme::Dark => Color::from_rgb(0.28, 0.32, 0.36),
                    _ => Color::from_rgb(0.78, 0.80, 0.84),
                },
                width: 1.0,
                radius: theme::BUTTON_RADIUS.into(),
            },
            ..Default::default()
        }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.5))),
        ..Default::default()
    })
    .into()
}

fn settings_body(app: &App) -> Element<'_, Message> {
    let mut theme_items: Vec<Element<Message>> = Vec::new();
    for pref in ThemePref::ALL {
        let selected = app.theme_pref == pref;
        let label = if pref == ThemePref::System {
            if prefs::system_apps_dark() {
                "System (dark)".to_string()
            } else {
                "System (light)".to_string()
            }
        } else {
            pref.label().to_string()
        };
        let btn = if selected {
            secondary_btn_idle(text(label).size(12)).style(theme::btn_primary)
        } else {
            secondary_btn(text(label).size(12), Message::SetTheme(pref))
        };
        theme_items.push(btn.into());
    }

    let startup = row![
        checkbox(app.integration.run_at_startup).on_toggle(Message::SetStartup),
        text("Run GUI when Windows starts").size(12),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let post = row![
        checkbox(app.integration.post_update).on_toggle(Message::SetPostUpdate),
        text("Re-apply lockdown after Windows Update (scheduled task)").size(12),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut col = column![
        text("Appearance").size(14),
        text("Use the system theme, or lock light/dark for this app.")
            .size(12)
            .color(Color::from_rgb(0.55, 0.55, 0.6)),
        wrap_row(theme_items),
        rule::horizontal(1),
        text("Automation").size(14),
        text(
            "Optional: open the app at logon, and re-run lockdown after Windows Update \
             (Event ID 19) with a logon backup."
        )
        .size(12)
        .color(Color::from_rgb(0.55, 0.55, 0.6)),
        startup,
        post,
    ]
    .spacing(6);

    if !app.elevated {
        col = col.push(
            text("Post-update task requires Administrator.")
                .size(12)
                .color(elevated_warn()),
        );
    }

    col = col.push(rule::horizontal(1));
    col = col.push(text("System tray").size(14));
    col = col.push(
        text("Keep the app available from the notification area.")
            .size(12)
            .color(Color::from_rgb(0.55, 0.55, 0.6)),
    );
    col = col.push(
        row![
            checkbox(app.tray_enabled).on_toggle(Message::SetTrayEnabled),
            text("Keep in system tray (close hides window)").size(12),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );

    if app.tray_enabled {
        col = col.push(
            text("Tray menu: Open dashboard · Disable telemetry · Clear safe logs · Quit")
                .size(11)
                .color(Color::from_rgb(0.55, 0.55, 0.6)),
        );
    }

    col.into()
}

// --- Dashboard ---

#[derive(Default)]
struct ChartHover {
    /// Nearest day index under the cursor, if any.
    day: Option<usize>,
}

struct CleanupChart {
    days: Vec<DayTotals>,
    theme: ThemePaint,
    cache: Cache,
}

impl CleanupChart {
    fn new(days: Vec<DayTotals>, theme: ThemePaint) -> Self {
        Self {
            days,
            theme,
            cache: Cache::default(),
        }
    }

    fn plot_layout(size: Size) -> (Rectangle, f32) {
        let left_pad = if size.width < 380.0 { 34.0 } else { 42.0 };
        let bottom_pad = 28.0;
        let top_pad = 10.0;
        let plot = Rectangle::new(
            Point::new(left_pad, top_pad),
            Size::new(
                (size.width - left_pad - 10.0).max(1.0),
                (size.height - top_pad - bottom_pad).max(1.0),
            ),
        );
        (plot, left_pad)
    }

    fn series_mb(days: &[DayTotals]) -> (Vec<f32>, Vec<f32>, Vec<f32>, f32) {
        const MB: f64 = 1024.0 * 1024.0;
        let totals: Vec<f32> = days
            .iter()
            .map(|d| (d.total_bytes() as f64 / MB) as f32)
            .collect();
        let logs: Vec<f32> = days
            .iter()
            .map(|d| (d.logs_bytes as f64 / MB) as f32)
            .collect();
        let temp: Vec<f32> = days
            .iter()
            .map(|d| (d.temp_bytes as f64 / MB) as f32)
            .collect();
        let max_y = totals.iter().cloned().fold(0.0_f32, f32::max).max(1.0);
        (totals, logs, temp, max_y)
    }

    fn x_at(plot: Rectangle, n: usize, i: usize) -> f32 {
        if n <= 1 {
            plot.position().x + plot.size().width / 2.0
        } else {
            plot.position().x + (i as f32 / (n - 1) as f32) * plot.size().width
        }
    }

    fn y_at(plot: Rectangle, max_y: f32, mb: f32) -> f32 {
        plot.position().y + plot.size().height * (1.0 - (mb / max_y).clamp(0.0, 1.0))
    }

    fn nearest_day(days: &[DayTotals], plot: Rectangle, cursor: Point) -> Option<usize> {
        if days.is_empty() || !plot.contains(cursor) {
            return None;
        }
        let n = days.len();
        let mut best = 0usize;
        let mut best_dx = f32::MAX;
        for i in 0..n {
            let dx = (Self::x_at(plot, n, i) - cursor.x).abs();
            if dx < best_dx {
                best_dx = dx;
                best = i;
            }
        }
        // Snap only when reasonably close to a point on the X axis.
        if best_dx <= (plot.size().width / n.max(1) as f32).max(18.0) {
            Some(best)
        } else {
            Some(best) // still show nearest day while inside the plot
        }
    }

    fn short_date(date: &str) -> String {
        // "yyyy-MM-dd" → "MM-dd"
        if date.len() >= 10 {
            date[5..10].to_string()
        } else {
            date.to_string()
        }
    }
}

impl<Message> canvas::Program<Message> for CleanupChart {
    type State = ChartHover;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let next = match event {
            Event::Mouse(mouse::Event::CursorLeft) => None,
            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Mouse(mouse::Event::CursorEntered) => {
                let (plot, _) = Self::plot_layout(bounds.size());
                cursor.position_in(bounds).and_then(|local| {
                    // Translate: position_in is already local to bounds.
                    Self::nearest_day(&self.days, plot, local)
                })
            }
            _ => return None,
        };

        if state.day != next {
            state.day = next;
            Some(canvas::Action::request_redraw())
        } else {
            None
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.day.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let base = self.cache.draw(renderer, bounds.size(), |frame| {
            draw_cleanup_chart(frame, &self.days, &self.theme, bounds.size());
        });

        let mut out = vec![base];
        if let Some(idx) = state.day {
            let mut overlay = Frame::new(renderer, bounds.size());
            draw_chart_hover(&mut overlay, &self.days, &self.theme, bounds.size(), idx);
            out.push(overlay.into_geometry());
        }
        out
    }
}

fn chart_label(
    content: impl Into<String>,
    position: Point,
    color: Color,
    size: f32,
    align_x: iced::widget::text::Alignment,
    align_y: alignment::Vertical,
) -> Text {
    Text {
        content: content.into(),
        position,
        color,
        size: Pixels(size),
        align_x,
        align_y,
        font: iced::Font::MONOSPACE,
        ..Text::default()
    }
}

fn draw_cleanup_chart(frame: &mut Frame, days: &[DayTotals], theme: &ThemePaint, size: Size) {
    let (plot, _left_pad) = CleanupChart::plot_layout(size);
    let (totals_mb, logs_mb, temp_mb, max_y) = CleanupChart::series_mb(days);
    let n = days.len().max(1);

    frame.fill_rectangle(Point::ORIGIN, size, theme.chart_bg);

    // Plot border + horizontal grid + Y labels (MB).
    frame.stroke(
        &Path::rectangle(plot.position(), plot.size()),
        Stroke::default()
            .with_width(1.0)
            .with_color(theme.chart_axis),
    );

    for i in 0..=4 {
        let t = i as f32 / 4.0;
        let y = plot.position().y + plot.size().height * (1.0 - t);
        frame.stroke(
            &Path::line(
                Point::new(plot.position().x, y),
                Point::new(plot.position().x + plot.size().width, y),
            ),
            Stroke::default()
                .with_width(1.0)
                .with_color(theme.chart_grid),
        );
        let label = format!("{:.0}", max_y * t);
        frame.fill_text(chart_label(
            label,
            Point::new(plot.position().x - 4.0, y),
            theme.text_muted,
            10.0,
            iced::widget::text::Alignment::Right,
            alignment::Vertical::Center,
        ));
    }

    // Y-axis unit caption.
    frame.fill_text(chart_label(
        "MB",
        Point::new(6.0, plot.position().y),
        theme.text_muted,
        9.0,
        iced::widget::text::Alignment::Left,
        alignment::Vertical::Top,
    ));

    // X-axis date labels (skip some when dense).
    let step = if n > 10 {
        (n / 6).max(1)
    } else if n > 7 {
        2
    } else {
        1
    };
    for i in 0..n {
        if i != 0 && i != n - 1 && i % step != 0 {
            continue;
        }
        let x = CleanupChart::x_at(plot, n, i);
        let date = days.get(i).map(|d| CleanupChart::short_date(&d.date)).unwrap_or_default();
        frame.fill_text(chart_label(
            date,
            Point::new(x, size.height - 6.0),
            theme.text_muted,
            9.0,
            iced::widget::text::Alignment::Center,
            alignment::Vertical::Bottom,
        ));
    }

    let x_at = |i: usize| CleanupChart::x_at(plot, n, i);
    let y_at = |mb: f32| CleanupChart::y_at(plot, max_y, mb);

    let stroke_line = |frame: &mut Frame, values: &[f32], color: Color, width: f32| {
        for (i, pair) in values.windows(2).enumerate() {
            frame.stroke(
                &Path::line(
                    Point::new(x_at(i), y_at(pair[0])),
                    Point::new(x_at(i + 1), y_at(pair[1])),
                ),
                Stroke::default().with_width(width).with_color(color),
            );
        }
        for (i, v) in values.iter().enumerate() {
            frame.fill(
                &Path::circle(Point::new(x_at(i), y_at(*v)), 2.4),
                color,
            );
        }
    };

    let col_logs = Color::from_rgb(0.353, 0.706, 0.549);
    let col_temp = Color::from_rgb(0.353, 0.549, 0.784);
    let col_total = if theme.dark {
        Color::from_rgb(0.863, 0.824, 0.627)
    } else {
        Color::from_rgb(0.627, 0.471, 0.157)
    };

    stroke_line(frame, &logs_mb, col_logs, 1.5);
    stroke_line(frame, &temp_mb, col_temp, 1.5);
    stroke_line(frame, &totals_mb, col_total, 2.2);
}

fn draw_chart_hover(
    frame: &mut Frame,
    days: &[DayTotals],
    theme: &ThemePaint,
    size: Size,
    idx: usize,
) {
    let Some(day) = days.get(idx) else {
        return;
    };
    let (plot, _) = CleanupChart::plot_layout(size);
    let (totals_mb, logs_mb, temp_mb, max_y) = CleanupChart::series_mb(days);
    let n = days.len().max(1);
    let x = CleanupChart::x_at(plot, n, idx);
    let y_total = CleanupChart::y_at(plot, max_y, totals_mb.get(idx).copied().unwrap_or(0.0));

    // Vertical guide + highlighted total point.
    frame.stroke(
        &Path::line(
            Point::new(x, plot.position().y),
            Point::new(x, plot.position().y + plot.size().height),
        ),
        Stroke::default()
            .with_width(1.0)
            .with_color(theme.text_muted.scale_alpha(0.55)),
    );
    frame.fill(&Path::circle(Point::new(x, y_total), 4.0), {
        if theme.dark {
            Color::from_rgb(0.863, 0.824, 0.627)
        } else {
            Color::from_rgb(0.627, 0.471, 0.157)
        }
    });

    let tip = format!(
        "{}\ntotal {:.2} MB\nlogs  {:.2} MB\ntemp  {:.2} MB",
        day.date,
        totals_mb.get(idx).copied().unwrap_or(0.0),
        logs_mb.get(idx).copied().unwrap_or(0.0),
        temp_mb.get(idx).copied().unwrap_or(0.0),
    );

    let tip_w = 138.0;
    let tip_h = 58.0;
    let mut tip_x = x + 10.0;
    let mut tip_y = y_total - tip_h - 8.0;
    if tip_x + tip_w > size.width - 4.0 {
        tip_x = x - tip_w - 10.0;
    }
    if tip_y < 4.0 {
        tip_y = y_total + 10.0;
    }
    if tip_y + tip_h > size.height - 4.0 {
        tip_y = (size.height - tip_h - 4.0).max(4.0);
    }

    let tip_bg = if theme.dark {
        Color::from_rgba(0.08, 0.09, 0.11, 0.94)
    } else {
        Color::from_rgba(1.0, 1.0, 1.0, 0.95)
    };
    let tip_border = theme.card_stroke;
    frame.fill_rectangle(Point::new(tip_x, tip_y), Size::new(tip_w, tip_h), tip_bg);
    frame.stroke(
        &Path::rectangle(Point::new(tip_x, tip_y), Size::new(tip_w, tip_h)),
        Stroke::default().with_width(1.0).with_color(tip_border),
    );
    frame.fill_text(chart_label(
        tip,
        Point::new(tip_x + 8.0, tip_y + 6.0),
        theme.text_strong,
        11.0,
        iced::widget::text::Alignment::Left,
        alignment::Vertical::Top,
    ));
}

fn dashboard(app: &App) -> Element<'_, Message> {
    let theme = ThemePaint::get(app.is_dark());
    let off = app.settings.iter().filter(|s| !s.active).count();
    let on = app.settings.iter().filter(|s| s.active).count();
    let total = app.settings.len().max(1);
    let blocked_frac = off as f32 / total as f32;

    let summary = if on == 0 {
        format!("{off}/{total} blocked — all quiet")
    } else if off == 0 {
        format!("{on}/{total} collecting — fully open")
    } else {
        format!("{off}/{total} blocked · {on} still collecting")
    };

    let chip_items: Vec<Element<Message>> = app
        .settings
        .iter()
        .map(|s| telemetry_chip(s, &theme))
        .collect();

    let (life_logs, life_temp) = cleanup_history::lifetime_totals();
    let today = cleanup_history::today_totals();

    section(
        column![
            wrap_row(vec![
                text("Dashboard").size(15).into(),
                small_btn("Refresh", Message::RefreshDashboard),
            ]),
            text("Telemetry status").size(12),
            progress_bar(blocked_frac, on, off, theme.track),
            text(summary).size(12).color(theme.text_muted),
            wrap_row(chip_items),
            rule::horizontal(1),
            text("Cleanup freed").size(12),
            text(format!(
                "today  logs {} · temp {} · total {}",
                cleanup_history::format_size(today.logs_bytes),
                cleanup_history::format_size(today.temp_bytes),
                cleanup_history::format_size(today.total_bytes()),
            ))
            .size(12)
            .color(off_color(theme.dark)),
            text(format!(
                "lifetime  logs {} · temp {} · total {}",
                cleanup_history::format_size(life_logs),
                cleanup_history::format_size(life_temp),
                cleanup_history::format_size(life_logs.saturating_add(life_temp)),
            ))
            .size(11)
            .color(theme.text_muted),
            text("Daily cleared (MB) — hover a point for exact values")
                .size(11)
                .color(theme.text_muted),
            Canvas::new(CleanupChart::new(app.cleanup_days.clone(), theme.clone()))
                .width(Length::Fill)
                .height(Length::Fixed(180.0)),
            wrap_row(vec![
                text("● total")
                    .size(11)
                    .color(if theme.dark {
                        Color::from_rgb(0.863, 0.824, 0.627)
                    } else {
                        Color::from_rgb(0.627, 0.471, 0.157)
                    })
                    .into(),
                text("● logs")
                    .size(11)
                    .color(Color::from_rgb(0.353, 0.706, 0.549))
                    .into(),
                text("● temp")
                    .size(11)
                    .color(Color::from_rgb(0.353, 0.549, 0.784))
                    .into(),
            ]),
        ]
        .spacing(6),
    )
}

fn progress_bar(blocked_frac: f32, on: usize, off: usize, track: Color) -> Element<'static, Message> {
    let fill = if on == 0 {
        Color::from_rgb(0.35, 0.71, 0.55)
    } else if off == 0 {
        Color::from_rgb(0.78, 0.47, 0.27)
    } else {
        Color::from_rgb(0.47, 0.67, 0.59)
    };
    // iced Length portions are integers — map fraction to 0..=100.
    let blocked = ((blocked_frac.clamp(0.0, 1.0) * 100.0).round() as u16).max(if blocked_frac > 0.0 {
        1
    } else {
        0
    });
    let rest = 100u16.saturating_sub(blocked);

    let mut segments = Row::new().height(Length::Fixed(10.0)).width(Length::Fill);
    if blocked > 0 {
        segments = segments.push(
            container(Space::new().width(Length::Fill).height(Length::Fill))
                .width(Length::FillPortion(blocked))
                .height(Length::Fill)
                .style(move |_theme: &Theme| container::Style {
                    background: Some(iced::Background::Color(fill)),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        );
    }
    if rest > 0 {
        segments = segments.push(Space::new().width(Length::FillPortion(rest)));
    }

    container(segments)
        .width(Length::Fill)
        .height(Length::Fixed(10.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced::Background::Color(track)),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn telemetry_chip(state: &SettingState, theme: &ThemePaint) -> Element<'static, Message> {
    let collecting = state.active;
    let badge = if collecting { "ON" } else { "OFF" };
    let badge_color = if collecting {
        on_color(theme.dark)
    } else {
        off_color(theme.dark)
    };
    let card = theme.card;
    let card_stroke = theme.card_stroke;
    let text_strong = theme.text_strong;
    let text_muted = theme.text_muted;
    let cli = state.id.cli_name();
    let title = state.id.short_title();

    container(
        row![
            text(badge).size(11).color(badge_color),
            column![
                text(title).size(12).color(text_strong),
                text(cli).size(10).color(text_muted),
            ]
            .spacing(2),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([8, 10])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(card)),
        border: iced::Border {
            color: card_stroke,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn hide_checkbox<'a>(hidden: bool, on_toggle: impl Fn(bool) -> Message + 'a) -> Element<'a, Message> {
    row![
        checkbox(hidden).on_toggle(on_toggle),
        text("Hide").size(12).color(Color::from_rgb(0.55, 0.55, 0.6)),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

fn verify_panel(app: &App) -> Element<'_, Message> {
    let theme = ThemePaint::get(app.is_dark());
    let stamp = if app.verify_stamp.is_empty() {
        "live status".to_string()
    } else {
        format!("as of {}", app.verify_stamp)
    };

    let mut col = column![
        wrap_row(vec![
            text("Verified values").size(14).into(),
            text(stamp).size(11).color(theme.text_muted).into(),
            small_btn("Re-check", Message::Verify),
            hide_checkbox(app.hide_verify, Message::HideVerify),
        ]),
        text(
            "Live values Windows reports for each field this app changes.",
        )
        .size(12)
        .color(theme.text_muted),
    ]
    .spacing(6)
    .width(Length::Fill);

    if !app.hide_verify {
        let rows: Vec<VerifyRow> = app
            .settings
            .iter()
            .map(|s| VerifyRow {
                setting: s.id.cli_name().to_string(),
                state: if s.active { "ON" } else { "OFF" }.to_string(),
                collecting: s.active,
                value: s.note.clone(),
            })
            .collect();

        if rows.is_empty() {
            col = col.push(
                text("No settings loaded yet.")
                    .size(11)
                    .color(theme.text_muted),
            );
        } else {
            col = col.push(verify_table(rows, theme.dark));
        }
    }

    section(col)
}

fn setting_card<'a>(app: &'a App, idx: usize, state: &SettingState) -> Element<'a, Message> {
    let id = state.id;
    let active = state.active;
    let dark = app.is_dark();
    let (badge, badge_color) = if active {
        ("ON", on_color(dark))
    } else {
        ("OFF", off_color(dark))
    };

    let mut action_items: Vec<Element<Message>> = vec![
        text(badge).size(12).color(badge_color).into(),
        column![
            text(id.title()).size(14),
            text(format!("cli: {}", id.cli_name()))
                .size(11)
                .color(Color::from_rgb(0.55, 0.55, 0.6)),
        ]
        .spacing(2)
        .into(),
    ];

    if app.elevated {
        // Switch shows current collecting state: ON (right) = collecting.
        let switch = row![
            text("OFF").size(11).color(off_color(dark)),
            toggler(active)
                .size(18)
                .on_toggle(move |want_on| Message::ToggleSetting {
                    id,
                    active: want_on,
                }),
            text("ON").size(11).color(on_color(dark)),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        action_items.push(switch.into());
    }

    action_items.push(small_btn("Verify", Message::VerifyOne(id)));
    action_items.push(
        text(if active { "Collecting" } else { "Blocked" })
            .size(11)
            .color(badge_color)
            .into(),
    );

    let open = app.expanded[idx];
    let detail_body = Some(
        column![
            text(id.explanation()).size(11),
            text(id.detail())
                .size(11)
                .color(Color::from_rgb(0.55, 0.55, 0.6)),
        ]
        .spacing(4)
        .width(Length::Fill)
        .into(),
    );

    section(
        column![
            wrap_row(action_items),
            text(format!("Value: {}", state.note)).size(11),
            accordion(
                "What is this?",
                None,
                open,
                &app.setting_anims[idx],
                app.anim_now,
                96.0,
                Message::ToggleExpand(idx),
                detail_body,
            ),
        ]
        .spacing(4),
    )
}

fn clear_logs_section(app: &App) -> Element<'_, Message> {
    let mut col = column![
        wrap_row(vec![
            text("Logging").size(14).into(),
            small_btn("Refresh status", Message::RefreshLogStatus),
        ]),
        text(
            "Use 📂 to open the log location. Clear asks for confirmation first. \
             Locked files are skipped. CLI: open-log <id> | clear <id> --confirm",
        )
        .size(11)
        .color(Color::from_rgb(0.55, 0.55, 0.6)),
    ]
    .spacing(6);

    if !app.elevated {
        col = col.push(
            text("Opening locations works without admin; clearing needs Administrator.")
                .size(11)
                .color(elevated_warn()),
        );
    }

    if !app.last_clear_report.is_empty() {
        col = col.push(
            text(&app.last_clear_report)
                .size(11)
                .color(off_color(app.is_dark())),
        );
    }

    let details: Element<'_, Message> = if app.hide_log_details {
        text("Size / count table hidden.")
            .size(11)
            .color(Color::from_rgb(0.55, 0.55, 0.6))
            .into()
    } else if !app.log_status.is_empty() {
        let rows: Vec<StatRow> = log_cleanup::ALL
            .iter()
            .filter_map(|action| {
                app.log_status.get(action.id).map(|st| StatRow {
                    target: action.id.to_string(),
                    count: st.files.to_string(),
                    size: log_cleanup::format_bytes(st.bytes),
                })
            })
            .collect();
        stats_table(rows)
    } else {
        text("Refresh status to load per-target sizes.")
            .size(11)
            .color(Color::from_rgb(0.55, 0.55, 0.6))
            .into()
    };

    col = col.push(
        column![
            wrap_row(vec![
                text("Size / count details").size(13).into(),
                hide_checkbox(app.hide_log_details, Message::HideLogDetails),
            ]),
            details,
        ]
        .spacing(4)
        .width(Length::Fill),
    );

    let mut btn_items: Vec<Element<Message>> = Vec::new();
    for action in log_cleanup::ALL {
        let available = app
            .clear_available
            .iter()
            .find(|(id, _)| *id == action.id)
            .map(|(_, a)| *a)
            .unwrap_or_else(|| action.is_available());
        let label = action.title.to_string();

        if available {
            btn_items.push(small_btn("📂", Message::OpenLog(action.id)));
            if app.elevated {
                let clear_btn = if action.dangerous {
                    danger_btn(text(label).size(11), Message::RequestClearLog(action.id))
                } else {
                    secondary_btn(text(label).size(11), Message::RequestClearLog(action.id))
                };
                btn_items.push(clear_btn.into());
            } else {
                btn_items.push(
                    text(label)
                        .size(11)
                        .color(Color::from_rgb(0.45, 0.45, 0.5))
                        .into(),
                );
            }
        }
    }
    col = col.push(wrap_row(btn_items));

    col = col.push(wrap_row(vec![
        if app.elevated {
            secondary_btn("Clear all safe logs", Message::RequestClearAllSafe).into()
        } else {
            text("Clear all safe logs").size(11).into()
        },
        if app.elevated {
            danger_btn("Clear all logs", Message::RequestClearAllLogs).into()
        } else {
            text("Clear all logs").size(11).into()
        },
    ]));

    section(col)
}

fn temp_section(app: &App) -> Element<'_, Message> {
    let mut col = column![
        wrap_row(vec![
            text("Temporary files").size(14).into(),
            small_btn("Refresh status", Message::RefreshTempStatus),
        ]),
        text(
            "Use 📂 to open a temp folder. Clear asks for confirmation first. \
             Reports files + space freed after clear. CLI: temp-list | clear-temp <id> --confirm",
        )
        .size(11)
        .color(Color::from_rgb(0.55, 0.55, 0.6)),
    ]
    .spacing(6);

    if !app.last_temp_report.is_empty() {
        col = col.push(
            text(&app.last_temp_report)
                .size(11)
                .color(off_color(app.is_dark())),
        );
    }

    let details: Element<'_, Message> = if app.hide_temp_details {
        text("Size / count table hidden.")
            .size(11)
            .color(Color::from_rgb(0.55, 0.55, 0.6))
            .into()
    } else if !app.temp_status.is_empty() {
        let rows: Vec<StatRow> = temp_cleanup::ALL
            .iter()
            .filter_map(|target| {
                app.temp_status.get(target.id).map(|st| StatRow {
                    target: target.id.to_string(),
                    count: st.files.to_string(),
                    size: log_cleanup::format_bytes(st.bytes),
                })
            })
            .collect();
        stats_table(rows)
    } else {
        text("Refresh status to load per-location sizes.")
            .size(11)
            .color(Color::from_rgb(0.55, 0.55, 0.6))
            .into()
    };

    col = col.push(
        column![
            wrap_row(vec![
                text("Size / count details").size(13).into(),
                hide_checkbox(app.hide_temp_details, Message::HideTempDetails),
            ]),
            details,
        ]
        .spacing(4)
        .width(Length::Fill),
    );

    let mut btn_items: Vec<Element<Message>> = Vec::new();
    for target in temp_cleanup::ALL {
        let available = app
            .temp_available
            .iter()
            .find(|(id, _)| *id == target.id)
            .map(|(_, a)| *a)
            .unwrap_or_else(|| target.is_available());
        if !available {
            continue;
        }
        let label = target.title.to_string();
        let clear_enabled = !target.needs_admin || app.elevated;

        btn_items.push(small_btn("📂", Message::OpenTemp(target.id)));
        if clear_enabled {
            btn_items.push(
                secondary_btn(text(label).size(11), Message::RequestClearTemp(target.id)).into(),
            );
        }
    }
    col = col.push(wrap_row(btn_items));

    let needs_admin = temp_cleanup::ALL
        .iter()
        .any(|t| t.is_available() && t.needs_admin);
    let all_enabled = !needs_admin || app.elevated;
    if all_enabled {
        col = col.push(secondary_btn(
            "Clear all temporary files",
            Message::RequestClearTempAll,
        ));
    }

    section(col)
}

fn links_section(app: &App) -> Element<'_, Message> {
    let mut col = column![
        wrap_row(vec![
            text("Windows logs & tools").size(14).into(),
            small_btn("Refresh availability", Message::RefreshLinks),
        ]),
        text(
            "Open built-in viewers and folders. Unavailable items are grayed out. \
             CLI: tluw logs | open <id>",
        )
        .size(11)
        .color(Color::from_rgb(0.55, 0.55, 0.6)),
    ]
    .spacing(6);

    let mut btn_items: Vec<Element<Message>> = Vec::new();
    for link in system_links::ALL {
        let available = app
            .link_available
            .iter()
            .find(|(id, _)| *id == link.id)
            .map(|(_, a)| *a)
            .unwrap_or_else(|| link.is_available());
        if available {
            btn_items.push(
                secondary_btn(text(link.title).size(11), Message::OpenLink(link.id)).into(),
            );
        } else {
            btn_items.push(
                text(link.title)
                    .size(11)
                    .color(Color::from_rgb(0.45, 0.45, 0.5))
                    .into(),
            );
        }
    }
    col = col.push(wrap_row(btn_items));

    section(col)
}
