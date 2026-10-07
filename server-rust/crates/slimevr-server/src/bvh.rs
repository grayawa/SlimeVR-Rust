//! Port of BVHFileStream (BLENDER settings) and TickReducer.
//! HIP-tail root, metres, ZXY degrees, 100 Hz; stream frames instead of retaining a clip.
use slimevr_core::{
    skeleton::{BoneLinks, SkeletonPose},
    Quaternion as Q, Vector3 as V,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{self, BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub const FRAME_INTERVAL: f32 = 0.01;

#[derive(Clone, Debug, PartialEq)]
struct Node {
    name: String,
    previous: Option<usize>,
    inverse_offset: bool,
    zero_offset: bool,
    children: Vec<usize>,
}
#[derive(Clone, Debug, PartialEq)]
struct Rig {
    nodes: Vec<Node>,
    signature: BTreeMap<String, (BoneLinks, f32, Q)>,
}
impl Rig {
    fn new(pose: &SkeletonPose) -> io::Result<Self> {
        let root = pose.hierarchy.get("hip").ok_or_else(|| {
            io::Error::other("BVH requires a solved skeleton; wait for the first pose frame")
        })?;
        let mut rig = Self {
            nodes: Vec::new(),
            signature: BTreeMap::new(),
        };
        rig.visit(
            pose,
            "hip",
            None,
            root.parent.is_some(),
            false,
            &mut BTreeSet::new(),
        )?;
        Ok(rig)
    }

    // Follow the original inverse traversal, including sibling offsets and tracker/finger bones.
    fn visit(
        &mut self,
        pose: &SkeletonPose,
        name: &str,
        previous: Option<usize>,
        inverse_offset: bool,
        zero_offset: bool,
        visited: &mut BTreeSet<String>,
    ) -> io::Result<usize> {
        if !visited.insert(name.into()) || visited.len() > 256 {
            return Err(io::Error::other("invalid BVH skeleton hierarchy"));
        }
        let links = pose
            .hierarchy
            .get(name)
            .ok_or_else(|| io::Error::other("missing BVH bone links"))?;
        let bone = pose
            .bones
            .get(name)
            .ok_or_else(|| io::Error::other("missing BVH bone"))?;
        self.signature.insert(
            name.into(),
            (links.clone(), bone.length, bone.rotation_offset),
        );
        let inverse = previous
            .is_some_and(|i| pose.hierarchy[&self.nodes[i].name].parent.as_deref() == Some(name));
        let visit_parent = (inverse || previous.is_none()) && links.parent.is_some();
        let index = self.nodes.len();
        self.nodes.push(Node {
            name: name.into(),
            previous,
            inverse_offset,
            zero_offset,
            children: Vec::new(),
        });
        let mut children = Vec::new();
        if visit_parent {
            let parent = links.parent.as_deref().unwrap();
            children.push(self.visit(
                pose,
                parent,
                Some(index),
                inverse_offset || inverse,
                false,
                visited,
            )?);
            for child in &pose.hierarchy[parent].children {
                if child != name {
                    children.push(self.visit(
                        pose,
                        child,
                        Some(index),
                        true,
                        !inverse_offset,
                        visited,
                    )?);
                }
            }
        }
        if !inverse {
            for child in &links.children {
                children.push(self.visit(
                    pose,
                    child,
                    Some(index),
                    false,
                    inverse_offset,
                    visited,
                )?);
            }
        }
        self.nodes[index].children = children;
        Ok(index)
    }
    fn validate(&self, pose: &SkeletonPose) -> io::Result<()> {
        for (name, (links, length, offset)) in &self.signature {
            if pose.hierarchy.get(name) != Some(links)
                || pose
                    .bones
                    .get(name)
                    .is_none_or(|b| b.length != *length || b.rotation_offset != *offset)
            {
                return Err(io::Error::other(
                    "BVH stopped because bone lengths or hierarchy changed; start a new recording",
                ));
            }
        }
        Ok(())
    }
    fn header(
        &self,
        pose: &SkeletonPose,
        writer: &mut impl Write,
        index: usize,
        depth: usize,
    ) -> io::Result<()> {
        let node = &self.nodes[index];
        let indent = "\t".repeat(depth);
        writeln!(
            writer,
            "{indent}{} {}",
            if node.previous.is_none() {
                "ROOT"
            } else {
                "JOINT"
            },
            node.name.to_ascii_uppercase()
        )?;
        writeln!(writer, "{indent}{{")?;
        let offset = match node.previous {
            Some(previous) if !node.zero_offset => {
                let last = pose.bones[&self.nodes[previous].name];
                last.rotation_offset.rotate(V::UP)
                    * (if node.inverse_offset {
                        last.length
                    } else {
                        -last.length
                    })
            }
            _ => V::ZERO,
        };
        writeln!(
            writer,
            "{indent}\tOFFSET {} {} {}",
            offset.x, offset.y, offset.z
        )?;
        writeln!(
            writer,
            "{indent}\tCHANNELS {}Zrotation Xrotation Yrotation",
            if node.previous.is_none() {
                "6 Xposition Yposition Zposition "
            } else {
                "3 "
            }
        )?;
        if node.children.is_empty() {
            let bone = pose.bones[&node.name];
            let end = bone.rotation_offset.rotate(V::UP) * -bone.length;
            writeln!(
                writer,
                "{indent}\tEnd Site\n{indent}\t{{\n{indent}\t\tOFFSET {} {} {}\n{indent}\t}}",
                end.x, end.y, end.z
            )?;
        } else {
            for child in &node.children {
                self.header(pose, writer, *child, depth + 1)?;
            }
        }
        writeln!(writer, "{indent}}}")
    }
    fn frame(&self, pose: &SkeletonPose, writer: &mut impl Write) -> io::Result<()> {
        self.validate(pose)?;
        for node in &self.nodes {
            let bone = pose.bones[&node.name];
            let inverse_parent = if let Some(previous) = node.previous {
                let last = pose.bones[&self.nodes[previous].name];
                last.rotation_offset * last.rotation.inv()
            } else {
                let position = if node.inverse_offset {
                    bone.tail
                } else {
                    bone.head
                };
                write!(writer, "{} {} {}", position.x, position.y, position.z)?;
                Q::IDENTITY
            };
            let angles = (bone.rotation_offset.inv() * inverse_parent * bone.rotation).euler_zxy();
            let degrees = 180.0 / std::f32::consts::PI;
            write!(
                writer,
                " {} {} {}",
                angles.z * degrees,
                angles.x * degrees,
                angles.y * degrees
            )?;
        }
        writeln!(writer)
    }
}

#[derive(Clone, Debug)]
pub struct SavedRecording {
    pub path: PathBuf,
    pub frames: u64,
}

pub struct Recorder {
    writer: BufWriter<File>,
    path: PathBuf,
    rig: Rig,
    frames: u64,
    count_offset: u64,
    delta: f32,
    offset: f32,
    finished: bool,
}
impl Recorder {
    /// Existing directories use BVH-RecordingN.bvh. Existing files are never overwritten.
    pub fn start(path: &Path, pose: &SkeletonPose) -> io::Result<Self> {
        let rig = Rig::new(pose)?;
        let (file, path) = if path.is_dir() {
            let mut number = 1u64;
            loop {
                let candidate = path.join(format!("BVH-Recording{number}.bvh"));
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&candidate)
                {
                    Ok(file) => break (file, candidate),
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => number += 1,
                    Err(e) => return Err(e),
                }
            }
        } else {
            if path.extension().is_none_or(|ext| ext != "bvh") {
                return Err(io::Error::other(
                    "BVH output must be an existing directory or a .bvh file",
                ));
            }
            (
                OpenOptions::new().write(true).create_new(true).open(path)?,
                path.to_owned(),
            )
        };
        let path = path.canonicalize()?;
        let mut writer = BufWriter::new(file);
        writeln!(writer, "HIERARCHY")?;
        rig.header(pose, &mut writer, 0, 0)?;
        write!(writer, "MOTION\nFrames: ")?;
        let count_offset = writer.stream_position()?;
        writeln!(writer, "{:<19}", 0)?;
        writeln!(writer, "Frame Time: {FRAME_INTERVAL}")?;
        writer.flush()?;
        Ok(Self {
            writer,
            path,
            rig,
            frames: 0,
            count_offset,
            delta: 0.0,
            offset: 0.0,
            finished: false,
        })
    }
    /// TickReducer phase correction: at most one frame per server tick, no interpolation.
    pub fn tick(&mut self, pose: &SkeletonPose, dt: f32) -> io::Result<()> {
        if self.finished {
            return Err(io::Error::other("BVH recording has already finished"));
        }
        self.rig.validate(pose)?;
        if !dt.is_finite() || dt < 0.0 {
            return Err(io::Error::other("invalid BVH tick interval"));
        }
        self.delta += dt;
        if self.delta + self.offset < FRAME_INTERVAL {
            return Ok(());
        }
        self.rig.frame(pose, &mut self.writer)?;
        self.frames += 1;
        self.offset = (self.delta - FRAME_INTERVAL).min(FRAME_INTERVAL / 2.0);
        self.delta = 0.0;
        Ok(())
    }
    pub fn finish(&mut self) -> io::Result<SavedRecording> {
        if !self.finished {
            self.writer.seek(SeekFrom::Start(self.count_offset))?;
            write!(self.writer, "{:<19}", self.frames)?;
            self.writer.flush()?;
            self.finished = true;
        }
        Ok(SavedRecording {
            path: self.path.clone(),
            frames: self.frames,
        })
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
