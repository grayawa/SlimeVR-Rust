use image::{AnimationDecoder, ImageDecoder};
use slimevr_gpui::i18n::Localizer;
use std::io::Cursor;

#[test]
fn guided_measurement_uses_original_localizations() {
    for locale in ["en", "zh-Hans"] {
        let l10n = Localizer::new(locale).unwrap();
        for key in [
            "onboarding-automatic_proportions-title",
            "onboarding-automatic_proportions-description",
            "onboarding-automatic_proportions-put_trackers_on-title",
            "onboarding-automatic_proportions-put_trackers_on-description",
            "onboarding-automatic_proportions-put_trackers_on-next",
            "onboarding-automatic_proportions-back",
            "onboarding-automatic_proportions-prev_step",
            "onboarding-automatic_proportions-requirements-title",
            "onboarding-automatic_proportions-requirements-descriptionv2",
            "onboarding-automatic_proportions-requirements-next",
            "onboarding-automatic_mounting-preparation-title",
            "onboarding-automatic_mounting-preparation-v2-step-0",
            "onboarding-automatic_mounting-preparation-v2-step-1",
            "onboarding-automatic_mounting-preparation-v2-step-2",
            "onboarding-automatic_proportions-start_recording-title",
            "onboarding-automatic_proportions-start_recording-description",
            "onboarding-automatic_proportions-start_recording-next",
            "onboarding-automatic_proportions-recording-title",
            "onboarding-automatic_proportions-recording-description-p0",
            "onboarding-automatic_proportions-recording-description-p1",
            "onboarding-automatic_proportions-recording-steps",
            "onboarding-automatic_proportions-recording-processing",
            "onboarding-automatic_proportions-verify_results-title",
            "onboarding-automatic_proportions-verify_results-description",
            "onboarding-automatic_proportions-verify_results-results",
            "onboarding-automatic_proportions-verify_results-redo",
            "onboarding-automatic_proportions-verify_results-confirm",
            "onboarding-automatic_proportions-done-title",
            "onboarding-automatic_proportions-done-description",
            "onboarding-automatic_proportions-error_modal-v2",
            "onboarding-automatic_proportions-error_modal-confirm",
            "tips-do_not_move_heels",
            "tips-find_tracker",
            "native-help",
        ] {
            assert_ne!(l10n.text(key), key, "{locale}: {key}");
        }
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("time", 10);
        let timer = l10n.format("onboarding-automatic_proportions-recording-timer", &args);
        assert!(timer.contains("10"), "{locale}: {timer}");
    }
}

#[test]
fn motion_demo_decodes_as_bounded_animation() {
    let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(include_bytes!(
        "../assets/autobone-motion.webp"
    )))
    .unwrap();
    assert!(decoder.has_animation());
    let (width, height) = decoder.dimensions();
    assert!(width <= 192 && height <= 400);
    let frames = decoder.into_frames().collect_frames().unwrap();
    assert_eq!(frames.len(), 144);
    assert!(frames.len() * width as usize * height as usize * 4 < 25 * 1024 * 1024);
    assert!(frames.iter().all(|frame| {
        let (numer, denom) = frame.delay().numer_denom_ms();
        numer == 125 * denom
    }));
}
