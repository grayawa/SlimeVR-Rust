//! Original Java DataInput/OutputStream PFS and PFR formats.
use slimevr_core::{
    autobone::MotionFrame,
    skeleton::{BodyPosition as B, HeadPose},
    Quaternion as Q, Vector3 as V,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
};
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FRAMES: usize = 5000;
const BODIES: [B; 50] = [
    B::Head,
    B::Neck,
    B::UpperChest,
    B::Chest,
    B::Waist,
    B::Hip,
    B::LeftUpperLeg,
    B::RightUpperLeg,
    B::LeftLowerLeg,
    B::RightLowerLeg,
    B::LeftFoot,
    B::RightFoot,
    B::LeftLowerArm,
    B::RightLowerArm,
    B::LeftUpperArm,
    B::RightUpperArm,
    B::LeftHand,
    B::RightHand,
    B::LeftShoulder,
    B::RightShoulder,
    B::LeftThumbMetacarpal,
    B::LeftThumbProximal,
    B::LeftThumbDistal,
    B::LeftIndexProximal,
    B::LeftIndexIntermediate,
    B::LeftIndexDistal,
    B::LeftMiddleProximal,
    B::LeftMiddleIntermediate,
    B::LeftMiddleDistal,
    B::LeftRingProximal,
    B::LeftRingIntermediate,
    B::LeftRingDistal,
    B::LeftLittleProximal,
    B::LeftLittleIntermediate,
    B::LeftLittleDistal,
    B::RightThumbMetacarpal,
    B::RightThumbProximal,
    B::RightThumbDistal,
    B::RightIndexProximal,
    B::RightIndexIntermediate,
    B::RightIndexDistal,
    B::RightMiddleProximal,
    B::RightMiddleIntermediate,
    B::RightMiddleDistal,
    B::RightRingProximal,
    B::RightRingIntermediate,
    B::RightRingDistal,
    B::RightLittleProximal,
    B::RightLittleIntermediate,
    B::RightLittleDistal,
];
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Frame {
    pub body: Option<B>,
    pub rotation: Option<Q>,
    pub position: Option<V>,
    pub acceleration: Option<V>,
    pub raw_rotation: Option<Q>,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Tracker {
    pub name: String,
    pub frames: Vec<Frame>,
}
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Recording {
    pub interval: f32,
    pub trackers: Vec<Tracker>,
}
impl Recording {
    pub fn from_motion(frames: &[MotionFrame], interval: f32) -> Self {
        let mut bodies = std::collections::BTreeSet::from([B::Head]);
        for f in frames {
            bodies.extend(f.rotations.keys().copied());
            bodies.extend(f.positions.keys().copied());
        }
        let trackers = bodies
            .into_iter()
            .map(|body| Tracker {
                name: format!(
                    "body:{}",
                    serde_json::to_value(body).unwrap().as_str().unwrap()
                ),
                frames: frames
                    .iter()
                    .map(|f| Frame {
                        body: Some(body),
                        rotation: if body == B::Head {
                            Some(f.head.rotation)
                        } else {
                            f.rotations.get(&body).copied()
                        },
                        position: if body == B::Head {
                            f.head.position
                        } else {
                            f.positions.get(&body).copied()
                        },
                        ..Default::default()
                    })
                    .collect(),
            })
            .collect();
        Self { interval, trackers }
    }
    pub fn motion_frames(&self) -> io::Result<Vec<MotionFrame>> {
        let count = self
            .trackers
            .iter()
            .map(|t| t.frames.len())
            .max()
            .unwrap_or(0);
        if count < 3 {
            return Err(invalid("AutoBone recording requires at least 3 frames"));
        }
        let mut states = vec![Frame::default(); self.trackers.len()];
        let mut result = Vec::with_capacity(count);
        for index in 0..count {
            let mut rotations = BTreeMap::new();
            let mut positions = BTreeMap::new();
            let mut head = None;
            for (tracker, state) in self.trackers.iter().zip(&mut states) {
                if let Some(frame) = tracker
                    .frames
                    .get(index.min(tracker.frames.len().saturating_sub(1)))
                {
                    if frame.body.is_some() {
                        state.body = frame.body;
                    }
                    if frame.rotation.is_some() {
                        state.rotation = frame.rotation;
                    }
                    if frame.position.is_some() {
                        state.position = frame.position;
                    }
                }
                if let Some(body) = state.body {
                    if body == B::Head {
                        head = Some(HeadPose {
                            rotation: state.rotation.unwrap_or(Q::IDENTITY),
                            position: state.position,
                        });
                    } else {
                        if let Some(q) = state.rotation {
                            rotations.insert(body, q);
                        }
                        if let Some(p) = state.position {
                            positions.insert(body, p);
                        }
                    }
                }
            }
            let head = head
                .filter(|h| h.position.is_some())
                .ok_or_else(|| invalid("recording has no HMD world position"))?;
            for body in [
                B::LeftUpperLeg,
                B::RightUpperLeg,
                B::LeftLowerLeg,
                B::RightLowerLeg,
            ] {
                if !rotations.contains_key(&body) {
                    return Err(invalid("recording requires both thigh and shin rotations"));
                }
            }
            result.push(MotionFrame {
                at_ms: (index as f64 * self.interval as f64 * 1000.0).round() as u64,
                rotations,
                positions,
                head,
            });
        }
        Ok(result)
    }
    pub fn encode(&self, pfr: bool) -> io::Result<Vec<u8>> {
        if !self.interval.is_finite()
            || !(0.001..=1.0).contains(&self.interval)
            || self.trackers.len() > 256
            || self.trackers.iter().any(|t| t.frames.len() > MAX_FRAMES)
        {
            return Err(invalid("recording limits exceeded"));
        }
        let mut out = Vec::new();
        if pfr {
            out.extend((self.trackers.len() as u32).to_be_bytes());
            for t in &self.trackers {
                write_utf(&mut out, &t.name)?;
                out.extend((t.frames.len() as u32).to_be_bytes());
                for f in &t.frames {
                    write_frame(&mut out, f);
                }
            }
        } else {
            out.push(0);
            out.extend(self.interval.to_be_bytes());
            for (id, t) in self.trackers.iter().enumerate() {
                out.extend([1, id as u8]);
                write_utf(&mut out, &t.name)?;
            }
            for i in 0..self
                .trackers
                .iter()
                .map(|t| t.frames.len())
                .max()
                .unwrap_or(0)
            {
                for (id, t) in self.trackers.iter().enumerate() {
                    if let Some(f) = t.frames.get(i) {
                        out.extend([2, id as u8]);
                        out.extend((i as u32).to_be_bytes());
                        write_frame(&mut out, f);
                    }
                }
            }
        }
        if out.len() as u64 > MAX_BYTES {
            return Err(invalid("recording exceeds 16 MiB"));
        }
        Ok(out)
    }
}
fn read_u32(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}
fn read_byte(r: &mut Cursor<&[u8]>) -> io::Result<u8> {
    let mut b = [0];
    r.read_exact(&mut b)?;
    Ok(b[0])
}
fn read_float(r: &mut Cursor<&[u8]>) -> io::Result<f32> {
    let f = f32::from_bits(read_u32(r)?);
    if !f.is_finite() {
        return Err(invalid("non-finite recording value"));
    }
    Ok(f)
}
fn read_vector(r: &mut Cursor<&[u8]>) -> io::Result<V> {
    Ok(V::new(read_float(r)?, read_float(r)?, read_float(r)?))
}
fn read_quat(r: &mut Cursor<&[u8]>) -> io::Result<Q> {
    let x = read_float(r)?;
    let y = read_float(r)?;
    let z = read_float(r)?;
    let w = read_float(r)?;
    let q = Q::new(w, x, y, z);
    if !q.is_rotation() {
        return Err(invalid("invalid recorded rotation"));
    }
    Ok(q)
}
fn read_utf(r: &mut Cursor<&[u8]>) -> io::Result<String> {
    let mut len = [0; 2];
    r.read_exact(&mut len)?;
    let mut bytes = vec![0; u16::from_be_bytes(len) as usize];
    r.read_exact(&mut bytes)?;
    let mut units = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        i += 1;
        let unit = if b & 0x80 == 0 {
            b as u16
        } else if b & 0xe0 == 0xc0 {
            let c = *bytes
                .get(i)
                .ok_or_else(|| invalid("truncated modified UTF-8"))?;
            i += 1;
            if c & 0xc0 != 0x80 {
                return Err(invalid("invalid modified UTF-8"));
            }
            ((b as u16 & 31) << 6) | (c as u16 & 63)
        } else if b & 0xf0 == 0xe0 {
            let c = *bytes
                .get(i)
                .ok_or_else(|| invalid("truncated modified UTF-8"))?;
            let d = *bytes
                .get(i + 1)
                .ok_or_else(|| invalid("truncated modified UTF-8"))?;
            i += 2;
            if c & 0xc0 != 0x80 || d & 0xc0 != 0x80 {
                return Err(invalid("invalid modified UTF-8"));
            }
            ((b as u16 & 15) << 12) | ((c as u16 & 63) << 6) | (d as u16 & 63)
        } else {
            return Err(invalid("invalid modified UTF-8"));
        };
        units.push(unit);
    }
    String::from_utf16(&units).map_err(|_| invalid("invalid UTF-16 tracker name"))
}
fn write_utf(out: &mut Vec<u8>, name: &str) -> io::Result<()> {
    let mut b = Vec::new();
    for c in name.encode_utf16() {
        if c != 0 && c < 128 {
            b.push(c as u8);
        } else if c < 2048 {
            b.extend([0xc0 | (c >> 6) as u8, 0x80 | (c & 63) as u8]);
        } else {
            b.extend([
                0xe0 | (c >> 12) as u8,
                0x80 | ((c >> 6) & 63) as u8,
                0x80 | (c & 63) as u8,
            ]);
        }
    }
    let len = u16::try_from(b.len()).map_err(|_| invalid("tracker name exceeds Java UTF limit"))?;
    out.extend(len.to_be_bytes());
    out.extend(b);
    Ok(())
}
fn read_frame(r: &mut Cursor<&[u8]>) -> io::Result<Frame> {
    let flags = read_u32(r)?;
    if flags & !63 != 0 {
        return Err(invalid("unknown PFR data flags"));
    }
    let mut f = Frame::default();
    if flags & 1 != 0 {
        let name = read_utf(r)?;
        f.body = serde_json::from_value(serde_json::Value::String(
            name.trim_start_matches("body:").to_owned(),
        ))
        .ok();
    }
    if flags & 2 != 0 {
        f.rotation = Some(read_quat(r)?);
    }
    if flags & 4 != 0 {
        f.position = Some(read_vector(r)?);
    }
    if flags & 8 != 0 {
        f.body = BODIES.get(read_u32(r)? as usize).copied();
    }
    if flags & 16 != 0 {
        f.acceleration = Some(read_vector(r)?);
    }
    if flags & 32 != 0 {
        f.raw_rotation = Some(read_quat(r)?);
    }
    Ok(f)
}
fn write_frame(out: &mut Vec<u8>, f: &Frame) {
    let flags = (u32::from(f.rotation.is_some()) << 1)
        | (u32::from(f.position.is_some()) << 2)
        | (u32::from(f.body.is_some()) << 3)
        | (u32::from(f.acceleration.is_some()) << 4)
        | (u32::from(f.raw_rotation.is_some()) << 5);
    out.extend(flags.to_be_bytes());
    if let Some(q) = f.rotation {
        for v in [q.x, q.y, q.z, q.w] {
            out.extend(v.to_be_bytes());
        }
    }
    if let Some(p) = f.position {
        for v in [p.x, p.y, p.z] {
            out.extend(v.to_be_bytes());
        }
    }
    if let Some(body) = f.body {
        out.extend((BODIES.iter().position(|b| *b == body).unwrap() as u32).to_be_bytes());
    }
    if let Some(p) = f.acceleration {
        for v in [p.x, p.y, p.z] {
            out.extend(v.to_be_bytes());
        }
    }
    if let Some(q) = f.raw_rotation {
        for v in [q.x, q.y, q.z, q.w] {
            out.extend(v.to_be_bytes());
        }
    }
}
pub fn decode(bytes: &[u8], pfr: bool) -> io::Result<Recording> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("recording exceeds 16 MiB"));
    }
    let mut r = Cursor::new(bytes);
    let mut recording = Recording {
        interval: 0.02,
        trackers: Vec::new(),
    };
    if pfr {
        let count = read_u32(&mut r)? as usize;
        if count > 256 {
            return Err(invalid("too many recorded trackers"));
        }
        for _ in 0..count {
            let name = read_utf(&mut r)?;
            let count = read_u32(&mut r)? as usize;
            if count > MAX_FRAMES {
                return Err(invalid("too many recorded frames"));
            }
            let frames = (0..count)
                .map(|_| read_frame(&mut r))
                .collect::<io::Result<_>>()?;
            recording.trackers.push(Tracker { name, frames });
        }
    } else {
        let mut trackers = BTreeMap::<u8, Tracker>::new();
        while r.position() < bytes.len() as u64 {
            match read_byte(&mut r)? {
                0 => recording.interval = read_float(&mut r)?,
                1 => {
                    let id = read_byte(&mut r)?;
                    trackers.entry(id).or_default().name = read_utf(&mut r)?;
                }
                2 => {
                    let id = read_byte(&mut r)?;
                    let index = read_u32(&mut r)? as usize;
                    if index >= MAX_FRAMES {
                        return Err(invalid("too many recorded frames"));
                    }
                    let f = read_frame(&mut r)?;
                    let t = trackers.entry(id).or_default();
                    t.frames.resize(
                        index.max(t.frames.len().saturating_sub(1)) + 1,
                        Frame::default(),
                    );
                    t.frames[index] = f;
                }
                3 => {
                    read_float(&mut r)?;
                    read_float(&mut r)?;
                    let mut count = [0; 2];
                    r.read_exact(&mut count)?;
                    for _ in 0..u16::from_be_bytes(count) {
                        read_utf(&mut r)?;
                        read_float(&mut r)?;
                    }
                }
                _ => return Err(invalid("unknown PFS packet")),
            }
        }
        recording.trackers = trackers.into_values().collect();
    }
    if r.position() != bytes.len() as u64
        || !recording.interval.is_finite()
        || !(0.001..=1.0).contains(&recording.interval)
    {
        return Err(invalid("invalid recording interval or trailing data"));
    }
    Ok(recording)
}
pub fn load(path: &Path) -> io::Result<Recording> {
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_BYTES {
        return Err(invalid("recording exceeds 16 MiB"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    decode(
        &bytes,
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("pfr")),
    )
}
pub fn save_next(dir: &Path, recording: &Recording) -> io::Result<PathBuf> {
    let bytes = recording.encode(false)?;
    fs::create_dir_all(dir)?;
    for number in 1..=10000 {
        let path = dir.join(format!("ABRecording{number}.pfs"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(e);
                }
                return Ok(path);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(invalid("AutoBone recording directory is full"))
}
pub fn load_directory(dir: &Path) -> io::Result<Vec<(PathBuf, Vec<MotionFrame>)>> {
    let entries = match fs::read_dir(dir) {
        Ok(v) => v,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pfs") || e.eq_ignore_ascii_case("pfr"))
        {
            paths.push(path);
        }
    }
    paths.sort();
    if paths.len() > 64 {
        return Err(invalid("too many AutoBone recordings"));
    }
    paths
        .into_iter()
        .map(|path| {
            let frames = load(&path)?.motion_frames()?;
            Ok((path, frames))
        })
        .collect()
}
pub fn save_last(dir: &Path, recording: &Recording) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join("LastABRecording.pfs");
    let bytes = recording.encode(false)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(&path).map_err(|e| e.error)?;
    Ok(path)
}
