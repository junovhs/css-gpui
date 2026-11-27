use css_gpui::paint::{Backdrop, Rect, composite_panel};

const WIDTH: u32 = 256;
const HEIGHT: u32 = 160;
const PANEL: Rect = Rect {
    x: 49,
    y: 38,
    width: 158,
    height: 84,
};

#[test]
fn backdrop_panel_updates_when_the_frame_or_panel_moves() {
    let mut rgba = Vec::with_capacity((WIDTH * HEIGHT) as usize);
    for shift in [0, 43] {
        rgba.clear();
        for _ in 0..HEIGHT {
            rgba.extend((0..WIDTH).map(|x| {
                if (x + shift) % 111 < 56 {
                    [255, 100, 32, 255]
                } else {
                    [8, 42, 230, 255]
                }
            }));
        }
        let backdrop = Backdrop::new(WIDTH, HEIGHT, &rgba, 12.0).unwrap();
        let panel = composite_panel(&backdrop, PANEL, 18.0, [11, 15, 32, 0]).unwrap();
        assert_eq!(panel[0], [0; 4]);
        let sample = panel[(PANEL.width * 40 + 6) as usize];
        assert!(sample[0] > 15 && sample[0] < 240, "{sample:?}");
        let moved = composite_panel(
            &backdrop,
            Rect {
                x: PANEL.x + 30,
                ..PANEL
            },
            18.0,
            [11, 15, 32, 0],
        )
        .unwrap();
        assert_ne!(sample, moved[(PANEL.width * 40 + 6) as usize]);
        if shift == 0 {
            assert!(sample[0] > 100, "{sample:?}");
        } else {
            assert!(sample[0] < 100, "{sample:?}");
        }
    }
}

#[test]
#[ignore = "run fixtures/neon-glass/backdrop-reference.mjs first"]
fn compares_custom_backdrop_to_chromium() {
    let reference = std::path::PathBuf::from(std::env::var_os("NEON_BACKDROP_REFERENCE").unwrap());
    for placement in [0, 1] {
        let read =
            |kind| std::fs::read(reference.join(format!("{placement}-{kind}.rgba"))).unwrap();
        let base: Vec<_> = read("scene")
            .chunks_exact(4)
            .map(|bytes| <[u8; 4]>::try_from(bytes).unwrap())
            .collect();
        let chrome = read("blurred");
        let backdrop = Backdrop::new(WIDTH, HEIGHT, &base, 12.0).unwrap();
        let panel = composite_panel(&backdrop, PANEL, 18.0, [11, 15, 32, 168]).unwrap();
        let mut absolute_error = 0u32;
        let mut worst = 0u8;
        let mut samples = 0;
        for y in 18..PANEL.height - 18 {
            for x in 18..PANEL.width - 18 {
                let actual = panel[(y * PANEL.width + x) as usize];
                let offset = 4 * ((PANEL.y as u32 + y) * WIDTH + PANEL.x as u32 + x) as usize;
                for c in 0..3 {
                    let error = actual[c].abs_diff(chrome[offset + c]);
                    worst = worst.max(error);
                    absolute_error += u32::from(error);
                    samples += 1;
                }
            }
        }
        let mean = f64::from(absolute_error) / f64::from(samples);
        println!("Backdrop {placement}: interior mean RGB error {mean:.3}, worst {worst}");
        assert!(mean <= 3.0 && worst <= 12);
    }
}
