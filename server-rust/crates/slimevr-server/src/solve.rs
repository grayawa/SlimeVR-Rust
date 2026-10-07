use crate::{
    recording::{self, ReplayInput},
    runtime::print_json,
};
use slimevr_core::pose::{PoseConfig, PoseEngine, SceneInput};
use std::{
    error::Error,
    fs::File,
    io::{self, BufRead, BufReader, Read},
    path::Path,
};

const MAX_SCENE_LINE: u64 = 256 * 1024;
pub fn load_config(path: &Path) -> Result<PoseConfig, Box<dyn Error>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_SCENE_LINE {
        return Err(io::Error::other("pose config is too large").into());
    }
    let bytes = std::fs::read(path)?;
    let config: PoseConfig = match serde_json::from_slice(&bytes) {
        Ok(config) => config,
        Err(_) => crate::config::load(path)?.pose,
    };
    config.validate().map_err(io::Error::other)?;
    Ok(config)
}

pub fn scene(file: &Path, config: &Path, frames: bool) -> Result<(), Box<dyn Error>> {
    let mut engine = PoseEngine::new(load_config(config)?).map_err(io::Error::other)?;
    let mut reader = BufReader::new(File::open(file)?);
    let mut number = 0;
    loop {
        let mut line = Vec::new();
        let count = (&mut reader)
            .take(MAX_SCENE_LINE + 1)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        number += 1;
        if count as u64 > MAX_SCENE_LINE {
            return Err(io::Error::other(format!("scene line {number} exceeds limit")).into());
        }
        let input: SceneInput = serde_json::from_slice(&line)
            .map_err(|e| io::Error::other(format!("scene line {number}: {e}")))?;
        let tick = engine
            .scene_input(input)
            .map_err(|e| io::Error::other(format!("scene line {number}: {e}")))?;
        if tick && frames {
            print_json(engine.snapshot())?;
        }
    }
    if engine.snapshot().frame == 0 {
        return Err(io::Error::other("scene contains no solve tick").into());
    }
    if !frames {
        print_json(engine.snapshot())?;
    }
    Ok(())
}

pub fn journal(file: &Path, config: &Path, frames: bool) -> Result<(), Box<dyn Error>> {
    let mut engine = PoseEngine::new(load_config(config)?).map_err(io::Error::other)?;
    let replay = recording::replay_observed(file, |input| {
        match input {
            ReplayInput::Event(e) => engine.ingest(e)?,
            ReplayInput::PoseSetup(config) => {
                engine = PoseEngine::new(config.clone())?;
            }
            ReplayInput::Control(input) => {
                engine.scene_input(input.clone())?;
            }
            ReplayInput::Tick(at) => {
                engine.tick(at)?;
                if frames {
                    print_json(engine.snapshot()).map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    })?;
    if engine.snapshot().frame == 0 {
        engine.tick(replay.at_ms).map_err(io::Error::other)?;
    }
    if !frames {
        print_json(engine.snapshot())?;
    }
    print_json(
        &serde_json::json!({"type":"solve_complete","frames":engine.snapshot().frame,"verified_replies":replay.verified_replies,"at_ms":replay.at_ms}),
    )?;
    Ok(())
}

/// AutoBone writes only explicitly requested new files and only accepted fits.
pub fn autobone(
    file: &Path,
    config_path: &Path,
    settings: Option<&Path>,
    target: Option<f32>,
    output: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    use slimevr_core::autobone::{self, AutoBoneConfig};
    let mut config = load_config(config_path)?;
    let settings: AutoBoneConfig = if let Some(path) = settings {
        let f = File::open(path)?;
        if f.metadata()?.len() > MAX_SCENE_LINE {
            return Err(io::Error::other("AutoBone settings too large").into());
        }
        serde_json::from_reader(f)?
    } else {
        let source = std::fs::read(config_path)?;
        if serde_json::from_slice::<PoseConfig>(&source).is_ok() {
            AutoBoneConfig::default()
        } else {
            crate::config::load(config_path)?.auto_bone
        }
    };
    let frames = if file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pfs") || e.eq_ignore_ascii_case("pfr"))
    {
        crate::pose_recording::load(file)?.motion_frames()?
    } else {
        let mut engine = PoseEngine::new(config.clone()).map_err(io::Error::other)?;
        let mut reader = BufReader::new(File::open(file)?);
        let mut frames = Vec::new();
        loop {
            let mut line = Vec::new();
            let count = (&mut reader)
                .take(MAX_SCENE_LINE + 1)
                .read_until(b'\n', &mut line)?;
            if count == 0 {
                break;
            }
            if count as u64 > MAX_SCENE_LINE {
                return Err(io::Error::other("AutoBone scene line too large").into());
            }
            let tick = engine
                .scene_input(serde_json::from_slice(&line)?)
                .map_err(io::Error::other)?;
            if tick {
                let f = engine.motion_frame().map_err(io::Error::other)?;
                if frames
                    .last()
                    .is_none_or(|prev: &autobone::MotionFrame| prev.at_ms < f.at_ms)
                {
                    frames.push(f);
                }
                if frames.len() > 5000 {
                    return Err(io::Error::other("AutoBone scene exceeds 5000 frames").into());
                }
            }
        }
        frames
    };
    let result = autobone::optimize(
        config.skeleton,
        &frames,
        target.or(config.hmd_height),
        settings,
    )
    .map_err(io::Error::other)?;
    if let Some(path) = output {
        if !result.accepted {
            return Err(io::Error::other(format!(
                "AutoBone final error {} exceeds acceptance limit; configuration not written",
                result.final_error
            ))
            .into());
        }
        config.skeleton = result.skeleton;
        let source_bytes = std::fs::read(config_path)?;
        let pose_json = serde_json::from_slice::<PoseConfig>(&source_bytes).is_ok();
        let native_yaml = path
            .extension()
            .is_some_and(|ext| ext == "yml" || ext == "yaml")
            || !pose_json;
        let bytes = if native_yaml {
            let mut frontend = if pose_json {
                crate::api::FrontendConfig::default()
            } else {
                crate::config::load(config_path)?
            };
            frontend.pose = config;
            serde_yaml_ng::to_string(&crate::config::to_yaml(&frontend)?)?.into_bytes()
        } else {
            serde_json::to_vec_pretty(&config)?
        };
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        std::io::Write::write_all(&mut out, &bytes)?;
    }
    print_json(&serde_json::json!({"type":"autobone_result","result":result}))?;
    Ok(())
}
