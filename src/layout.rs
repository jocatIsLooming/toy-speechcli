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
    pub frame: Rect,
    pub content: Rect,
}

const MIN_OUTER_MARGIN_X: u16 = 4;
const MIN_OUTER_MARGIN_Y: u16 = 1;
const FRAME_HORIZONTAL_PADDING: u16 = 2;
const FRAME_VERTICAL_PADDING: u16 = 1;
const PREFERRED_CONTENT_WIDTH: u16 = 80;
const MIN_CONTENT_WIDTH: u16 = 10;
const MIN_CONTENT_HEIGHT: u16 = 1;
const MIN_FRAME_WIDTH_RATIO_NUMERATOR: u16 = 4;
const MIN_FRAME_WIDTH_RATIO_DENOMINATOR: u16 = 5;

impl Layout {
    pub fn centered_panel(cols: u16, rows: u16) -> Self {
        let safe_cols = cols.max(MIN_CONTENT_WIDTH + 2);
        let safe_rows = rows.max(MIN_CONTENT_HEIGHT + 2);

        let available_width = safe_cols.saturating_sub(MIN_OUTER_MARGIN_X.saturating_mul(2));
        let available_height = safe_rows.saturating_sub(MIN_OUTER_MARGIN_Y.saturating_mul(2));

        let minimum_ratio_width = safe_cols
            .saturating_mul(MIN_FRAME_WIDTH_RATIO_NUMERATOR)
            / MIN_FRAME_WIDTH_RATIO_DENOMINATOR;
        let preferred_frame_width = PREFERRED_CONTENT_WIDTH
            .saturating_add(FRAME_HORIZONTAL_PADDING.saturating_mul(2))
            .saturating_add(2)
            .max(minimum_ratio_width);
        let frame_width = preferred_frame_width
            .min(available_width.max(MIN_CONTENT_WIDTH + 2))
            .max((MIN_CONTENT_WIDTH + 2).max(minimum_ratio_width.min(available_width)));
        let frame_height = available_height.max(MIN_CONTENT_HEIGHT + 2);

        let frame = Rect {
            x: safe_cols.saturating_sub(frame_width) / 2,
            y: safe_rows.saturating_sub(frame_height) / 2,
            width: frame_width,
            height: frame_height,
        };

        let max_inner_width = frame.width.saturating_sub(2);
        let max_inner_height = frame.height.saturating_sub(2);

        let content_x_padding = FRAME_HORIZONTAL_PADDING.min(max_inner_width.saturating_sub(1) / 2);
        let content_y_padding = FRAME_VERTICAL_PADDING.min(max_inner_height.saturating_sub(1) / 2);

        let content = Rect {
            x: frame.x.saturating_add(1).saturating_add(content_x_padding),
            y: frame.y.saturating_add(1).saturating_add(content_y_padding),
            width: frame
                .width
                .saturating_sub(2)
                .saturating_sub(content_x_padding.saturating_mul(2))
                .max(MIN_CONTENT_WIDTH.min(max_inner_width)),
            height: frame
                .height
                .saturating_sub(2)
                .saturating_sub(content_y_padding.saturating_mul(2))
                .max(MIN_CONTENT_HEIGHT.min(max_inner_height)),
        };

        Self { frame, content }
    }
}
