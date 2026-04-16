#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn bottom(self) -> u16 {
        self.y.saturating_add(self.height.saturating_sub(1))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub sidebar_frame: Rect,
    pub sidebar_content: Rect,
    pub content: Rect,
}

const MIN_OUTER_MARGIN_X: u16 = 4;
const MIN_OUTER_MARGIN_Y: u16 = 1;
const PANEL_GAP: u16 = 2;
const SIDEBAR_MIN_WIDTH: u16 = 18;
const SIDEBAR_PREFERRED_WIDTH: u16 = 26;
const PREFERRED_CONTENT_WIDTH: u16 = 80;
const MIN_CONTENT_WIDTH: u16 = 10;
const MIN_CONTENT_HEIGHT: u16 = 1;
const MIN_CONTENT_WIDTH_RATIO_NUMERATOR: u16 = 4;
const MIN_CONTENT_WIDTH_RATIO_DENOMINATOR: u16 = 5;

impl Layout {
    pub fn centered_panel(cols: u16, rows: u16) -> Self {
        let minimum_total_width = MIN_CONTENT_WIDTH
            .saturating_add(SIDEBAR_MIN_WIDTH)
            .saturating_add(PANEL_GAP)
            .saturating_add(2);
        let safe_cols = cols.max(minimum_total_width);
        let safe_rows = rows.max(MIN_CONTENT_HEIGHT.saturating_add(2));

        let available_width = safe_cols.saturating_sub(MIN_OUTER_MARGIN_X.saturating_mul(2));
        let available_height = safe_rows.saturating_sub(MIN_OUTER_MARGIN_Y.saturating_mul(2));
        let content_area_height = available_height.max(MIN_CONTENT_HEIGHT.saturating_add(2));

        let minimum_ratio_width = safe_cols
            .saturating_mul(MIN_CONTENT_WIDTH_RATIO_NUMERATOR)
            / MIN_CONTENT_WIDTH_RATIO_DENOMINATOR;
        let reserved_content_width = PREFERRED_CONTENT_WIDTH
            .max(minimum_ratio_width)
            .max(MIN_CONTENT_WIDTH);

        let max_sidebar_width = available_width
            .saturating_sub(MIN_CONTENT_WIDTH)
            .saturating_sub(PANEL_GAP);
        let sidebar_frame_width = SIDEBAR_PREFERRED_WIDTH
            .min(max_sidebar_width)
            .max(SIDEBAR_MIN_WIDTH.min(max_sidebar_width));
        let content_width = available_width
            .saturating_sub(sidebar_frame_width)
            .saturating_sub(PANEL_GAP)
            .max(MIN_CONTENT_WIDTH);
        let total_panel_width = sidebar_frame_width
            .saturating_add(PANEL_GAP)
            .saturating_add(content_width);
        let start_x = safe_cols.saturating_sub(total_panel_width) / 2;
        let start_y = safe_rows.saturating_sub(content_area_height) / 2;

        let sidebar_frame = Rect {
            x: start_x,
            y: start_y,
            width: sidebar_frame_width,
            height: content_area_height,
        };
        let sidebar_content = Rect {
            x: sidebar_frame.x.saturating_add(1),
            y: sidebar_frame.y.saturating_add(1),
            width: sidebar_frame.width.saturating_sub(2),
            height: sidebar_frame.height.saturating_sub(2),
        };

        let content = Rect {
            x: sidebar_frame
                .x
                .saturating_add(sidebar_frame.width)
                .saturating_add(PANEL_GAP),
            y: start_y,
            width: content_width,
            height: content_area_height,
        };

        let _ = reserved_content_width;

        Self {
            sidebar_frame,
            sidebar_content,
            content,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_panel_allocates_sidebar_and_content() {
        let layout = Layout::centered_panel(120, 30);

        assert!(layout.sidebar_frame.width >= SIDEBAR_MIN_WIDTH);
        assert!(layout.sidebar_content.width > 0);
        assert!(layout.content.width >= MIN_CONTENT_WIDTH);
        assert!(layout.content.x > layout.sidebar_frame.x + layout.sidebar_frame.width);
    }
}
