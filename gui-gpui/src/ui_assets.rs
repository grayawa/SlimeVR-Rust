use gpui_kit::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: Option<&'static [u8]> = match path {
            "slime/slimes.webp" => Some(include_bytes!("../../gui/public/images/slimes.webp")),
            "slime/trackers/v1_2_slime.webp" => Some(include_bytes!(
                "../../gui/public/images/trackers/v1_2_slime.webp"
            )),
            "slime/trackers/butterfly_slime.webp" => Some(include_bytes!(
                "../../gui/public/images/trackers/butterfly_slime.webp"
            )),
            "slime/quiz/quiz_mocap-pos_forehead.webp" => Some(include_bytes!(
                "../../gui/public/images/quiz/quiz_mocap-pos_forehead.webp"
            )),
            "slime/quiz/quiz_mocap-pos_face.webp" => Some(include_bytes!(
                "../../gui/public/images/quiz/quiz_mocap-pos_face.webp"
            )),
            "slime/WifiNetwork.svg" => Some(include_bytes!("../assets/icons/WifiNetwork.svg")),
            "slime/USB.svg" => Some(include_bytes!("../assets/icons/USB.svg")),
            "slime/Sitting.svg" => Some(include_bytes!("../assets/icons/Sitting.svg")),
            "slime/boxslime.webp" => Some(include_bytes!("../../gui/public/images/boxslime.webp")),
            "slime/assignment-pose.webp" => Some(include_bytes!(
                "../../gui/public/images/assignment-pose.webp"
            )),
            "slime/mounting-reset-pose.webp" => Some(include_bytes!(
                "../../gui/public/images/mounting-reset-pose.webp"
            )),
            "slime/slime-girl.webp" => {
                Some(include_bytes!("../../gui/public/images/slime-girl.webp"))
            }
            "slime/front-standing-pose.webp" => Some(include_bytes!(
                "../../gui/public/images/front-standing-pose.webp"
            )),
            "slime/reset-pose.webp" => {
                Some(include_bytes!("../../gui/public/images/reset-pose.webp"))
            }
            "slime/Bulb.svg" => Some(include_bytes!("../assets/icons/Bulb.svg")),
            "slime/Ankle.svg" => Some(include_bytes!("../assets/icons/Ankle.svg")),
            "slime/Chest.svg" => Some(include_bytes!("../assets/icons/Chest.svg")),
            "slime/Controller.svg" => Some(include_bytes!("../assets/icons/Controller.svg")),
            "slime/Foot.svg" => Some(include_bytes!("../assets/icons/Foot.svg")),
            "slime/Gear.svg" => Some(include_bytes!("../assets/icons/Gear.svg")),
            "slime/Headset.svg" => Some(include_bytes!("../assets/icons/Headset.svg")),
            "slime/Hip.svg" => Some(include_bytes!("../assets/icons/Hip.svg")),
            "slime/Home.svg" => Some(include_bytes!("../assets/icons/Home.svg")),
            "slime/Human.svg" => Some(include_bytes!("../assets/icons/Human.svg")),
            "slime/LowerArm.svg" => Some(include_bytes!("../assets/icons/LowerArm.svg")),
            "slime/Neck.svg" => Some(include_bytes!("../assets/icons/Neck.svg")),
            "slime/Ruler.svg" => Some(include_bytes!("../assets/icons/Ruler.svg")),
            "slime/Shoulder.svg" => Some(include_bytes!("../assets/icons/Shoulder.svg")),
            "slime/Ski.svg" => Some(include_bytes!("../assets/icons/Ski.svg")),
            "slime/SlimeVR.svg" => Some(include_bytes!("../assets/icons/SlimeVR.svg")),
            "slime/UpperArm.svg" => Some(include_bytes!("../assets/icons/UpperArm.svg")),
            "slime/UpperChest.svg" => Some(include_bytes!("../assets/icons/UpperChest.svg")),
            "slime/UpperLeg.svg" => Some(include_bytes!("../assets/icons/UpperLeg.svg")),
            "slime/Waist.svg" => Some(include_bytes!("../assets/icons/Waist.svg")),
            "slime/Wifi.svg" => Some(include_bytes!("../assets/icons/Wifi.svg")),
            "slime/Steam.svg" => Some(include_bytes!("../assets/icons/Steam.svg")),
            "slime/Wrench.svg" => Some(include_bytes!("../assets/icons/Wrench.svg")),
            "slime/Router.svg" => Some(include_bytes!("../assets/icons/Router.svg")),
            "slime/VRC.svg" => Some(include_bytes!("../assets/icons/VRC.svg")),
            "slime/VMC.svg" => Some(include_bytes!("../assets/icons/VMC.svg")),
            "slime/Squares.svg" => Some(include_bytes!("../assets/icons/Squares.svg")),
            "slime/Check.svg" => Some(include_bytes!("../assets/icons/Check.svg")),
            "slime/Bell.svg" => Some(include_bytes!("../assets/icons/Bell.svg")),
            "slime/ArrowRightLeft.svg" => {
                Some(include_bytes!("../assets/icons/ArrowRightLeft.svg"))
            }
            "slime/reset/FullResetPose.webp" => Some(include_bytes!(
                "../../gui/public/images/reset/FullResetPose.webp"
            )),
            "slime/reset/FullResetPoseSide.webp" => Some(include_bytes!(
                "../../gui/public/images/reset/FullResetPoseSide.webp"
            )),
            "slime/reset/FullResetPoseWrong.webp" => Some(include_bytes!(
                "../../gui/public/images/reset/FullResetPoseWrong.webp"
            )),
            "slime/mounting/MountingFeets.webp" => Some(include_bytes!(
                "../../gui/public/images/mounting/MountingFeets.webp"
            )),
            "slime/mounting/MountingFeetsSide.webp" => Some(include_bytes!(
                "../../gui/public/images/mounting/MountingFeetsSide.webp"
            )),
            "slime/Bug.svg" => Some(include_bytes!("../assets/icons/Bug.svg")),
            "slime/stay-aligned/StayAlignedStanding.webp" => Some(include_bytes!(
                "../../gui/public/images/stay-aligned/StayAlignedStanding.webp"
            )),
            "slime/stay-aligned/StayAlignedSitting.webp" => Some(include_bytes!(
                "../../gui/public/images/stay-aligned/StayAlignedSitting.webp"
            )),
            "slime/stay-aligned/StayAlignedFloor.webp" => Some(include_bytes!(
                "../../gui/public/images/stay-aligned/StayAlignedFloor.webp"
            )),
            _ => None,
        };
        if let Some(bytes) = bytes {
            Ok(Some(Cow::Borrowed(bytes)))
        } else {
            gpui_kit::assets::Assets.load(path)
        }
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit::assets::Assets.list(path)
    }
}

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};
enum Illustration {
    Loading,
    Ready(Arc<gpui_kit::RenderImage>),
    Failed(SharedString),
}
static IMAGES: OnceLock<Mutex<HashMap<String, Illustration>>> = OnceLock::new();
fn images() -> &'static Mutex<HashMap<String, Illustration>> {
    IMAGES.get_or_init(Default::default)
}

/// Decode off the UI thread and bound textures to their actual display needs.
/// The original assets remain embedded; decoding/uploading their full-resolution
/// versions (the mounting pose is 13 MP) wastes memory and delays navigation.
pub fn image(path: &str, cx: &mut gpui_kit::App) -> gpui_kit::Img {
    let path = path.to_owned();
    let start = {
        let mut cache = images().lock().unwrap_or_else(|e| e.into_inner());
        if cache.contains_key(&path) {
            false
        } else {
            cache.insert(path.clone(), Illustration::Loading);
            true
        }
    };
    if start {
        let source = Assets.load(&path);
        let key = path.clone();
        let task = cx.background_executor().spawn(async move {
            let bytes = source?.ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Missing illustration: {key}"),
                )
            })?;
            let (width, height) = if key.contains("assignment-pose") {
                (640, 1600)
            } else if key.contains("front-standing") {
                (360, 1000)
            } else {
                (720, 1000)
            };
            let decoded = image::load_from_memory(&bytes)?;
            let mut rgba = decoded.thumbnail(width, height).into_rgba8();
            for pixel in rgba.pixels_mut() {
                pixel.0.swap(0, 2);
            }
            let image = Arc::new(gpui_kit::RenderImage::new([image::Frame::new(rgba)]));
            let result: Result<Arc<gpui_kit::RenderImage>> = Ok(image);
            result
        });
        let key = path.clone();
        cx.spawn(async move |cx| {
            let result = task.await;
            let value = match result {
                Ok(image) => Illustration::Ready(image),
                Err(error) => {
                    slimevr_gpui::logging::write(
                        slimevr_gpui::log_level::LogLevel::Error,
                        "illustrations",
                        &error.to_string(),
                    );
                    Illustration::Failed(error.to_string().into())
                }
            };
            images()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(key, value);
            cx.update(|cx| cx.refresh_windows());
        })
        .detach();
    }
    gpui_kit::img(gpui_kit::ImageSource::Custom(Arc::new(move |_, _| {
        let cache = images().lock().unwrap_or_else(|e| e.into_inner());
        match cache.get(&path) {
            Some(Illustration::Ready(image)) => Some(Ok(image.clone())),
            Some(Illustration::Failed(error)) => {
                Some(Err(gpui_kit::ImageCacheError::Asset(error.clone())))
            }
            _ => None,
        }
    })))
}
