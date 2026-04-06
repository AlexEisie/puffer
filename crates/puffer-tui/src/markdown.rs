use ratatui::text::{Line, Text};

/// Renders a small markdown subset into `ratatui::Text`.
pub(crate) fn render_markdown(input: &str) -> Text<'static> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    for raw_line in input.lines() {
        let trimmed = raw_line.trim_end();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        let rendered = if in_code_block {
            format!("    {trimmed}")
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            format!("{rest}\n{}", "=".repeat(rest.len()))
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            format!("{rest}\n{}", "-".repeat(rest.len()))
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            format!("• {}", &trimmed[2..])
        } else {
            trimmed.to_string()
        };
        lines.push(Line::from(rendered));
    }

    if lines.is_empty() {
        Text::from("")
    } else {
        Text::from(lines)
    }
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
        assert!(rendered.contains("Header"));
        assert!(rendered.contains("• item"));
    }
}
