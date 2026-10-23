fn main() {
    println!("cargo:rerun-if-changed=locales");
    let mut catalogs: Vec<_> = std::fs::read_dir("locales")
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
        .collect();
    catalogs.sort();
    let entries = catalogs
        .iter()
        .map(|path| {
            format!(
                "include_bytes!({:?}).as_slice()",
                std::fs::canonicalize(path).unwrap()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("locales.rs"),
        format!("&[{entries}] as &[&[u8]]"),
    )
    .unwrap();
    println!("cargo:rerun-if-changed=assets/app/sequoia.svg");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let tree = resvg::usvg::Tree::from_str(
        include_str!("assets/app/sequoia.svg"),
        &resvg::usvg::Options::default(),
    )
    .expect("application SVG");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(256, 256).unwrap();
    let scale = 256. / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let rgba: Vec<_> = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let pixel = pixel.demultiply();
            [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()]
        })
        .collect();
    let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("sequoia.ico");
    image::save_buffer_with_format(
        &path,
        &rgba,
        256,
        256,
        image::ColorType::Rgba8,
        image::ImageFormat::Ico,
    )
    .unwrap();
    winresource::WindowsResource::new()
        .set_icon(path.to_str().unwrap())
        .set("ProductName", "Sequoia")
        .set("FileDescription", "Sequoia media player")
        .compile()
        .expect("Windows icon resource");
}
