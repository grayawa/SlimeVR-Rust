//! Self-contained HLSL for portable runtime compilation.

/// Both GPUI shader programs share one include. Expand it in memory so the
/// compiler never opens a file from the build machine or the user's working directory.
pub(crate) fn embedded_shader_source(emoji: bool) -> String {
    let source = if emoji {
        include_str!("color_text_raster.hlsl")
    } else {
        include_str!("shaders.hlsl")
    };
    source.replace(
        "#include \"alpha_correction.hlsl\"",
        include_str!("alpha_correction.hlsl"),
    )
}
