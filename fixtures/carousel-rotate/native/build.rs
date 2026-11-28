use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../source.html");
    println!("cargo:rerun-if-changed=../source.js");
    println!("cargo:rerun-if-env-changed=CAROUSEL_CSS");
    let css = env::var_os("CAROUSEL_CSS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../source.css"));
    println!("cargo:rerun-if-changed={}", css.display());
    let scene = css_gpui::carousel::compile(
        &fs::read_to_string("../source.html")?,
        &fs::read_to_string(css)?,
    )?;
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("carousel.json"), serde_json::to_vec(&scene)?)?;
    Ok(())
}
