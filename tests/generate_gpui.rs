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
fn unsupported_input_behavior_is_a_source_loss() {
    let directory =
        std::env::temp_dir().join(format!("css-gpui-unsupported-input-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let html = directory.join("inputs.html");
    let css = directory.join("email.css");
    fs::write(
        &html,
        "<body>\n  <input type=\"email\">\n  <input DISABLED>\n  <input type=\"text\" readonly>\n</body>\n",
    )
    .unwrap();
    fs::write(&css, "").unwrap();

    let strict = run_example("generate_gpui", &[], &html, &css);
    let permissive = run_example("generate_gpui", &["--allow-loss"], &html, &css);
    let inspected = run_example("inspect", &[], &html, &css);
    fs::remove_dir_all(&directory).unwrap();

    assert!(
        !strict.status.success(),
        "strict conversion accepted unsupported form controls"
    );
    assert!(strict.stdout.is_empty());
    assert!(permissive.status.success());
    assert!(!permissive.stdout.is_empty());

    for (line_number, behavior) in [(2, "email"), (3, "disabled"), (4, "readonly")] {
        let location = format!("{}:{line_number}:3:", html.display());
        for output in [&strict, &permissive] {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.lines().any(|line| line.starts_with(&location)
                    && line.contains("source loss")
                    && line.contains(behavior)),
                "expected a source loss at {location} for {behavior}, got {stderr:?}"
            );
        }
    }
    assert!(inspected.status.success());
    let scene =
        css_gpui::ir::SceneIr::from_json(&String::from_utf8(inspected.stdout).unwrap()).unwrap();
    assert_eq!(
        scene.nodes.len(),
        3,
        "unsupported inputs should remain in the IR"
    );
    assert!(scene.nodes.iter().all(|node| node.control_kind.is_none()));
}

#[test]
fn interaction_paint_survives_scene_json_and_unsupported_states_are_reported() {
    let directory =
        std::env::temp_dir().join(format!("css-gpui-state-paint-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let html = directory.join("paint.html");
    let css = directory.join("paint.css");
    fs::write(&html, "<main><button id='paint'>Paint</button></main>").unwrap();
    let supported = "
        button { background: #123456; color: #282828; }
        button:hover { background-color: #234567; color: #343434; }
        #paint:focus { background-color: #456789; }
        #paint:hover:focus { background-color: #123456; }
    ";
    fs::write(&css, supported).unwrap();

    let direct = run_example("generate_gpui", &[], &html, &css);
    assert!(
        direct.status.success(),
        "{}",
        String::from_utf8_lossy(&direct.stderr)
    );
    let inspected = run_example("inspect", &[], &html, &css);
    assert!(inspected.status.success());
    let scene = directory.join("scene.json");
    fs::write(&scene, inspected.stdout).unwrap();
    let replayed = run_example("generate_gpui", &[], Path::new("--scene-json"), &scene);
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    assert_eq!(replayed.stdout, direct.stdout);
    let scene = css_gpui::ir::SceneIr::from_json(&fs::read_to_string(scene).unwrap()).unwrap();
    let css = css_gpui::emitter::emit(&scene).css;
    assert!(css.lines().any(|line| line.ends_with(":hover:focus {")));
    assert!(css.contains("color: #282828;"));

    let unsupported = directory.join("unsupported.css");
    fs::write(
        &unsupported,
        format!(
            "{supported}\n\
        button:hover {{ width: 80px; }}\n\
        button:hover {{ background: linear-gradient(black, white); }}\n\
        button:hover {{ background-color: var(--undefined); }}\n\
        main:hover button {{ color: #345678; }}\n\
        button:focus-visible {{ background-color: #234567; }}\n\
    "
        ),
    )
    .unwrap();
    fs::write(
        &html,
        "<main><button id='paint'>Paint</button><button disabled>Disabled</button></main>",
    )
    .unwrap();
    let strict = run_example("generate_gpui", &[], &html, &unsupported);
    fs::remove_dir_all(&directory).unwrap();

    assert!(!strict.status.success());
    assert!(strict.stdout.is_empty());
    let errors = String::from_utf8_lossy(&strict.stderr);
    for (line_number, declaration) in [
        (7, "width"),
        (8, "background"),
        (9, "background-color"),
        (10, "main:hover button"),
        (11, "button:focus-visible"),
    ] {
        let location = format!("{}:{line_number}:", unsupported.display());
        assert!(
            errors.lines().any(|line| line.starts_with(&location)
                && line.contains("source loss")
                && line.contains(declaration)),
            "{errors}"
        );
    }
    assert!(
        errors
            .lines()
            .any(|line| line.starts_with(&format!("{}:1:", html.display()))
                && line.contains("source loss")
                && line.contains("disabled")),
        "{errors}"
    );
}
