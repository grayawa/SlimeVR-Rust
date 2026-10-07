use super::*;
use slimevr_gpui::visualization::{self, Camera};
impl SlimeView {
    pub(super) fn skeleton_panel(&self, height: f32, cx: &mut Context<Self>) -> AnyElement {
        self.skeleton_scene(height, true, cx)
    }
    pub(super) fn skeleton_scene(
        &self,
        height: f32,
        controls: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bones = self
            .snapshot
            .feed
            .as_ref()
            .map(|f| f.bones.clone())
            .unwrap_or_default();
        let camera = self.camera;
        let mirror = self.preferences.value["mirrorView"] == true;
        let mut panel = div()
            .v_flex()
            .gap_3()
            .p_3()
            .rounded_lg()
            .bg(cx.theme().muted);
        let mut views = div().h_flex().gap_2().flex_wrap();
        for (name, yaw, pitch) in [
            ("front", 0.0, 0.0),
            ("side", std::f32::consts::FRAC_PI_2, 0.0),
            ("top", 0.0, std::f32::consts::FRAC_PI_2),
        ] {
            views = views.child(
                Button::new(format!("camera-{name}"))
                    .small()
                    .label(self.text(&format!("native-camera-{name}")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.camera = Camera {
                            yaw,
                            pitch,
                            ..Default::default()
                        };
                        cx.notify();
                    })),
            );
        }
        panel =
            panel.child(views).child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(
                        Button::new("camera-rotate")
                            .small()
                            .label("↻")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.camera.yaw += std::f32::consts::FRAC_PI_6;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("camera-zoom-in")
                            .small()
                            .label("+")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.camera.zoom(12.0);
                                cx.notify();
                            })),
                    )
                    .child(Button::new("camera-zoom-out").small().label("−").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.camera.zoom(-12.0);
                            cx.notify();
                        }),
                    )),
            );
        let mut center = [0.0; 3];
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for bone in &bones {
            for point in [bone.head, bone.tail] {
                for axis in 0..3 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
            }
        }
        if !bones.is_empty() {
            center = [
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            ];
        }
        let span = if bones.is_empty() {
            2.0
        } else {
            (0..3).map(|a| max[a] - min[a]).fold(0.4, f32::max)
        };
        let floor = if bones.is_empty() { 0. } else { min[1].min(0.) };
        let area = div()
            .id("skeleton-canvas")
            .w_full()
            .h(px(height))
            .overflow_hidden()
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let delta = event.delta.pixel_delta(px(20.));
                this.camera.zoom(f32::from(delta.y) * 0.1);
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    if let Some(previous) = this.camera_drag.replace(event.position) {
                        this.camera.orbit(
                            f32::from(event.position.x - previous.x),
                            f32::from(event.position.y - previous.y),
                        );
                        cx.notify();
                    }
                } else if event.pressed_button == Some(MouseButton::Right) {
                    if let Some(previous) = this.camera_drag.replace(event.position) {
                        this.camera.pan[0] += f32::from(event.position.x - previous.x);
                        this.camera.pan[1] += f32::from(event.position.y - previous.y);
                        cx.notify();
                    }
                } else {
                    this.camera_drag = None;
                }
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let width = f32::from(bounds.size.width);
                        let height = f32::from(bounds.size.height);
                        let scale = ((height - 38.0) / span).min((width - 30.0) / (span * 0.55));
                        let project = |mut p: [f32; 3]| {
                            if mirror {
                                p[0] = -p[0];
                            }
                            let mut c = center;
                            if mirror {
                                c[0] = -c[0];
                            }
                            let p = camera.project(p, c, scale);
                            point(
                                bounds.origin.x + px(width * 0.5 + p[0]),
                                bounds.origin.y + px(height * 0.5 + p[1]),
                            )
                        };
                        let extent = if controls { 10 } else { 30 };
                        let grid_size = if controls { 1.5 } else { 7.5 };
                        for step in -extent..=extent {
                            let v = step as f32 * grid_size / extent as f32;
                            for (a, b) in [
                                ([v, floor, -grid_size], [v, floor, grid_size]),
                                ([-grid_size, floor, v], [grid_size, floor, v]),
                            ] {
                                let mut path = PathBuilder::stroke(px(1.0));
                                path.move_to(project(a));
                                path.line_to(project(b));
                                if let Ok(path) = path.build() {
                                    window.paint_path(path, rgb(0x2c2c6b));
                                }
                            }
                        }
                        let mut ordered = bones.clone();
                        ordered.sort_by(|a, b| {
                            camera.rotate(a.head)[2].total_cmp(&camera.rotate(b.head)[2])
                        });
                        for bone in ordered {
                            let mut path = PathBuilder::stroke(px(3.0));
                            path.move_to(project(bone.head));
                            path.line_to(project(bone.tail));
                            if let Ok(path) = path.build() {
                                let color =
                                    match BodyPart(bone.body).variant_name().unwrap_or("NONE") {
                                        "HEAD" => 0xffd700,
                                        "NECK" => 0xc0c0c0,
                                        "UPPER_CHEST" => 0x7fff00,
                                        "CHEST" => 0x800080,
                                        "WAIST" => 0xff0000,
                                        "HIP" => 0xffa500,
                                        n if n.ends_with("UPPER_LEG") => 0x7fff00,
                                        n if n.ends_with("LOWER_LEG") => 0x008080,
                                        n if n.ends_with("FOOT") => 0xffd700,
                                        n if n.ends_with("LOWER_ARM") => 0xff0000,
                                        n if n.ends_with("UPPER_ARM") => 0xcd5c5c,
                                        n if n.ends_with("HAND") => 0xff00ff,
                                        n if n.ends_with("SHOULDER") => 0x00ffff,
                                        _ => 0xb994d8,
                                    };
                                window.paint_path(path, rgb(color));
                            }
                        }
                    },
                )
                .size_full(),
            );
        if controls {
            panel.child(area).into_any_element()
        } else {
            area.into_any_element()
        }
    }
    pub(super) fn imu_native(&self, t: &Tracker, cx: &mut Context<Self>) -> AnyElement {
        let q = if self.preferences.value["devSettings"]["rawSlimeRotation"] == true {
            t.raw_rotation
        } else {
            t.rotation
        }
        .unwrap_or([0., 0., 0., 1.]);
        let acceleration = t.acceleration.unwrap_or([0.; 3]);
        let magnetic = t.magnetic.unwrap_or([0.; 3]);
        let extension = t.key.sensor != 0;
        let camera = Camera {
            yaw: 0.4,
            pitch: -0.15,
            ..Default::default()
        };
        div()
            .p_3()
            .bg(cx.theme().muted)
            .rounded_lg()
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let width = f32::from(bounds.size.width);
                        let height = f32::from(bounds.size.height);
                        let model = visualization::mesh(extension);
                        let scale = width.min(height) * 0.72;
                        let project = |p: [f32; 3]| {
                            let p = camera.project(
                                visualization::quaternion_rotate(p, q),
                                [0.; 3],
                                scale,
                            );
                            point(
                                bounds.origin.x + px(width * 0.5 + p[0]),
                                bounds.origin.y + px(height * 0.5 + p[1]),
                            )
                        };
                        let mut triangles: Vec<_> = model.iter().collect();
                        triangles.sort_by(|a, b| {
                            let depth = |t: &&visualization::Triangle| {
                                t.points
                                    .iter()
                                    .map(|p| {
                                        camera.rotate(visualization::quaternion_rotate(*p, q))[2]
                                    })
                                    .sum::<f32>()
                            };
                            depth(a).total_cmp(&depth(b))
                        });
                        for triangle in triangles {
                            let mut path = PathBuilder::fill();
                            path.move_to(project(triangle.points[0]));
                            path.line_to(project(triangle.points[1]));
                            path.line_to(project(triangle.points[2]));
                            path.close();
                            if let Ok(path) = path.build() {
                                let color = triangle.color;
                                let c = ((color[0] * 255.0) as u32) << 16
                                    | ((color[1] * 255.0) as u32) << 8
                                    | (color[2] * 255.0) as u32;
                                window.paint_path(path, rgb(c));
                            }
                        }
                        let origin = point(
                            bounds.origin.x + px(width * 0.5),
                            bounds.origin.y + px(height * 0.5),
                        );
                        for (vector, scale, color) in
                            [(acceleration, 0.03, 0xe6ab52), (magnetic, 0.001, 0x55cba4)]
                        {
                            let p = camera.project(vector, [0.; 3], width.min(height) * scale);
                            let mut path = PathBuilder::stroke(px(3.0));
                            path.move_to(origin);
                            path.line_to(point(origin.x + px(p[0]), origin.y + px(p[1])));
                            if let Ok(path) = path.build() {
                                window.paint_path(path, rgb(color));
                            }
                        }
                    },
                )
                .w_full()
                .h(px(260.)),
            )
            .into_any_element()
    }
}
