use tray_icon::Icon;

const SIZE: u32 = 32;

/// What the tray icon should say at a glance, the way the macOS app swaps
/// between its waveform, arrow and radio-wave symbols.
#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    /// Sound is coming out: four ascending bars.
    Playing,
    /// Idle, paused or failed: a ring, the "not playing" mark.
    Idle,
    /// Between attempts: the same ring, broken, which reads as a loop.
    Reconnecting,
}

/// Drawn into an RGBA buffer so the port needs no icon files. A mid blue reads
/// on both light and dark panels.
pub fn tray_icon(look: Look) -> Result<Icon, String> {
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    match look {
        Look::Playing => {
            let bars: [(u32, u32, u32); 4] = [(6, 12, 21), (12, 8, 25), (18, 5, 28), (24, 10, 23)];
            for (x, top, bottom) in bars {
                for y in top..bottom {
                    for dx in 0..3 {
                        paint(&mut rgba, x + dx, y);
                    }
                }
            }
        }
        Look::Idle => {
            ring(&mut rgba, 9.5, 3.0, &[]);
            disc(&mut rgba, 3.0);
        }
        Look::Reconnecting => {
            ring(&mut rgba, 9.5, 3.0, &[(70.0, 110.0), (250.0, 290.0)]);
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

fn distance(x: u32, y: u32) -> (f32, f32) {
    let half = SIZE as f32 / 2.0;
    let dx = x as f32 + 0.5 - half;
    let dy = y as f32 + 0.5 - half;
    ((dx * dx + dy * dy).sqrt(), dy.atan2(dx).to_degrees().rem_euclid(360.0))
}

/// An annulus, optionally with wedge-shaped gaps in degrees.
fn ring(rgba: &mut [u8], radius: f32, thickness: f32, gaps: &[(f32, f32)]) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (distance, angle) = distance(x, y);
            if (distance - radius).abs() > thickness / 2.0 {
                continue;
            }
            if gaps.iter().any(|(from, to)| angle >= *from && angle <= *to) {
                continue;
            }
            paint(rgba, x, y);
        }
    }
}

fn disc(rgba: &mut [u8], radius: f32) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            if distance(x, y).0 <= radius {
                paint(rgba, x, y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(look: Look) -> usize {
        let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
        match look {
            Look::Playing => {
                for (x, top, bottom) in [(6u32, 12u32, 21u32), (12, 8, 25), (18, 5, 28), (24, 10, 23)] {
                    for y in top..bottom {
                        for dx in 0..3 {
                            paint(&mut rgba, x + dx, y);
                        }
                    }
                }
            }
            Look::Idle => {
                ring(&mut rgba, 9.5, 3.0, &[]);
                disc(&mut rgba, 3.0);
            }
            Look::Reconnecting => ring(&mut rgba, 9.5, 3.0, &[(70.0, 110.0), (250.0, 290.0)]),
        }
        rgba.chunks(4).filter(|pixel| pixel[3] == 255).count()
    }

    #[test]
    fn every_look_draws_something_and_they_differ() {
        let playing = filled(Look::Playing);
        let idle = filled(Look::Idle);
        let reconnecting = filled(Look::Reconnecting);
        assert!(playing > 50, "playing drew {playing} pixels");
        assert!(idle > 50, "idle drew {idle} pixels");
        assert!(reconnecting > 50, "reconnecting drew {reconnecting} pixels");
        assert_ne!(idle, reconnecting, "the broken ring should show gaps");
    }
}
