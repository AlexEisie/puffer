use crate::OverlayState;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use std::fmt;
use std::sync::{Arc, Mutex};

const MIN_OVERLAY_WIDTH: u16 = 34;
const MAX_OVERLAY_WIDTH: u16 = 200;

/// Stores a generic scrollable text overlay used for settings-style slash commands.
#[derive(Clone)]
pub(crate) struct TextOverlay {
    shared: Arc<Mutex<TextOverlayState>>,
}

#[derive(Debug, Clone)]
struct TextOverlayState {
    title: String,
    body: String,
    scroll: u16,
}

impl TextOverlay {
    /// Builds a generic text overlay wrapped in `OverlayState`.
    pub(crate) fn open(title: impl Into<String>, body: impl Into<String>) -> OverlayState {
        OverlayState::Text(TextOverlay {
            shared: Arc::new(Mutex::new(TextOverlayState {
                title: title.into(),
                body: body.into(),
                scroll: 0,
            })),
        })
    }

    /// Scrolls the overlay upward by one row.
    pub(crate) fn scroll_up(&self) {
        if let Ok(mut state) = self.shared.lock() {
            state.scroll = state.scroll.saturating_sub(1);
        }
    }

    /// Scrolls the overlay downward by one row.
    pub(crate) fn scroll_down(&self) {
        if let Ok(mut state) = self.shared.lock() {
            state.scroll = state.scroll.saturating_add(1);
        }
    }

    /// Scrolls the overlay upward by one page.
    pub(crate) fn page_up(&self) {
        if let Ok(mut state) = self.shared.lock() {
            state.scroll = state.scroll.saturating_sub(30);
        }
    }

    /// Scrolls the overlay downward by one page.
    pub(crate) fn page_down(&self) {
        if let Ok(mut state) = self.shared.lock() {
            state.scroll = state.scroll.saturating_add(30);
        }
    }

    fn snapshot(&self) -> TextOverlayState {
        self.shared
            .lock()
            .map(|state| state.clone())
            .unwrap_or(TextOverlayState {
                title: "Panel".to_string(),
                body: "Overlay unavailable.".to_string(),
                scroll: 0,
            })
    }
}

impl PartialEq for TextOverlay {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }
}

impl Eq for TextOverlay {}

impl fmt::Debug for TextOverlay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TextOverlay")
            .finish_non_exhaustive()
    }
}

/// Renders a generic scrollable text overlay.
pub(crate) fn render_text_overlay(frame: &mut Frame<'_>, viewport: Rect, overlay: &TextOverlay) {
    let snapshot = overlay.snapshot();
    let width = viewport
        .width
        .saturating_sub(4)
        .clamp(MIN_OVERLAY_WIDTH, MAX_OVERLAY_WIDTH);
    let area = Rect {
        x: viewport.x + viewport.width.saturating_sub(width) / 2,
        y: viewport.y + 1,
        width,
        height: viewport.height.saturating_sub(2).max(6),
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!(
            "{}\n\n↑/↓ scroll · PgUp/PgDn page · Esc closes",
            snapshot.body
        ))
        .scroll((snapshot.scroll, 0))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(snapshot.title)
                .borders(Borders::ALL)
                .border_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        ),
        area,
    );
}
