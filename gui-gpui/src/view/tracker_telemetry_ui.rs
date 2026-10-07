use super::*;
use gpui_kit::component::tooltip::Tooltip;
use slimevr_gpui::battery::{Color, Marker, Reading};

impl SlimeView {
    pub(super) fn tracker_battery(
        &self,
        tracker: &Tracker,
        more_info: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(mut reading) = Reading::from_tracker(tracker) else {
            return div().into_any_element();
        };
        reading.disabled |= self.snapshot.connection != Connection::Connected;
        let debug = self.preferences.value["debug"] == true
            || self.preferences.value["devSettings"]["moreInfo"] == true;
        let background = rgb(0x3d6381);
        let foreground = match reading.color() {
            Color::Background => cx.theme().link,
            Color::Success => cx.theme().success,
            Color::Warning => cx.theme().warning,
            Color::Critical => cx.theme().danger,
            Color::Disabled => cx.theme().muted_foreground,
        };
        let icon = canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let p =
                    |x: f32, y: f32| point(bounds.origin.x + px(x), bounds.origin.y + px(y + 1.));
                // Original BatteryIcon silhouette (19 x 9 viewbox in a 19 x 11 icon).
                let mut path = PathBuilder::fill();
                path.move_to(p(1.31203, 0.));
                path.line_to(p(15.0005, 0.));
                path.cubic_bezier_to(p(16.1975, 1.134), p(15.6612, 0.), p(16.1945, 0.505976));
                path.line_to(p(16.1975, 2.558));
                path.line_to(p(17.9972, 2.558));
                path.line_to(p(17.9972, 5.968));
                path.line_to(p(16.1975, 5.968));
                path.line_to(p(16.1975, 7.391));
                path.cubic_bezier_to(p(15.0005, 8.525), p(16.1945, 8.01902), p(15.6612, 8.525));
                path.line_to(p(1.31203, 8.525));
                path.cubic_bezier_to(
                    p(0.115039, 7.383),
                    p(0.650395, 8.52315),
                    p(0.114754, 8.01458),
                );
                path.line_to(p(0.115039, 1.134));
                path.cubic_bezier_to(
                    p(1.31203, 0.),
                    p(0.116428, 0.505976),
                    p(0.650395, 0.00131561),
                );
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path.clone(), background);
                    if reading.fill() > 0. {
                        window.with_content_mask(
                            Some(ContentMask {
                                bounds: Bounds::new(
                                    p(0., 0.),
                                    size(px(reading.fill() * 18.), px(9.)),
                                ),
                            }),
                            |window| window.paint_path(path, foreground),
                        );
                    }
                }
                let ink = rgb(0x081e30);
                match reading.marker() {
                    Marker::None => (),
                    Marker::Charging => {
                        let mut bolt = PathBuilder::fill();
                        bolt.move_to(p(7.76384, 8.41896));
                        for (x, y) in [
                            (8.01123, 4.98346),
                            (5.77128, 4.98346),
                            (8.56441, 0.07978),
                            (8.31702, 3.51528),
                            (10.55696, 3.51528),
                        ] {
                            bolt.line_to(p(x, y));
                        }
                        bolt.close();
                        if let Ok(path) = bolt.build() {
                            window.paint_path(path, ink);
                        }
                    }
                    Marker::Charged => {
                        let mut check = PathBuilder::stroke(px(1.5));
                        check.move_to(p(5.53425, 4.62251));
                        check.cubic_bezier_to(
                            p(7.52534, 6.54522),
                            p(6.17778, 5.01062),
                            p(6.61315, 5.25165),
                        );
                        check.cubic_bezier_to(
                            p(10.6052, 1.55203),
                            p(8.43409, 4.40164),
                            p(8.78097, 3.66148),
                        );
                        if let Ok(path) = check.build() {
                            window.paint_path(path, ink);
                        }
                    }
                }
            },
        )
        .w(px(19.))
        .h(px(11.))
        .flex_shrink_0();
        let mut tooltip = format!("{}: {}%", self.text("native-battery"), reading.percent);
        if reading.charging() {
            tooltip.push_str(&format!(
                "\n{}",
                self.text(if reading.marker() == Marker::Charged {
                    "native-battery-charged"
                } else {
                    "native-battery-charging"
                })
            ));
        }
        if let Some(voltage) = reading.voltage {
            tooltip.push_str(&format!(
                "\n{}: {voltage:.2} V",
                self.text("native-battery-voltage")
            ));
        }
        if let Some(runtime) = reading.runtime_text() {
            tooltip.push_str(&format!(
                "\n{}: {runtime}",
                self.text("native-battery-runtime")
            ));
        }
        if reading.disabled {
            tooltip.push_str(&format!("\n{}", self.text("tracker-status-disconnected")));
        }
        let mut lines = reading.lines(debug);
        if more_info
            && debug
            && let Some(voltage) = reading.voltage
        {
            lines.push(format!("{voltage:.2} V"));
        }
        div()
            .id(format!(
                "tracker-battery-{}-{}",
                tracker.key.device, tracker.key.sensor
            ))
            .h_flex()
            .items_center()
            .gap_2()
            .flex_shrink_0()
            .text_xs()
            .text_color(if reading.disabled {
                cx.theme().muted_foreground
            } else {
                cx.theme().foreground
            })
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .child(icon)
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .min_w(px(28.))
                    .flex_shrink_0()
                    .children(lines)
                    .when(
                        reading.charging() && !(more_info && debug && reading.voltage.is_some()),
                        |d| {
                            d.child(
                                div()
                                    .w(px(20.))
                                    .h(px(4.))
                                    .rounded_full()
                                    .bg(cx.theme().muted_foreground),
                            )
                        },
                    ),
            )
            .into_any_element()
    }
    pub(super) fn tracker_wifi(
        &self,
        tracker: &Tracker,
        numeric: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        if tracker.rssi.is_none() && tracker.ping.is_none() {
            return div().into_any_element();
        }
        let disabled = tracker.status == TrackerStatus::DISCONNECTED.0;
        let rssi = if disabled { None } else { tracker.rssi };
        let ping = if disabled { None } else { tracker.ping };
        let signal = rssi
            .map(|v| format!("{v} dBm"))
            .unwrap_or_else(|| "—".into());
        let latency = ping
            .map(|v| format!("{v} ms"))
            .unwrap_or_else(|| "—".into());
        let tooltip = if disabled {
            self.text("tracker-status-disconnected")
        } else {
            format!(
                "{}: {signal}\n{}: {latency}",
                self.text("native-wifi-signal"),
                self.text("native-ping")
            )
        };
        // Same RSSI range and strict color boundaries as React's WifiIcon.
        let percent = rssi.map_or(0., |v| ((f32::from(v) + 95.) / 55.).clamp(0., 1.));
        let background = rgb(0x3d6381);
        let foreground = if disabled {
            cx.theme().muted_foreground
        } else if percent > 0.4 {
            cx.theme().success
        } else if percent > 0.2 {
            cx.theme().warning
        } else {
            cx.theme().danger
        };
        let icon = canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let p =
                    |x: f32, y: f32| point(bounds.origin.x + px(x), bounds.origin.y + px(y + 1.5));
                let mut path = PathBuilder::fill();
                // Original SlimeVR signal silhouette, in a 16 x 13 viewbox.
                path.move_to(p(7.799, 12.378));
                path.line_to(p(15.585, 2.678));
                path.cubic_bezier_to(p(7.793, 0.), p(13.3492, 0.95947), p(10.6129, 0.01903));
                path.cubic_bezier_to(p(0., 2.678), p(4.9725, 0.0172), p(2.23528, 0.95782));
                path.line_to(p(7.799, 12.378));
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path.clone(), background);
                    let fill = if disabled { 1. } else { percent };
                    if fill > 0. {
                        window.with_content_mask(
                            Some(ContentMask {
                                bounds: Bounds::new(
                                    p(0., 13. * (1. - fill)),
                                    size(px(16.), px(13. * fill)),
                                ),
                            }),
                            |window| window.paint_path(path, foreground),
                        );
                    }
                }
            },
        )
        .size(px(16.))
        .flex_shrink_0();
        div()
            .id(format!(
                "tracker-wifi-{}-{}",
                tracker.key.device, tracker.key.sensor
            ))
            .h_flex()
            .items_center()
            .gap_2()
            .flex_shrink_0()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .child(icon)
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .child(latency)
                    .when(numeric, |d| d.child(signal)),
            )
            .into_any_element()
    }
}
