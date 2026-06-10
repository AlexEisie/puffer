use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(alias = "allowed-tools")]
    allowed_tools: Vec<String>,
    #[serde(alias = "user-invocable")]
    user_invocable: bool,
    #[serde(alias = "disable-model-invocation")]
    disable_model_invocation: bool,
}

fn parse_skill(markdown: &str) -> (SkillFrontmatter, &str) {
    let rest = markdown
        .strip_prefix("---\n")
        .expect("skill starts with frontmatter");
    let (frontmatter, body) = rest
        .split_once("\n---\n")
        .expect("skill frontmatter terminates");
    let parsed = serde_yaml::from_str(frontmatter).expect("skill frontmatter parses");
    (parsed, body)
}

#[test]
fn image_generation_skill_guides_image_generation_tool_use() {
    let (frontmatter, body) = parse_skill(include_str!(
        "../../../resources/skills/image-generation/SKILL.md"
    ));

    assert_eq!(frontmatter.name, "image-generation");
    assert!(frontmatter.description.contains("ImageGeneration"));
    assert_eq!(frontmatter.allowed_tools, vec!["ImageGeneration"]);
    assert!(frontmatter.user_invocable);
    assert!(!frontmatter.disable_model_invocation);
    assert!(body.contains("Use `ImageGeneration`"));
    assert!(body.contains("Call `ImageGeneration` once"));
    assert!(body.contains("set `count`"));
    assert!(body.contains("Do not hand-author SVG"));
}

#[test]
fn video_generation_skill_guides_text_to_video_tool_use() {
    let (frontmatter, body) = parse_skill(include_str!(
        "../../../resources/skills/video-generation/SKILL.md"
    ));

    assert_eq!(frontmatter.name, "video-generation");
    assert!(frontmatter.description.contains("VideoGeneration"));
    assert_eq!(frontmatter.allowed_tools, vec!["VideoGeneration"]);
    assert!(frontmatter.user_invocable);
    assert!(!frontmatter.disable_model_invocation);
    assert!(body.contains("Use `VideoGeneration`"));
    assert!(body.contains("Call `VideoGeneration` once"));
    assert!(body.contains("text-to-video only"));
    assert!(body.contains("persisted video artifact"));
}
