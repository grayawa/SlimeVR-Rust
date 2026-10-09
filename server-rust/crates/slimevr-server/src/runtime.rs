mod budget;
mod ingress;
mod replies;
mod timing;
use crate::{
    api::{self, FrontendConfig, Service},
    log_level::LogLevel,
    logging::{self, write_json},
    protocol,
    receiver::{Effects, Receiver, ReceiverConfig},
    recording::{encode_hex, Record, Recorder},
};
use slimevr_core::pose::{PoseConfig, PoseEngine};
use std::{
    error::Error,
    io::{self, Write},
    net::SocketAddr,
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};
use tokio::{
    net::UdpSocket,
    time::{interval, MissedTickBehavior},
};

// Discovery leaves a ten-second quiet window so official firmware can time out
// its prior transport session and initiate a fresh handshake.
const DISCOVERY_INTERVAL_MS: u64 = 10_000;

enum Ready {
    Udp(io::Result<ingress::Datagram>),
    Tick,
    Shutdown(io::Result<()>),
    SteamVr(Option<crate::steamvr::Event>),
    Request(Option<api::Request>),
}

pub struct ListenOptions {
    pub bind: SocketAddr,
    pub bind_explicit: bool,
    pub config: ReceiverConfig,
    pub record: Option<PathBuf>,
    pub log_level: LogLevel,
    pub summary_ms: u64,
    pub timing_window_ms: u64,
    pub run_for: Option<Duration>,
    pub discovery: bool,
    pub discovery_targets: Vec<SocketAddr>,
    pub pose_config: Option<PoseConfig>,
    pub pose_ms: u64,
    pub pose_output_ms: u64,
    pub api_bind: Option<SocketAddr>,
    pub state: Option<PathBuf>,
    pub shutdown_on_stdin_eof: bool,
    pub no_steamvr: bool,
    pub steamvr_endpoint: Option<PathBuf>,
    pub bindings_provider: Option<PathBuf>,
    pub no_bindings_provider: bool,
    pub steamvr_driver: Option<PathBuf>,
    pub steamvr_runtime: Option<PathBuf>,
    pub steamvr_http: Option<String>,
    pub no_driver_install: bool,
    pub no_steamvr_restart: bool,
    pub serial_ports: Vec<String>,
}

pub fn print_json(value: &impl serde::Serialize) -> Result<(), Box<dyn Error>> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}

async fn apply_effects(
    socket: &UdpSocket,
    recorder: &mut Option<Recorder>,
    effects: Effects,
    at: u64,
    engine: &mut Option<PoseEngine>,
    api: &Option<Service>,
) -> Result<(), Box<dyn Error>> {
    for event in effects.events {
        if let (
            Some(api),
            slimevr_core::EventKind::TapSetup {
                device_key,
                sensor_id,
            },
        ) = (api, &event.kind)
        {
            if let Some(device) = api.config.device_ids.get(device_key) {
                let _ = api.events.send(api::Wire::Binary(api::protocol::tap_frame(
                    *device, *sensor_id,
                )));
            }
        }
        if let (Some(api), slimevr_core::EventKind::DevicePending { mac: Some(mac), .. }) =
            (api, &event.kind)
        {
            let _ = api
                .events
                .send(api::Wire::Binary(api::protocol::unknown_frame(mac)));
        }
        if let Some(engine) = engine {
            engine.ingest(&event).map_err(io::Error::other)?;
        }
        write_json(logging::event_level(&event.kind), &event)?;
    }
    for packet in effects.outbound {
        // Journal each UDP send intent for deterministic reply verification.
        if let Some(journal) = recorder {
            journal.write(&Record::Send {
                at_ms: at,
                to: packet.to,
                hex: encode_hex(&packet.bytes),
            })?;
        }
        if let Err(error) = socket.send_to(&packet.bytes, packet.to).await {
            write_json(
                LogLevel::Warn,
                &serde_json::json!({"type":"send_error", "at_ms":at, "to":packet.to, "error":error.to_string()}),
            )?;
        }
    }
    Ok(())
}

pub async fn listen(mut options: ListenOptions) -> Result<(), Box<dyn Error>> {
    logging::configure(options.log_level);
    let mut initial_config_save = None;
    if options.state.is_none() && (options.api_bind.is_some() || options.steamvr_endpoint.is_some())
    {
        options.state = Some(crate::config::default_path());
    }
    let mut api_config = if options.state.is_some() {
        let requested = options.state.as_ref().unwrap().clone();
        let mut path = requested.clone();
        if path.extension().is_some_and(|ext| ext == "json") {
            path = path.with_file_name("vrconfig.yml");
        }
        if !path.exists() && path.with_extension("yaml").exists() {
            path = path.with_extension("yaml");
        }
        let legacy = if requested.extension().is_some_and(|ext| ext == "json") {
            requested
        } else {
            path.with_file_name("rust-backend.json")
        };
        let mut config = if path.exists() {
            FrontendConfig::load(&path)?
        } else if legacy.exists() {
            FrontendConfig::load(&legacy)?
        } else {
            let mut config =
                crate::config::from_yaml(serde_yaml_ng::Value::Mapping(Default::default()))?;
            if let Some(pose) = options.pose_config.take() {
                config.pose = pose;
            }
            config
        };
        config
            .allowed_macs
            .extend(options.config.allowed_macs.iter().cloned());
        config.allowed_macs.sort();
        config.allowed_macs.dedup();
        config.validate().map_err(io::Error::other)?;
        let (saved, report) = crate::config::save_measured(&config, Some(&path));
        if saved.is_err() {
            if let Some(report) = report {
                report.emit();
            }
            saved?;
        } else {
            initial_config_save = report;
        }
        options.state = Some(path);
        if !options.bind_explicit {
            options.bind.set_port(config.tracker_port);
        }
        options.config.allowed_macs = config.allowed_macs.clone();
        options.pose_config = Some(config.pose.clone());
        Some(config)
    } else {
        None
    };
    let mut receiver = Receiver::new(options.config.clone()).map_err(io::Error::other)?;
    let mut engine = options
        .pose_config
        .map(PoseEngine::new)
        .transpose()
        .map_err(io::Error::other)?;
    let socket = Arc::new(UdpSocket::bind(options.bind).await?);
    socket.set_broadcast(options.discovery)?;
    let mut recorder = options
        .record
        .as_deref()
        .map(|path| {
            Recorder::create_version(
                path,
                &receiver.config,
                if options.api_bind.is_some() {
                    4
                } else if engine.is_some() {
                    2
                } else {
                    1
                },
            )
        })
        .transpose()?;
    if let (Some(recorder), Some(engine)) = (&mut recorder, &engine) {
        recorder.write(&Record::PoseSetup {
            at_ms: 0,
            config: Box::new(engine.export_config()),
        })?;
    }
    let (commands, mut requests) = tokio::sync::mpsc::channel(32);
    let (events, _) = tokio::sync::broadcast::channel(64);
    let pubsub_hub = api::pubsub::Hub::default();
    let mut api = api_config
        .take()
        .map(|c| Service::new(c, options.state.clone(), commands.clone(), events.clone()));
    if let Some(service) = &mut api {
        // Create the idle writer before tracking; no thread spawn on a pose tick.
        service.ensure_config_writer().map_err(io::Error::other)?;
    }
    if let Some(service) = &mut api {
        service.local_ip = if !options.bind.ip().is_unspecified() {
            options.bind.ip()
        } else {
            if_addrs::get_if_addrs()?
                .into_iter()
                .find_map(|i| match i.addr {
                    if_addrs::IfAddr::V4(v) if !v.ip.is_loopback() && v.broadcast.is_some() => {
                        Some(std::net::IpAddr::V4(v.ip))
                    }
                    _ => None,
                })
                .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
        };
    }
    if let Some(service) = &mut api {
        service.serial = Some(
            crate::serial::Controller::start(
                service.commands.clone(),
                options.serial_ports.clone(),
            )
            .map_err(io::Error::other)?,
        );
    }
    if let Some(service) = &mut api {
        service.hid = Some(
            crate::hid::Controller::start(
                service.commands.clone(),
                service.config.yaml["hidConfig"]["trackersOverHID"]
                    .as_bool()
                    .unwrap_or(false),
            )
            .map_err(io::Error::other)?,
        );
    }
    let mut osc_state = api
        .as_ref()
        .map(|s| crate::osc::State::new(s.config.osc.clone()))
        .transpose()
        .map_err(io::Error::other)?;
    let mut osc = api
        .as_ref()
        .map(|s| crate::osc::Controller::start(s.config.osc.clone(), s.commands.clone()));
    let hotkeys = api
        .as_ref()
        .map(|s| {
            crate::hotkeys::Controller::start(
                crate::hotkeys::Settings::read(&s.config.yaml).map_err(io::Error::other)?,
                s.commands.clone(),
            )
        })
        .transpose()?;
    let vrchat_task = api
        .as_ref()
        .map(|s| api::vrchat::monitor(s.commands.clone()));
    let diagnostics_task = api
        .as_ref()
        .map(|s| api::diagnostics::monitor(s.commands.clone()));
    let want_bridge = !options.no_steamvr
        && (options.steamvr_endpoint.is_some()
            || (options.api_bind.is_some()
                && cfg!(any(target_os = "windows", target_os = "linux"))));
    let mut steamvr = if want_bridge {
        Some(crate::steamvr::Bridge::start(
            &options
                .steamvr_endpoint
                .clone()
                .unwrap_or_else(crate::steamvr::default_endpoint),
            options.bindings_provider.clone(),
            options.no_bindings_provider,
            commands.clone(),
            events.clone(),
            pubsub_hub.clone(),
        )?)
    } else {
        None
    };
    if let (Some(service), Some(bridge)) = (&mut api, &steamvr) {
        service.steam_vr = bridge.state.status.clone();
    }
    let mut driver_task = None;
    if want_bridge {
        if let Some(service) = &mut api {
            let manager = crate::steamvr::manager::Manager::new(
                options.steamvr_driver.clone(),
                options.steamvr_runtime.clone(),
                options.steamvr_http.clone(),
                !options.no_driver_install,
                !options.no_steamvr_restart,
            )
            .map_err(io::Error::other)?;
            service.driver_manager = Some(manager.clone());
            let sender = service.commands.clone();
            driver_task = Some(tokio::spawn(async move {
                let registration = manager.register().await;
                use crate::steamvr::manager::{DriverNotice, RegistrationOutcome};
                let (registration_notice, registration_error) = match registration {
                    Ok(RegistrationOutcome::ExistingManualDriver) => {
                        (Some(DriverNotice::ExistingManualDriver), None)
                    }
                    Ok(_) => (None, None),
                    Err(error) => (None, Some(error)),
                };
                let mut clock = interval(Duration::from_secs(5));
                clock.set_missed_tick_behavior(MissedTickBehavior::Skip);
                loop {
                    clock.tick().await;
                    let mut status = manager.status().await.unwrap_or_default();
                    status.registration_error = registration_error.clone();
                    status.registration_notice = registration_notice;
                    if sender
                        .send(api::Request::DriverStatus {
                            status,
                            error: None,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
    }
    let mut live = None;
    let mut api_task = None;
    let mut api_address = None;
    if let (Some(bind), Some(service), Some(engine)) = (options.api_bind, &mut api, &engine) {
        let (publisher, subscriber) =
            tokio::sync::watch::channel(service.live(&receiver, engine, 0));
        let (address, task) = api::serve(bind, commands, subscriber, events, pubsub_hub).await?;
        api_address = Some(address);
        api_task = Some(task);
        live = Some(publisher);
    }
    let mut targets = options.discovery_targets;
    if options.discovery && targets.is_empty() {
        for interface in if_addrs::get_if_addrs()? {
            if let if_addrs::IfAddr::V4(v4) = interface.addr {
                if let Some(broadcast) = v4.broadcast {
                    targets.push(SocketAddr::new(broadcast.into(), 6969));
                }
            }
        }
        targets.sort();
        targets.dedup();
    }
    write_json(
        LogLevel::Info,
        &serde_json::json!({"type":"listening", "bind":socket.local_addr()?, "api_bind":api_address, "accept_new_devices":receiver.config.accept_new_devices,
        "allowed_macs":receiver.config.allowed_macs, "discovery_targets":targets,
        "steamvr_endpoint": steamvr.as_ref().and_then(|b| b.state.status.endpoint.as_ref())}),
    )?;
    // Preserve the ready/listening event as the first startup diagnostic.
    if let Some(report) = initial_config_save {
        report.emit();
    }
    let start = Instant::now();
    let period = Duration::from_millis(if engine.is_some() {
        options.pose_ms
    } else {
        50
    });
    let mut clock = interval(period);
    clock.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut timing = timing::Timing::new(
        start,
        period,
        Duration::from_millis(options.timing_window_ms),
        engine.is_some(),
    );
    let mut next_pose_output = 0;
    let mut next_summary = options.summary_ms;
    let mut next_discovery = DISCOVERY_INTERVAL_MS;
    let mut next_api_publish = 0;
    let ingress = ingress::Ingress::start(socket.clone(), period);
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    let parent_shutdown = async {
        if options.shutdown_on_stdin_eof {
            let (notify, receiver) = tokio::sync::oneshot::channel();
            // A detached OS thread avoids Tokio's non-cancellable stdin blocking task.
            std::thread::spawn(move || {
                let _ = io::copy(&mut io::stdin().lock(), &mut io::sink());
                let _ = notify.send(());
            });
            let _ = receiver.await;
        } else {
            std::future::pending::<()>().await;
        }
    };
    tokio::pin!(parent_shutdown);
    let mut prioritize_tick = true;
    let mut deferred_replies = replies::Replies::default();
    loop {
        let activity = async {
            tokio::select! {
                packet = ingress.recv() => Ready::Udp(packet),
                event = async { match &mut steamvr { Some(bridge) => bridge.events.recv().await, None => std::future::pending().await } }, if steamvr.is_some() => Ready::SteamVr(event),
                request = requests.recv(), if api.is_some() => Ready::Request(request),
            }
        };
        let ready = tokio::select! {
            biased;
            result = &mut shutdown => Ready::Shutdown(result),
            _ = &mut parent_shutdown => Ready::Shutdown(Ok(())),
            turn = budget::schedule(&mut clock, activity, prioritize_tick) => match turn {
                budget::Turn::Tick => Ready::Tick,
                budget::Turn::Activity(ready) => ready,
            },
        };
        prioritize_tick = !matches!(ready, Ready::Tick);
        match ready {
            Ready::Udp(packet) => {
                let batch_start = Instant::now();
                let budget = budget::Budget::new(batch_start, period);
                let mut packet = Some(packet?);
                let mut handled = 0;
                let mut yielded = false;
                while let Some(next) = packet {
                    let processed = Instant::now();
                    let at = processed.duration_since(start).as_millis() as u64;
                    let received_at_ms =
                        next.received.saturating_duration_since(start).as_millis() as u64;
                    timing.udp_received(processed.saturating_duration_since(next.received));
                    if let Some(journal) = &mut recorder {
                        journal.write(&Record::Receive {
                            at_ms: at,
                            received_at_ms: Some(received_at_ms),
                            from: next.from,
                            hex: encode_hex(&next.bytes),
                        })?;
                    }
                    let effects =
                        receiver.receive_parsed(next.from, next.parsed, at, Some(received_at_ms));
                    apply_effects(&socket, &mut recorder, effects, at, &mut engine, &api).await?;
                    handled += 1;
                    if budget.exhausted(handled, Instant::now()) {
                        yielded = true;
                        break;
                    }
                    packet = ingress.try_recv()?;
                }
                timing.udp_batch(batch_start.elapsed(), yielded);
            }
            Ready::Tick => {
                let at = start.elapsed().as_millis() as u64;
                let tick_started = Instant::now();
                timing.begin_tick(tick_started);
                if let Some(service) = &api {
                    if let Some(completion) = service.poll_config_save() {
                        deferred_replies.complete(completion);
                    }
                }
                if let (Some(bridge), Some(service), Some(engine)) =
                    (&mut steamvr, &mut api, &mut engine)
                {
                    for input in bridge.expire(at) {
                        dispatch_source(service, input, &mut receiver, engine)?;
                    }
                    bridge.provider_tick(at);
                    service.steamvr_status(bridge.state.status.clone(), engine.snapshot());
                }
                if let (Some(service), Some(engine)) = (&mut api, &mut engine) {
                    if let (Some(state), Some(osc)) = (&mut osc_state, &mut osc) {
                        if let Some(config) = service.take_osc_update() {
                            for event in state.reconfigure(config, at)? {
                                service.source_input(event, &mut receiver, engine)?;
                            }
                            osc.configure(state.generation, &state.config);
                        }
                    }
                    if let Some(keys) = &hotkeys {
                        if let Some(settings) = service.take_hotkeys_update()? {
                            keys.configure(settings);
                        }
                    }
                    service.device_tick(&mut receiver, at);
                    flush_device_commands(service, &socket, &mut recorder, at).await?;
                    service.before_tick(engine, at);
                    write_api_changes(service, &mut recorder, at)?;
                }
                if let Some(journal) = &mut recorder {
                    journal.write(&Record::Tick { at_ms: at })?;
                }
                let effects = receiver.tick(at);
                apply_effects(&socket, &mut recorder, effects, at, &mut engine, &api).await?;
                if let Some(engine) = &mut engine {
                    engine.tick(at).map_err(io::Error::other)?;
                    if logging::enabled(LogLevel::Debug) && at >= next_pose_output {
                        write_json(LogLevel::Debug, engine.snapshot())?;
                        next_pose_output = at.saturating_add(options.pose_output_ms);
                    }
                }
                if let (Some(bridge), Some(service), Some(engine)) =
                    (&mut steamvr, &mut api, &engine)
                {
                    service.steamvr_auto_share(engine.snapshot())?;
                    bridge.state.output(
                        &service.config,
                        engine.snapshot(),
                        &receiver,
                        &bridge.output,
                    );
                }
                if let (Some(service), Some(engine)) = (&mut api, &engine) {
                    if let (Some(state), Some(osc)) = (&mut osc_state, &osc) {
                        state.neck_height =
                            slimevr_core::autobone::skeleton_height(service.config.pose.skeleton)
                                - service.config.pose.skeleton.neck_length;
                        state.controller_arms = [
                            slimevr_core::skeleton::BodyPosition::LeftHand,
                            slimevr_core::skeleton::BodyPosition::RightHand,
                        ]
                        .into_iter()
                        .filter(|b| {
                            !service.config.pose.skeleton.force_arms_from_hmd
                                && (service
                                    .external
                                    .get(b)
                                    .is_some_and(|p| p.position.is_some())
                                    || engine
                                        .snapshot()
                                        .trackers
                                        .iter()
                                        .any(|p| p.body == *b && p.position.is_some()))
                        })
                        .collect();
                        match state.output(engine.snapshot(), at, engine.calibrated_head()) {
                            Ok(output) => osc.send(output),
                            Err(e) => service.error(e),
                        }
                    }
                    service.after_tick(engine, &receiver, at);
                    if at >= next_api_publish {
                        next_api_publish = at.saturating_add(10);
                        if let Some(live) = &live {
                            let snapshot_started = Instant::now();
                            let snapshot = service.live(&receiver, engine, at);
                            timing.live_snapshot(snapshot_started.elapsed());
                            live.send_replace(snapshot);
                        }
                    }
                }
                if options.discovery && at >= next_discovery && receiver.needs_discovery() {
                    next_discovery = at.saturating_add(DISCOVERY_INTERVAL_MS);
                    for target in &targets {
                        if let Err(error) = socket.send_to(&protocol::header(0, &[]), target).await
                        {
                            write_json(
                                LogLevel::Warn,
                                &serde_json::json!({"type":"discovery_error", "target":target, "error":error.to_string()}),
                            )?;
                        }
                    }
                }
                if at >= next_summary {
                    next_summary = at.saturating_add(options.summary_ms);
                    let stats = ingress.stats();
                    let coalesced = stats.coalesced.swap(0, Ordering::Relaxed);
                    let dropped_poses = stats.dropped_poses.swap(0, Ordering::Relaxed);
                    let dropped_controls = stats.dropped_controls.swap(0, Ordering::Relaxed);
                    timing.ingress_counts(coalesced, dropped_poses, dropped_controls);
                    if coalesced != 0 || dropped_poses != 0 || dropped_controls != 0 {
                        let level = if dropped_poses != 0 || dropped_controls != 0 {
                            LogLevel::Warn
                        } else {
                            LogLevel::Debug
                        };
                        logging::diagnostic(
                            level,
                            &serde_json::json!({"type":"udp_ingress_backpressure","at_ms":at,
                            "coalesced":coalesced,"dropped_poses":dropped_poses,"dropped_controls":dropped_controls}),
                        );
                    }
                    if logging::enabled(LogLevel::Debug) {
                        write_json(LogLevel::Debug, &receiver.snapshot(at))?;
                    }
                    if let Some(journal) = &mut recorder {
                        journal.flush()?;
                    }
                }
                let tick_finished = Instant::now();
                timing.end_tick(tick_finished.duration_since(tick_started));
                if let Some(mut report) =
                    timing.report(tick_finished, start.elapsed().as_millis() as u64, false)
                {
                    if let Some(bridge) = &steamvr {
                        report.steamvr_output = bridge.state.output_stats().take_window();
                    }
                    report_timing(&report);
                }
                if options
                    .run_for
                    .is_some_and(|duration| start.elapsed() >= duration)
                {
                    break;
                }
            }
            Ready::Shutdown(result) => {
                result?;
                break;
            }
            Ready::SteamVr(event) => {
                if event.is_none() {
                    if let (Some(mut bridge), Some(service), Some(engine)) =
                        (steamvr.take(), &mut api, &mut engine)
                    {
                        let at = start.elapsed().as_millis() as u64;
                        for input in bridge.state.disconnect(at) {
                            dispatch_source(service, input, &mut receiver, engine)?;
                        }
                        bridge.state.status.available = false;
                        bridge.state.status.last_error = Some("SteamVR transport stopped".into());
                        service.steamvr_status(bridge.state.status.clone(), engine.snapshot());
                        write_api_changes(service, &mut recorder, at)?;
                    }
                    continue;
                }
                if let (Some(event), Some(bridge), Some(service), Some(engine)) =
                    (event, &mut steamvr, &mut api, &mut engine)
                {
                    let at = start.elapsed().as_millis() as u64;
                    let current = bridge.state.current(&event);
                    let action = if let crate::steamvr::Event::Message(_, message) = &event {
                        if let Some(
                            crate::steamvr::messages::protobuf_message::Message::UserAction(action),
                        ) = &message.message
                        {
                            Some(action.name.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    match bridge.receive(event, at) {
                        Ok(inputs) => {
                            for input in inputs {
                                dispatch_source(service, input, &mut receiver, engine)?;
                            }
                        }
                        Err(error) => {
                            bridge.state.status.last_error = Some(error.clone());
                            write_json(
                                LogLevel::Warn,
                                &serde_json::json!({"type":"steamvr_error","message":error}),
                            )?;
                        }
                    }
                    if current && action.as_deref() == Some("pause_tracking") {
                        service.steamvr_input(
                            slimevr_core::pose::SceneInput::Pause {
                                at_ms: at,
                                paused: !engine.is_paused(),
                            },
                            engine,
                        )?;
                    } else if current && action.as_deref() == Some("feet_mounting_reset") {
                        service.steamvr_input(
                            slimevr_core::pose::SceneInput::ResetSelected {
                                at_ms: at,
                                kind: slimevr_core::calibration::ResetKind::Mounting,
                                bodies: [
                                    slimevr_core::skeleton::BodyPosition::LeftFoot,
                                    slimevr_core::skeleton::BodyPosition::RightFoot,
                                ]
                                .into_iter()
                                .collect(),
                            },
                            engine,
                        )?;
                    }
                    service.steamvr_status(bridge.state.status.clone(), engine.snapshot());
                    write_api_changes(service, &mut recorder, at)?;
                }
            }
            Ready::Request(request) => {
                let at = start.elapsed().as_millis() as u64;
                let request = if let Some(api::Request::Hid(event)) = request {
                    if let Some(journal) = &mut recorder {
                        journal.write(&Record::Hid {
                            at_ms: at,
                            event: event.clone(),
                        })?;
                    }
                    let effects = receiver.hid(&event, at);
                    apply_effects(&socket, &mut recorder, effects, at, &mut engine, &api).await?;
                    None
                } else {
                    request
                };
                if let (Some(request), Some(service), Some(engine)) =
                    (request, &mut api, &mut engine)
                {
                    match request {
                        api::Request::Hotkey { action, delay_ms } => {
                            if let Err(e) = service.hotkey(action, delay_ms, at) {
                                service.error(e);
                            }
                        }
                        api::Request::HotkeyError(error) => service.error(error),
                        api::Request::Vrchat(values) => service.vrchat = values,
                        api::Request::Osc(event) => {
                            if let Some(state) = &mut osc_state {
                                match event {
                                    crate::osc::Event::Datagram {
                                        generation,
                                        port,
                                        bytes,
                                    } if generation == state.generation => {
                                        match state.receive(
                                            port,
                                            &bytes,
                                            at,
                                            engine.calibrated_head(),
                                        ) {
                                            Ok(events) => {
                                                for event in events {
                                                    service.source_input(
                                                        event,
                                                        &mut receiver,
                                                        engine,
                                                    )?;
                                                }
                                            }
                                            Err(e) => service.error(e),
                                        }
                                        write_api_changes(service, &mut recorder, at)?;
                                    }
                                    crate::osc::Event::Error {
                                        generation,
                                        message,
                                    } if generation == state.generation => service.error(message),
                                    _ => {}
                                }
                            }
                        }
                        api::Request::Hid(_) => unreachable!(),
                        api::Request::Diagnostics(context) => {
                            service.diagnostics.udev = context.udev;
                            service.diagnostics.wayland = context.wayland;
                            service.diagnostics.public_networks = context.public_networks;
                            service.diagnostics.network_supported = context.network_supported;
                        }
                        api::Request::Firmware(event) => {
                            service.firmware_event(event, &mut receiver, at)
                        }
                        api::Request::Serial(event) => {
                            let previous = receiver.config.allowed_macs.clone();
                            service.serial_event(event, &mut receiver, at);
                            if previous != receiver.config.allowed_macs {
                                if let Some(recorder) = &mut recorder {
                                    recorder.write(&Record::Admission {
                                        at_ms: at,
                                        allowed_macs: receiver.config.allowed_macs.clone(),
                                    })?;
                                }
                            }
                        }
                        api::Request::Client { data, reply } => {
                            if deferred_replies.full() {
                                let _ = reply.send(vec![api::error_wire(
                                    "Too many pending configuration saves".into(),
                                )]);
                                continue;
                            }
                            let save_before = service.config_save_revision();
                            let previous = receiver.config.allowed_macs.clone();
                            let response = service.handle(data, &mut receiver, engine, at);
                            if previous != receiver.config.allowed_macs {
                                if let Some(recorder) = &mut recorder {
                                    recorder.write(&Record::Admission {
                                        at_ms: at,
                                        allowed_macs: receiver.config.allowed_macs.clone(),
                                    })?;
                                }
                            }
                            write_api_changes(service, &mut recorder, at)?;
                            flush_device_commands(service, &socket, &mut recorder, at).await?;
                            let save_after = service.config_save_revision();
                            if save_after > save_before {
                                deferred_replies.defer(save_after, response, reply);
                            } else {
                                let _ = reply.send(response);
                            }
                        }
                        api::Request::AutoBoneEpoch { epoch, total } => {
                            service.auto_epoch(&epoch, total)
                        }
                        api::Request::DriverStatus { status, error } => {
                            service.driver_update(status, error, engine.snapshot())
                        }
                        api::Request::AutoBoneDone(result) => service.auto_done(result),
                        api::Request::AutoBoneSaved {
                            result,
                            record,
                            count,
                        } => service.auto_saved(result, record, count),
                    }
                }
            }
        }
    }
    let at = start.elapsed().as_millis() as u64;
    let stats = ingress.stats();
    timing.ingress_counts(
        stats.coalesced.swap(0, Ordering::Relaxed),
        stats.dropped_poses.swap(0, Ordering::Relaxed),
        stats.dropped_controls.swap(0, Ordering::Relaxed),
    );
    if let Some(mut report) = timing.report(Instant::now(), at, true) {
        if let Some(bridge) = &steamvr {
            report.steamvr_output = bridge.state.output_stats().take_window();
        }
        report_timing(&report);
    }
    if let (Some(service), Some(engine)) = (&mut api, &mut engine) {
        service.before_tick(engine, at);
        write_api_changes(service, &mut recorder, at)?;
    }
    if let Some(journal) = &mut recorder {
        journal.write(&Record::Tick { at_ms: at })?;
    }
    let effects = receiver.tick(at);
    apply_effects(&socket, &mut recorder, effects, at, &mut engine, &api).await?;
    if let Some(engine) = &mut engine {
        engine.tick(at).map_err(io::Error::other)?;
        write_json(LogLevel::Debug, engine.snapshot())?;
    }
    let mut config_save_result = Ok(());
    if let (Some(service), Some(engine)) = (&mut api, &engine) {
        service.after_tick(engine, &receiver, at);
        service.finish_bvh();
        service.hid.take();
        service.serial.take();
        let saved = service.finish_config_save().await;
        if let Some(completion) = service.poll_config_save() {
            deferred_replies.complete(completion);
        }
        config_save_result = saved;
    }
    if let Some(journal) = &mut recorder {
        journal.finish(at)?;
    }
    if logging::enabled(LogLevel::Debug) {
        write_json(LogLevel::Debug, &receiver.snapshot(at))?;
    }
    write_json(
        LogLevel::Info,
        &serde_json::json!({"type":"stopped", "at_ms":at}),
    )?;
    drop(osc);
    if let Some(task) = vrchat_task {
        task.abort();
        let _ = task.await;
    }
    if let Some(task) = diagnostics_task {
        task.abort();
        let _ = task.await;
    }
    if let Some(task) = driver_task {
        task.abort();
    }
    if let Some(task) = api_task {
        task.abort();
    }
    config_save_result.map_err(|error| io::Error::other(error).into())
}

fn write_api_changes(
    service: &mut Service,
    recorder: &mut Option<Recorder>,
    at: u64,
) -> Result<(), Box<dyn Error>> {
    for device_key in std::mem::take(&mut service.forgotten_devices) {
        if let Some(recorder) = recorder {
            recorder.write(&Record::ForgetDevice {
                at_ms: at,
                device_key,
            })?;
        }
    }
    for input in std::mem::take(&mut service.changes) {
        if let Some(recorder) = recorder {
            recorder.write(&Record::Control { at_ms: at, input })?;
        }
    }
    Ok(())
}

async fn flush_device_commands(
    service: &mut Service,
    socket: &UdpSocket,
    recorder: &mut Option<Recorder>,
    at: u64,
) -> Result<(), Box<dyn Error>> {
    for (command, effects) in std::mem::take(&mut service.device_control.outbound) {
        if let Some(journal) = recorder {
            journal.write(&Record::DeviceConfig { at_ms: at, command })?;
        }
        for packet in effects.outbound {
            if let Some(journal) = recorder {
                journal.write(&Record::Send {
                    at_ms: at,
                    to: packet.to,
                    hex: encode_hex(&packet.bytes),
                })?;
            }
            if let Err(e) = socket.send_to(&packet.bytes, packet.to).await {
                service.error(format!("Unable to send tracker configuration: {e}"));
            }
        }
    }
    Ok(())
}

fn dispatch_source(
    service: &mut Service,
    input: slimevr_core::pose::SceneInput,
    receiver: &mut Receiver,
    engine: &mut PoseEngine,
) -> Result<(), String> {
    if let slimevr_core::pose::SceneInput::Input { event } = input {
        service.source_input(event, receiver, engine)
    } else {
        service.steamvr_input(input, engine)
    }
}

fn report_timing(report: &timing::Report) {
    let level = if report.runtime_stall.gt_50ms != 0
        || report.steamvr_output.steamvr_output_queue_full != 0
        || report.steamvr_output.steamvr_output_queue_closed != 0
        || report.steamvr_output.steamvr_output_write_failed != 0
    {
        LogLevel::Warn
    } else {
        LogLevel::Info
    };
    logging::diagnostic(level, report);
}
