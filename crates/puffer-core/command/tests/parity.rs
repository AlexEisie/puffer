use puffer_resources::{PromptTemplate, ToolSpec};
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn read_repo_file(relative_path: &str) -> String {
    fs::read_to_string(repo_root().join(relative_path)).unwrap()
}

fn load_prompt(relative_path: &str) -> PromptTemplate {
    serde_yaml::from_str(&read_repo_file(relative_path)).unwrap()
}

fn load_tool(relative_path: &str) -> ToolSpec {
    serde_yaml::from_str(&read_repo_file(relative_path)).unwrap()
}

fn render_prompt(relative_path: &str, variables: &[(&str, &str)]) -> String {
    let prompt = load_prompt(relative_path);
    prompt.render(
        &variables
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
    )
}

fn extract_template_literal(contents: &str, marker: &str) -> String {
    let start = contents.find(marker).unwrap() + marker.len();
    let source = &contents[start..];
    let mut end = None;
    let mut index = 0usize;
    let mut escaped = false;
    let mut interpolation_depth = 0usize;

    while index < source.len() {
        let ch = source[index..].chars().next().unwrap();
        let width = ch.len_utf8();
        if escaped {
            escaped = false;
            index += width;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            index += width;
            continue;
        }
        if interpolation_depth == 0 && ch == '`' {
            end = Some(start + index);
            break;
        }
        if source[index..].starts_with("${") {
            interpolation_depth += 1;
            index += 2;
            continue;
        }
        if interpolation_depth > 0 {
            match ch {
                '{' => interpolation_depth += 1,
                '}' => interpolation_depth = interpolation_depth.saturating_sub(1),
                _ => {}
            }
        }
        index += width;
    }

    contents[start..end.unwrap()].to_string()
}

fn normalize_reference_template(raw: &str) -> String {
    let unescaped = raw.replace("\\`", "`");
    let trimmed = unescaped.strip_prefix('\n').unwrap_or(&unescaped);
    dedent(trimmed)
}

fn dedent(raw: &str) -> String {
    let indent = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|ch| *ch == ' ').count())
        .min()
        .unwrap_or(0);
    raw.lines()
        .map(|line| line.strip_prefix(&" ".repeat(indent)).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_frontmatter(markdown: &str) -> String {
    let trimmed = markdown.trim_start();
    if !trimmed.starts_with("---\n") {
        return trimmed.to_string();
    }
    let remainder = &trimmed[4..];
    let end = remainder.find("\n---\n").unwrap();
    remainder[end + 5..].trim_start_matches('\n').to_string()
}

fn fenced(output: &str) -> String {
    format!("```\n{output}\n```")
}

#[test]
fn init_prompt_matches_claude_reference() {
    let prompt = load_prompt("resources/prompts/init.yaml");
    let reference = read_repo_file("references/claude-code/src/commands/init.ts");
    let expected = normalize_reference_template(&extract_template_literal(
        &reference,
        "const NEW_INIT_PROMPT = `",
    ));

    assert_eq!(prompt.template.trim_end(), expected.trim_end());
}

#[test]
fn review_prompt_matches_claude_reference_when_rendered() {
    let rendered = render_prompt("resources/prompts/review.yaml", &[("ARGUMENTS", "123")]);
    let reference = read_repo_file("references/claude-code/src/commands/review.ts");
    let expected = normalize_reference_template(&extract_template_literal(
        &reference,
        "const LOCAL_REVIEW_PROMPT = (args: string) => `",
    ))
    .replace("${args}", "123");

    assert_eq!(rendered.trim_end(), expected.trim_end());
}

#[test]
fn pr_comments_prompt_matches_claude_reference_when_rendered() {
    let rendered = render_prompt(
        "resources/prompts/pr-comments.yaml",
        &[(
            "ADDITIONAL_USER_INPUT_BLOCK",
            "Additional user input: focus on unresolved threads",
        )],
    );
    let reference = read_repo_file("references/claude-code/src/commands/pr_comments/index.ts");
    let expected = normalize_reference_template(&extract_template_literal(&reference, "text: `"))
        .replace(
            "${args ? 'Additional user input: ' + args : ''}",
            "Additional user input: focus on unresolved threads",
        );

    assert_eq!(rendered.trim_end(), expected.trim_end());
}

#[test]
fn security_review_prompt_matches_claude_reference_when_rendered() {
    let git_status = "On branch main\nnothing to commit, working tree clean";
    let files_modified = "src/lib.rs";
    let commits = "abc123 tighten prompt parity";
    let diff_content = "diff --git a/src/lib.rs b/src/lib.rs";
    let rendered = render_prompt(
        "resources/prompts/security-review.yaml",
        &[
            ("GIT_STATUS", git_status),
            ("FILES_MODIFIED", files_modified),
            ("COMMITS", commits),
            ("DIFF_CONTENT", diff_content),
        ],
    );
    let reference = read_repo_file("references/claude-code/src/commands/security-review.ts");
    let expected = strip_frontmatter(&normalize_reference_template(&extract_template_literal(
        &reference,
        "const SECURITY_REVIEW_MARKDOWN = `",
    )))
    .replace("```\n!`git status`\n```", &fenced(git_status))
    .replace(
        "```\n!`git diff --name-only origin/HEAD...`\n```",
        &fenced(files_modified),
    )
    .replace(
        "```\n!`git log --no-decorate origin/HEAD...`\n```",
        &fenced(commits),
    )
    .replace(
        "```\n!`git diff origin/HEAD...`\n```",
        &fenced(diff_content),
    );

    assert_eq!(rendered.trim_end(), expected.trim_end());
}

#[test]
fn statusline_prompt_matches_claude_reference_when_rendered() {
    let rendered = render_prompt(
        "resources/prompts/statusline.yaml",
        &[("STATUSLINE_PROMPT_JSON", "\"Mirror my starship prompt\"")],
    );
    let reference = read_repo_file("references/claude-code/src/commands/statusline.tsx");
    let expected = normalize_reference_template(&extract_template_literal(&reference, "text: `"))
        .replace("${AGENT_TOOL_NAME}", "Agent")
        .replace("${prompt}", "Mirror my starship prompt");

    assert_eq!(rendered.trim_end(), expected.trim_end());
}

#[test]
fn commit_prompt_matches_claude_reference_when_rendered() {
    let prompt = load_prompt("resources/prompts/commit.yaml");
    let rendered = prompt.render(&std::collections::BTreeMap::from([
        ("GIT_STATUS".to_string(), "STATUS".to_string()),
        ("GIT_DIFF".to_string(), "DIFF".to_string()),
        ("CURRENT_BRANCH".to_string(), "BRANCH".to_string()),
        ("RECENT_COMMITS".to_string(), "COMMITS".to_string()),
        ("COMMIT_ATTRIBUTION_BLOCK".to_string(), String::new()),
    ]));
    let reference = read_repo_file("references/claude-code/src/commands/commit.ts");
    let expected =
        normalize_reference_template(&extract_template_literal(&reference, "return `${prefix}"))
            .replace("!`git status`", "STATUS")
            .replace("!`git diff HEAD`", "DIFF")
            .replace("!`git branch --show-current`", "BRANCH")
            .replace("!`git log --oneline -10`", "COMMITS")
            .replace(
                r#"${commitAttribution ? `\n\n${commitAttribution}` : ''}"#,
                "",
            );

    assert_eq!(rendered.trim_end(), expected.trim_end());
}

#[test]
fn ask_user_question_tool_prompt_matches_claude_reference() {
    let tool = load_tool("resources/tools/ask_user_question.yaml");
    let reference =
        read_repo_file("references/claude-code/src/tools/AskUserQuestionTool/prompt.ts");
    let prompt = normalize_reference_template(&extract_template_literal(
        &reference,
        "export const ASK_USER_QUESTION_TOOL_PROMPT = `",
    ))
    .replace("${EXIT_PLAN_MODE_TOOL_NAME}", "ExitPlanMode");
    let preview =
        normalize_reference_template(&extract_template_literal(&reference, "markdown: `"));
    let expected = format!("{prompt}\n{preview}");

    assert_eq!(tool.description.trim_end(), expected.trim_end());
}

#[test]
fn enter_plan_mode_tool_prompt_matches_claude_reference() {
    let tool = load_tool("resources/tools/enter_plan_mode.yaml");
    let reference = read_repo_file("references/claude-code/src/tools/EnterPlanModeTool/prompt.ts");
    let what_happens = normalize_reference_template(&extract_template_literal(
        &reference,
        "const WHAT_HAPPENS_SECTION = `",
    ))
    .replace("${ASK_USER_QUESTION_TOOL_NAME}", "AskUserQuestion");
    let expected = normalize_reference_template(&extract_template_literal(&reference, "return `"))
        .replace("${whatHappens}", &format!("{what_happens}\n"))
        .replace("${ASK_USER_QUESTION_TOOL_NAME}", "AskUserQuestion");

    assert_eq!(tool.description.trim_end(), expected.trim_end());
}

#[test]
fn exit_plan_mode_tool_prompt_matches_claude_reference() {
    let tool = load_tool("resources/tools/exit_plan_mode.yaml");
    let reference = read_repo_file("references/claude-code/src/tools/ExitPlanModeTool/prompt.ts");
    let expected = normalize_reference_template(&extract_template_literal(
        &reference,
        "export const EXIT_PLAN_MODE_V2_TOOL_PROMPT = `",
    ))
    .replace("${ASK_USER_QUESTION_TOOL_NAME}", "AskUserQuestion");

    assert_eq!(tool.description.trim_end(), expected.trim_end());
}

#[test]
fn todo_write_tool_prompt_matches_claude_reference() {
    let tool = load_tool("resources/tools/todo_write.yaml");
    let reference = read_repo_file("references/claude-code/src/tools/TodoWriteTool/prompt.ts");
    let expected = normalize_reference_template(&extract_template_literal(
        &reference,
        "export const PROMPT = `",
    ))
    .replace("${FILE_EDIT_TOOL_NAME}", "Edit");

    assert_eq!(tool.description.trim_end(), expected.trim_end());
}
