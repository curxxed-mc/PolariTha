//! Renders assets/icons/polaritha.svg to the PNG sizes used by the app and the
//! desktop entry: `cargo run --example render-icon`.

fn main() {
    let svg = std::fs::read("assets/icons/polaritha.svg").expect("read svg");
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default()).expect("parse svg");
    for size in [32u32, 48, 64, 128, 256, 512] {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).unwrap();
        let s = size as f32 / tree.size().width();
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(s, s), &mut pixmap.as_mut());
        let path = format!("assets/icons/polaritha-{size}.png");
        pixmap.save_png(&path).expect("write png");
        println!("{path}");
    }
}
