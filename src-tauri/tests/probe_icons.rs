//! A sheet of candidate icons, so one can be chosen by looking at it.
#[test]
#[ignore]
fn contact_sheet() {
    // One cache for this probe, rather than a process-wide one.
    let icons = sill_lib::icons::Icons::new(None);

    use base64::Engine;

    let file = std::env::var("FILE").unwrap();
    let from: i32 = std::env::var("FROM")
        .unwrap_or_else(|_| "0".into())
        .parse()
        .unwrap();
    let count: i32 = std::env::var("COUNT")
        .unwrap_or_else(|_| "64".into())
        .parse()
        .unwrap();
    let out = std::env::var("OUT").unwrap();

    const CELL: usize = 32;
    const COLS: usize = 8;
    let rows = (count as usize).div_ceil(COLS);
    let (w, h) = (COLS * CELL, rows * CELL);

    // White, so a dark glyph reads and the grid is obvious.
    let mut canvas = vec![255u8; w * h * 4];
    let mut found = 0;

    for step in 0..count {
        let index = from + step;
        let Some(uri) = icons.data_uri(&format!("{file},{index}")) else {
            continue;
        };
        let Some(b64) = uri.split(',').next_back() else {
            continue;
        };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else {
            continue;
        };

        let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
        let Ok(mut reader) = decoder.read_info() else {
            continue;
        };
        let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
        let Ok(info) = reader.next_frame(&mut buf) else {
            continue;
        };
        if info.color_type != png::ColorType::Rgba {
            continue;
        }

        found += 1;
        let cell = step as usize;
        let (ox, oy) = ((cell % COLS) * CELL, (cell / COLS) * CELL);
        let scale = info.width as usize / CELL.max(1);
        let scale = scale.max(1);

        for y in 0..CELL.min(info.height as usize / scale) {
            for x in 0..CELL.min(info.width as usize / scale) {
                let src = ((y * scale) * info.width as usize + x * scale) * 4;
                if src + 3 >= buf.len() {
                    continue;
                }
                let a = f32::from(buf[src + 3]) / 255.0;
                let dst = ((oy + y) * w + ox + x) * 4;
                for c in 0..3 {
                    let over = f32::from(buf[src + c]) * a + 255.0 * (1.0 - a);
                    canvas[dst + c] = over as u8;
                }
            }
        }
    }

    let writer = std::fs::File::create(&out).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(writer), w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&canvas)
        .unwrap();

    println!("  {found} icons from {file} starting at {from}, laid out {COLS} per row");
    println!("  {out}");
}
