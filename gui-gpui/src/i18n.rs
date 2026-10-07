//! Existing Fluent translations, with English fallback.
use fluent_bundle::{FluentBundle, FluentResource};

pub struct Localizer {
    bundles: Vec<FluentBundle<FluentResource>>,
}
impl Localizer {
    pub fn new(locale: &str) -> Result<Self, String> {
        let mut bundles = Vec::new();
        if locale != "en" {
            let source = crate::locales::source(locale)
                .ok_or_else(|| format!("Unsupported language: {locale}"))?;
            bundles.push(bundle(
                locale,
                source,
                if locale == "zh-Hans" {
                    include_str!("../i18n/zh-Hans.ftl")
                } else {
                    ""
                },
            )?);
        }
        bundles.push(bundle(
            "en",
            include_str!("../../gui/public/i18n/en/translation.ftl"),
            include_str!("../i18n/en.ftl"),
        )?);
        Ok(Self { bundles })
    }
    pub fn add_override(&mut self, source: &str) -> Result<(), String> {
        let resource = FluentResource::try_new(source.to_owned())
            .map_err(|(_, e)| format!("Invalid override.ftl: {e:?}"))?;
        self.bundles[0].add_resource_overriding(resource);
        Ok(())
    }
    pub fn text(&self, id: &str) -> String {
        self.resolve(id, None)
    }
    pub fn format(&self, id: &str, args: &fluent_bundle::FluentArgs) -> String {
        self.resolve(id, Some(args))
    }
    fn resolve(&self, id: &str, args: Option<&fluent_bundle::FluentArgs>) -> String {
        for bundle in &self.bundles {
            let (name, attribute) = id.split_once('.').map_or((id, None), |(a, b)| (a, Some(b)));
            if let Some(value) = bundle.get_message(name).and_then(|m| match attribute {
                Some(a) => m.get_attribute(a).map(|a| a.value()),
                None => m
                    .value()
                    .or_else(|| m.get_attribute("label").map(|a| a.value())),
            }) {
                let mut errors = Vec::new();
                let text = bundle.format_pattern(value, args, &mut errors);
                if errors.is_empty() {
                    return plain_text(&text);
                }
            }
        }
        id.to_owned()
    }
}
fn bundle(
    locale: &str,
    existing: &str,
    native: &str,
) -> Result<FluentBundle<FluentResource>, String> {
    let mut bundle = FluentBundle::new(vec![
        (if locale == "en-x-owo" { "en" } else { locale })
            .parse()
            .map_err(|e| format!("Invalid locale: {e}"))?,
    ]);
    bundle.set_use_isolating(false);
    for source in [existing, native] {
        let resource = FluentResource::try_new(source.to_owned())
            .map_err(|(_, errors)| format!("Invalid Fluent resource: {errors:?}"))?;
        // Existing translations contain repeated IDs; match the web frontend's
        // last-definition-wins behavior while still rejecting syntax errors.
        bundle.add_resource_overriding(resource);
    }
    Ok(bundle)
}

fn plain_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut tag = false;
    for c in text.chars() {
        if c == '<' {
            tag = true;
        } else if c == '>' && tag {
            tag = false;
        } else if !tag {
            out.push(c);
        }
    }
    out
}
