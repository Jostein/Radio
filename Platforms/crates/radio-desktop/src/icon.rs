use tray_icon::Icon;

const SIZE: u32 = 32;

/// Four ascending bars in the app's accent colour, drawn into an RGBA buffer so
/// the port needs no icon files. A mid blue reads on both light and dark panels.
pub fn tray_icon() -> Result<Icon, String> {
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let bars: [(u32, u32, u32); 4] = [(6, 12, 21), (12, 8, 25), (18, 5, 28), (24, 10, 23)];

    for (x, top, bottom) in bars {
        for y in top..bottom {
            for dx in 0..3 {
                paint(&mut rgba, x + dx, y);
            }
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).map_err(|error| format!("The tray icon could not be created: {error}"))
}

fn paint(rgba: &mut [u8], x: u32, y: u32) {
    if x >= SIZE || y >= SIZE {
        return;
    }
    let offset = ((y * SIZE + x) * 4) as usize;
    rgba[offset] = 59;
    rgba[offset + 1] = 130;
    rgba[offset + 2] = 246;
    rgba[offset + 3] = 255;
}
