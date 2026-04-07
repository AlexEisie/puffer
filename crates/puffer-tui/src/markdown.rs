use ratatui::text::{Line, Text};
use std::path::Path;

/// Renders markdown into styled `ratatui::Text`.
pub(crate) fn render_markdown(input: &str) -> Text<'static> {
    crate::markdown_render::render_markdown_text(input)
}

/// Appends rendered markdown lines while resolving local file-link display relative to `cwd`.
pub(crate) fn append_markdown(
    markdown_source: &str,
    width: Option<usize>,
    cwd: Option<&Path>,
    lines: &mut Vec<Line<'static>>,
) {
    let rendered =
        crate::markdown_render::render_markdown_text_with_width_and_cwd(markdown_source, width, cwd);
    lines.extend(rendered.lines);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_and_lists_are_rendered() {
        let text = render_markdown("# Header\n- item");
        let rendered = text
            .lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("# Header"));
        assert!(rendered.contains("- item"));
    }
}
