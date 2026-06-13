use ratatui::style::Color;

// ── Depth palette: distinct hue per nesting level ───────────────────────────

const DEPTH_COLORS: [Color; 8] = [
    Color::Rgb(100, 180, 255), // L0 - blue
    Color::Rgb(180, 140, 255), // L1 - purple
    Color::Rgb(255, 180, 100), // L2 - orange
    Color::Rgb(100, 220, 160), // L3 - teal
    Color::Rgb(255, 130, 130), // L4 - salmon
    Color::Rgb(220, 200, 100), // L5 - gold
    Color::Rgb(130, 200, 255), // L6 - sky
    Color::Rgb(200, 160, 200), // L7 - mauve
];

const DEPTH_BG: [Color; 2] = [Color::Rgb(20, 20, 30), Color::Rgb(28, 28, 38)];

pub(super) fn depth_fg(depth: usize) -> Color {
    DEPTH_COLORS[depth % DEPTH_COLORS.len()]
}

pub(super) fn depth_bg(depth: usize) -> Color {
    DEPTH_BG[depth % 2]
}

pub(super) fn size_color(size: u64) -> Color {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB100: u64 = 100 * 1024 * 1024;
    const MB10: u64 = 10 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    if size >= GB {
        Color::Red
    } else if size >= MB100 {
        Color::Yellow
    } else if size >= MB10 {
        Color::Green
    } else if size >= MB {
        Color::Cyan
    } else {
        Color::White
    }
}

pub(super) fn bar_color(pct: f64) -> Color {
    if pct >= 75.0 {
        Color::Red
    } else if pct >= 50.0 {
        Color::Yellow
    } else if pct >= 25.0 {
        Color::Green
    } else {
        Color::Cyan
    }
}
