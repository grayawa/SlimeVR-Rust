use super::*;
use slimevr_gpui::mounting;
pub(super) use slimevr_gpui::mounting::Target as MountTarget;
impl SlimeView {
    pub(super) fn mounting_body_active(&self) -> bool {
        (self.navigation.page == Page::Mounting
            || self.in_onboarding()
                && self.onboarding.step == slimevr_gpui::onboarding::Step::Mounting)
            && self.form_value("mounting-method", "choose") == "manual"
    }
    fn mounting_step(&self) -> u8 {
        self.form_value("mounting-step", "0").parse().unwrap_or(0)
    }
    fn set_mounting_step(&mut self, step: u8, cx: &mut Context<Self>) {
        self.mounting_wait = None;
        self.form_values
            .insert("mounting-step".into(), step.to_string());
        cx.notify();
    }
    fn mounting_back(&self, cx: &mut Context<Self>) -> Button {
        Button::new("mounting-back")
            .ghost()
            .label(self.text("onboarding-automatic_mounting-prev_step"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.mounting_wait = None;
                this.mounting_target = None;
                this.form_values
                    .insert("mounting-method".into(), "choose".into());
                cx.notify();
            }))
    }
    fn assigned_body(&self, width: f32, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let mirror = self.preferences.value["mirrorView"] != false;
        let mut points = slimevr_gpui::assignment::targets("all", mirror);
        points.retain(|p| {
            self.snapshot
                .feed
                .as_ref()
                .is_some_and(|f| f.trackers.iter().any(|t| !t.computed && t.body == p.body))
        });
        slimevr_gpui::assignment::position_labels(&mut points, height);
        self.body_diagram(width, height, points, cx)
    }
    pub(super) fn mounting_aligned(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let width = (f32::from(window.viewport_size().width)
            - if self.in_onboarding() { 32. } else { 150. })
        .max(300.);
        let height = (f32::from(window.viewport_size().height) - 90.).max(450.);
        let method = self.form_value("mounting-method", "choose");
        if method == "manual" {
            let narrow = width < 850.;
            let text_width = if narrow { width.min(650.) } else { 384. };
            let info = div()
                .v_flex()
                .gap_4()
                .w(px(text_width))
                .flex_shrink_0()
                .child(
                    div()
                        .text_2xl()
                        .font_bold()
                        .child(self.text("onboarding-manual_mounting")),
                )
                .child(self.paragraph("onboarding-manual_mounting-description", text_width, window))
                .child(self.tip("tips-find_tracker", text_width, window, cx))
                .child(
                    div()
                        .h_flex()
                        .gap_3()
                        .justify_between()
                        .child(self.mounting_back(cx))
                        .when(self.in_onboarding(), |d| {
                            d.child(
                                Button::new("setup-manual-next")
                                    .primary()
                                    .label(self.text("onboarding-manual_mounting-next"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.onboarding_to(
                                            slimevr_gpui::onboarding::Step::Height,
                                            cx,
                                        )
                                    })),
                            )
                        }),
                );
            let body = self.assigned_body(
                if narrow {
                    width
                } else {
                    width.min(840.) - text_width - 24.
                },
                height,
                cx,
            );
            return div()
                .flex()
                .items_center()
                .justify_center()
                .gap_6()
                .when(narrow, |d| d.flex_col())
                .when(!narrow, |d| d.flex_row())
                .child(info)
                .child(body)
                .into_any_element();
        }
        if method == "automatic" {
            return self.automatic_mounting(width, height, window, cx);
        }
        let card_width = if width < 720. { width.min(500.) } else { 327. };
        let mut choices = div()
            .flex()
            .gap_4()
            .when(width < 720., |d| d.flex_col())
            .when(width >= 720., |d| d.flex_row());
        for (method, label, button_id) in [
            (
                "automatic",
                "auto_mounting",
                "onboarding-manual_mounting-auto_mounting",
            ),
            (
                "manual",
                "manual_mounting",
                "onboarding-automatic_mounting-manual_mounting",
            ),
        ] {
            let button = Button::new(format!("mounting-choice-{method}"))
                .label(self.text(button_id))
                .when(method == "automatic", |b| b.primary())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.form_values
                        .insert("mounting-method".into(), method.into());
                    this.set_mounting_step(0, cx);
                }));
            choices = choices.child(
                div()
                    .relative()
                    .v_flex()
                    .gap_4()
                    .p_4()
                    .w(px(card_width))
                    .rounded_lg()
                    .bg(cx.theme().popover)
                    .when(method == "automatic", |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(-20.))
                                .left(px(-16.))
                                .p_2()
                                .rounded_lg()
                                .bg(cx.theme().primary)
                                .text_color(cx.theme().primary_foreground)
                                .child(
                                    self.text("onboarding-choose_mounting-auto_mounting-label-v2"),
                                ),
                        )
                    })
                    .when(method == "manual", |d| {
                        d.child(
                            crate::ui_assets::image("slime/boxslime.webp", cx)
                                .id("mounting-box-slime")
                                .absolute()
                                .right(px(-8.))
                                .top(px(-40.))
                                .w(px(100.))
                                .h(px(78.)),
                        )
                    })
                    .child(
                        div()
                            .text_2xl()
                            .font_bold()
                            .child(self.text(&format!("onboarding-choose_mounting-{label}"))),
                    )
                    .child(self.paragraph(
                        &format!("onboarding-choose_mounting-{label}-description"),
                        card_width - 32.,
                        window,
                    ))
                    .child(div().h_flex().items_start().child(button)),
            );
        }
        div()
            .v_flex()
            .min_h(px(height))
            .justify_center()
            .items_center()
            .gap_8()
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .w(px(if width < 720. { card_width } else { 670. }))
                    .child(
                        div()
                            .text_2xl()
                            .font_bold()
                            .child(self.text("onboarding-choose_mounting")),
                    )
                    .child(self.paragraph(
                        "onboarding-choose_mounting-description",
                        width.min(670.),
                        window,
                    )),
            )
            .child(choices)
            .when(self.in_onboarding(), |d| {
                d.child(
                    Button::new("setup-mounting-prev")
                        .label(self.text("onboarding-previous_step"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.onboarding_to(slimevr_gpui::onboarding::Step::Assignment, cx)
                        })),
                )
            })
            .into_any_element()
    }
    fn wizard_reset(&self, kind: ResetKind, next: u8, cx: &mut Context<Self>) -> Button {
        let busy =
            self.mounting_wait.is_some() || self.snapshot.reset.as_ref().is_some_and(|r| !r.done);
        let permitted =
            kind != ResetKind::Mounting || self.snapshot.feed.as_ref().is_some_and(|f| f.can_mount);
        Button::new(format!("mounting-wizard-reset-{kind:?}"))
            .primary()
            .label(self.text(kind.label()))
            .disabled(busy || !permitted || self.snapshot.connection != Connection::Connected)
            .on_click(cx.listener(move |this, _, _, cx| {
                match this.client.send(Command::Reset(kind)) {
                    Ok(tx) => this.mounting_wait = Some((this.snapshot.session, tx, next)),
                    Err(e) => this.ui_error = Some(e),
                };
                cx.notify();
            }))
    }
    fn automatic_mounting(
        &self,
        width: f32,
        height: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Warm the original pose assets while the user checks their trackers.
        // Each image is decoded once off-thread and kept at display resolution.
        for path in [
            "slime/mounting-reset-pose.webp",
            "slime/reset/FullResetPose.webp",
            "slime/reset/FullResetPoseSide.webp",
            "slime/reset/FullResetPoseWrong.webp",
        ] {
            let _ = crate::ui_assets::image(path, cx);
        }
        let step = self.mounting_step().min(3);
        let body_width = width.min(768.);
        let narrow = body_width < 680.;
        let heading = div()
            .v_flex()
            .gap_2()
            .w_full()
            .child(
                div()
                    .text_2xl()
                    .font_bold()
                    .child(self.text("onboarding-automatic_mounting-title")),
            )
            .child(self.paragraph(
                "onboarding-automatic_mounting-description",
                body_width,
                window,
            ))
            .child(div().h_flex().gap_2().children((0..4).map(|i| {
                div().h(px(4.)).flex_1().rounded_full().bg(if i <= step {
                    cx.theme().primary
                } else {
                    cx.theme().secondary
                })
            })));
        let prev = Button::new("mounting-wizard-prev")
            .ghost()
            .label(self.text("onboarding-automatic_mounting-prev_step"))
            .disabled(self.mounting_wait.is_some())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.set_mounting_step(step.saturating_sub(1), cx)
            }));
        let body: AnyElement = match step {
            0 => div()
                .flex()
                .items_center()
                .gap_4()
                .when(narrow, |d| d.flex_col())
                .when(!narrow, |d| d.flex_row())
                .child(
                    div()
                        .v_flex()
                        .gap_4()
                        .w(px(if narrow { body_width } else { 300. }))
                        .child(div().text_2xl().font_bold().child(
                            self.text("onboarding-automatic_mounting-put_trackers_on-title"),
                        ))
                        .child(self.paragraph(
                            "onboarding-automatic_mounting-put_trackers_on-description",
                            if narrow { body_width } else { 300. },
                            window,
                        ))
                        .child(self.tip(
                            "tips-find_tracker",
                            if narrow { body_width } else { 300. },
                            window,
                            cx,
                        ))
                        .child(
                            div()
                                .h_flex()
                                .gap_3()
                                .flex_wrap()
                                .child(self.mounting_back(cx))
                                .child(
                                    Button::new("mounting-put-on-next")
                                        .primary()
                                        .label(self.text(
                                            "onboarding-automatic_mounting-put_trackers_on-next",
                                        ))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.set_mounting_step(1, cx)
                                        })),
                                ),
                        ),
                )
                .child(self.assigned_body(
                    if narrow {
                        body_width
                    } else {
                        body_width - 316.
                    },
                    (height - 140.).clamp(380., 540.),
                    cx,
                ))
                .into_any_element(),
            1 => {
                let mut prep = div().v_flex().gap_4().child(
                    div()
                        .text_2xl()
                        .font_bold()
                        .child(self.text("onboarding-automatic_mounting-preparation-title")),
                );
                for i in 0..3 {
                    prep = prep.child(self.paragraph(
                        &format!("onboarding-automatic_mounting-preparation-v2-step-{i}"),
                        body_width,
                        window,
                    ));
                }
                let mut pictures = div().h_flex().gap_4();
                for (i, path) in ["FullResetPose", "FullResetPoseSide", "FullResetPoseWrong"]
                    .into_iter()
                    .enumerate()
                {
                    pictures = pictures.child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .font_bold()
                                    .text_2xl()
                                    .text_color(if i == 2 {
                                        cx.theme().danger
                                    } else {
                                        rgb(0x50e897).into()
                                    })
                                    .child(if i == 2 { "×" } else { "✓" }),
                            )
                            .child(
                                crate::ui_assets::image(&format!("slime/reset/{path}.webp"), cx)
                                    .id(format!("mounting-prep-pose-{i}"))
                                    .w(px((body_width - 32.) / 3.))
                                    .h(px((height - 320.).clamp(180., 350.))),
                            ),
                    );
                }
                prep.child(pictures)
                    .child(div().h_flex().gap_3().child(prev).child(self.wizard_reset(
                        ResetKind::Full,
                        2,
                        cx,
                    )))
                    .into_any_element()
            }
            2 => {
                div()
                    .flex()
                    .gap_4()
                    .items_center()
                    .when(narrow, |d| d.flex_col())
                    .when(!narrow, |d| d.flex_row())
                    .child(
                        div()
                            .v_flex()
                            .gap_4()
                            .w(px(if narrow { body_width } else { 300. }))
                            .child(div().text_2xl().font_bold().child(
                                self.text("onboarding-automatic_mounting-mounting_reset-title"),
                            ))
                            .child(self.paragraph(
                                "onboarding-automatic_mounting-mounting_reset-step-0",
                                if narrow { body_width } else { 300. },
                                window,
                            ))
                            .child(self.paragraph(
                                "onboarding-automatic_mounting-mounting_reset-step-1",
                                if narrow { body_width } else { 300. },
                                window,
                            ))
                            .child(div().h_flex().gap_3().child(prev).child(self.wizard_reset(
                                ResetKind::Mounting,
                                3,
                                cx,
                            ))),
                    )
                    .child(
                        crate::ui_assets::image("slime/mounting-reset-pose.webp", cx)
                            .id("mounting-wizard-ski")
                            .w(px(if narrow {
                                body_width
                            } else {
                                body_width - 316.
                            }))
                            .h(px((height - 170.).clamp(250., 480.))),
                    )
                    .into_any_element()
            }
            _ => div()
                .v_flex()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .text_2xl()
                        .font_bold()
                        .child(self.text("onboarding-automatic_mounting-done-title")),
                )
                .child(self.text("onboarding-automatic_mounting-done-description"))
                .child(
                    div()
                        .h_flex()
                        .gap_3()
                        .child(
                            Button::new("mounting-restart")
                                .ghost()
                                .label(self.text("onboarding-automatic_mounting-done-restart"))
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.set_mounting_step(0, cx)),
                                ),
                        )
                        .child(
                            Button::new("mounting-finish")
                                .primary()
                                .label(self.text(if self.in_onboarding() {
                                    "onboarding-automatic_mounting-next"
                                } else {
                                    "onboarding-automatic_mounting-return-home"
                                }))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.in_onboarding() {
                                        this.onboarding_to(
                                            slimevr_gpui::onboarding::Step::Height,
                                            cx,
                                        );
                                    } else {
                                        this.go(Page::Home, cx);
                                    }
                                })),
                        ),
                )
                .child(div().w_full().child(self.skeleton_scene(
                    (height - 300.).max(250.),
                    false,
                    cx,
                )))
                .into_any_element(),
        };
        div()
            .v_flex()
            .items_center()
            .gap_5()
            .min_h(px(height))
            .child(
                div()
                    .v_flex()
                    .gap_6()
                    .w(px(body_width))
                    .child(heading)
                    .child(body),
            )
            .into_any_element()
    }
    pub(super) fn mounting_selection(
        &self,
        target: MountTarget,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let trackers: Vec<_> = self
            .snapshot
            .feed
            .as_ref()
            .map(|f| {
                f.trackers
                    .iter()
                    .filter(|t| mounting::matches(target, t))
                    .collect()
            })
            .unwrap_or_default();
        let selected = trackers
            .first()
            .and_then(|t| t.mounting)
            .and_then(mounting::direction)
            .filter(|d| {
                trackers
                    .iter()
                    .all(|t| t.mounting.and_then(mounting::direction) == Some(*d))
            });
        let side = (f32::from(window.viewport_size().width) - 32.).clamp(260., 400.);
        let radius = side * 0.396;
        let sector = cx.theme().muted_foreground.opacity(0.35);
        let marker = cx.theme().background;
        let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::default()));
        let prepaint = bounds.clone();
        let wheel = div()
            .id("mounting-wheel")
            .relative()
            .size(px(side))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    let b = bounds.get();
                    let x = f32::from(event.position.x - b.origin.x) - side / 2.;
                    let y = f32::from(event.position.y - b.origin.y) - side / 2.;
                    if let Some(direction) = mounting::hit(x, y, radius) {
                        this.apply_mounting(target, direction, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .child(
                canvas(
                    move |b, _, _| {
                        prepaint.set(b);
                    },
                    move |b, _, window, _| {
                        for i in 0..8 {
                            let mut path = PathBuilder::fill();
                            let origin =
                                point(b.origin.x + px(side / 2.), b.origin.y + px(side / 2.));
                            path.move_to(origin);
                            let center = i as f32 * std::f32::consts::FRAC_PI_4
                                - std::f32::consts::FRAC_PI_2;
                            for k in 0..=24 {
                                let angle = center - std::f32::consts::FRAC_PI_8
                                    + 0.016
                                    + (std::f32::consts::FRAC_PI_4 - 0.032) * k as f32 / 24.;
                                path.line_to(point(
                                    origin.x + px(angle.cos() * radius),
                                    origin.y + px(angle.sin() * radius),
                                ));
                            }
                            path.close();
                            if let Ok(path) = path.build() {
                                window.paint_path(path, sector);
                            }
                            if selected == Some(i) {
                                let mut marker_path = PathBuilder::fill();
                                let tangent = [-center.sin(), center.cos()];
                                let radial = [center.cos(), center.sin()];
                                for (k, (x, y)) in [(-21., -6.), (21., -6.), (21., 6.), (-21., 6.)]
                                    .into_iter()
                                    .enumerate()
                                {
                                    let p = point(
                                        origin.x
                                            + px(radial[0] * radius * 0.55
                                                + tangent[0] * x
                                                + radial[0] * y),
                                        origin.y
                                            + px(radial[1] * radius * 0.55
                                                + tangent[1] * x
                                                + radial[1] * y),
                                    );
                                    if k == 0 {
                                        marker_path.move_to(p);
                                    } else {
                                        marker_path.line_to(p);
                                    }
                                }
                                marker_path.close();
                                if let Ok(path) = marker_path.build() {
                                    window.paint_path(path, marker);
                                }
                            }
                        }
                    },
                )
                .size_full(),
            )
            .child(
                svg()
                    .path("slime/Foot.svg")
                    .absolute()
                    .left(px(side / 2. - side * 0.25))
                    .top(px(side / 2. - side * 0.25))
                    .size(px(side * 0.5))
                    .text_color(cx.theme().foreground),
            );
        let mut labels = div().relative().size(px(side)).child(wheel);
        for (i, (label, _)) in mounting::DIRECTIONS.iter().enumerate() {
            let angle = i as f32 * std::f32::consts::FRAC_PI_4 - std::f32::consts::FRAC_PI_2;
            labels = labels.child(
                Button::new(format!("mounting-direction-{i}"))
                    .ghost()
                    .small()
                    .tooltip(self.text(&format!("tracker-rotation-{label}")))
                    .absolute()
                    .left(px(side / 2.
                        + angle.cos() * radius * if i % 2 == 0 { 1.1 } else { 0.75 }
                        - 32.))
                    .top(px(side / 2.
                        + angle.sin() * radius * if i % 2 == 0 { 1.1 } else { 0.75 }
                        - 14.))
                    .w(px(64.))
                    .child(div().text_2xl().font_bold().child(if i % 2 == 0 {
                        self.text(&format!("tracker-rotation-{label}"))
                    } else {
                        String::new()
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply_mounting(target, i, cx);
                        cx.stop_propagation();
                    })),
            );
        }
        div()
            .id("mounting-overlay")
            .absolute()
            .inset_0()
            .bg(cx.theme().background.opacity(0.9))
            .v_flex()
            .items_center()
            .justify_start()
            .pt(px(80.))
            .gap_5()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.mounting_target = None;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .id("mounting-dialog")
                    .v_flex()
                    .items_center()
                    .gap_4()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_2xl()
                            .font_bold()
                            .child(self.text("mounting_selection_menu")),
                    )
                    .child(labels),
            )
            .child(
                div().absolute().left(px(40.)).bottom(px(40.)).child(
                    Button::new("mounting-close")
                        .primary()
                        .label(self.text("mounting_selection_menu-close"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.mounting_target = None;
                            cx.notify();
                        })),
                ),
            )
            .into_any_element()
    }
    fn apply_mounting(&mut self, target: MountTarget, direction: usize, cx: &mut Context<Self>) {
        let result = self
            .snapshot
            .feed
            .as_ref()
            .ok_or_else(|| self.text("native-disconnected"))
            .and_then(|f| mounting::requests(&f.trackers, target, direction));
        match result {
            Ok(r) => {
                self.batch(r, cx);
                if self.ui_error.is_none() {
                    self.mounting_target = None;
                }
            }
            Err(e) => self.ui_error = Some(e),
        };
        cx.notify();
    }
}
