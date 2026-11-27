use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run_example(example: &str, options: &[&str], html: &Path, css: &Path) -> Output {
    Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--locked", "--example", example, "--"])
        .args(options)
        .arg(html)
        .arg(css)
        .output()
        .expect("run css-gpui example")
}

#[test]
fn unsupported_email_input_is_a_source_loss() {
    let directory =
        std::env::temp_dir().join(format!("css-gpui-unsupported-email-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let html = directory.join("email.html");
    let css = directory.join("email.css");
    fs::write(&html, "<body>\n  <input type=\"email\">\n</body>\n").unwrap();
    fs::write(&css, "").unwrap();

    let strict = run_example("generate_gpui", &[], &html, &css);
    let permissive = run_example("generate_gpui", &["--allow-loss"], &html, &css);
    let inspected = run_example("inspect", &[], &html, &css);
    fs::remove_dir_all(&directory).unwrap();

    assert!(
        !strict.status.success(),
        "strict conversion accepted an unsupported email input"
    );
    assert!(strict.stdout.is_empty());
    assert!(permissive.status.success());
    assert!(!permissive.stdout.is_empty());

    let location = format!("{}:2:3:", html.display());
    for output in [&strict, &permissive] {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.lines().any(|line| line.starts_with(&location)
                && line.contains("source loss")
                && line.contains("email")),
            "expected a source loss at {location} for the email input, got {stderr:?}"
        );
    }
    assert!(inspected.status.success());
    let scene =
        css_gpui::ir::SceneIr::from_json(&String::from_utf8(inspected.stdout).unwrap()).unwrap();
    assert_eq!(scene.nodes.len(), 1, "the input should remain in the IR");
    assert_eq!(scene.nodes[0].source.dom_path, "0/0/0");
}
