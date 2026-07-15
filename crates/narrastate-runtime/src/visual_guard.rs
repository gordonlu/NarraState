/// Fixed response for questions that try to treat decorative generated visuals as case facts.
pub const VISUAL_ASSET_DISCLAIMER: &str =
    "这是视觉氛围图或角色示意图，不构成案件事实或证据。请以案件中的结构化线索、事实和陈述为准。";

/// Returns true when the player explicitly asks about content depicted in a decorative visual.
///
/// This guard intentionally stays narrow: ordinary questions about a location or character are
/// not intercepted unless the player refers to a cover, portrait, illustration, or image.
pub fn is_visual_asset_question(text: &str) -> bool {
    let normalized = text.to_lowercase();
    const MARKERS: &[&str] = &[
        "背景图",
        "场景图",
        "示意图",
        "氛围图",
        "插画",
        "图片里",
        "图片中",
        "图里",
        "图中",
        "头像里",
        "头像中",
        "封面里",
        "封面中",
        "background image",
        "scene image",
        "illustration",
        "in the image",
        "in the picture",
        "portrait",
        "cover image",
    ];
    MARKERS.iter().any(|marker| normalized.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_questions_about_visual_details() {
        assert!(is_visual_asset_question("背景图里那辆车是不是嫌疑人的？"));
        assert!(is_visual_asset_question(
            "Who is standing in the background image?"
        ));
    }

    #[test]
    fn leaves_normal_case_questions_for_the_interpreter() {
        assert!(!is_visual_asset_question("案发时停车场里有谁？"));
        assert!(!is_visual_asset_question("请解释这份结构化证据。"));
    }
}
