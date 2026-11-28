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
fn strict_generation_resolves_viewport_media() {
    let directory =
        std::env::temp_dir().join(format!("css-gpui-viewport-media-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let html = directory.join("media.html");
    let css = directory.join("media.css");
    fs::write(&html, "<main class='probe'></main>").unwrap();
    let supported = ".probe { width: 111px; height: 121px; }
@media only screen and (min-width: 1300px) { .probe { width: 131px; } }
@media (orientation: landscape) and (max-height: 520px) {
    .probe { height: 151px; }
}
@media SCREEN and (orientation: portrait) and
       (min-height: 500px) { .probe { height: 181px; } }
";
    fs::write(&css, supported).unwrap();

    let snapshots = [
        ("390x499", 111, 121),
        ("390x844", 111, 181),
        ("520x520", 111, 181),
        ("1299x520", 111, 151),
        ("1300x520", 131, 151),
        ("1300x521", 131, 121),
    ]
    .map(|(viewport, width, height)| {
        (
            viewport,
            width,
            height,
            run_example("generate_gpui", &["--viewport", viewport], &html, &css),
        )
    });
    let unspecified = run_example("generate_gpui", &[], &html, &css);
    fs::write(
        &css,
        "\
@media (min-width: 2000px) and (prefers-reduced-motion: reduce) { .probe { height: 191px; } }
@media (max-width: 480px), (max-height: 520px) { .probe { height: 192px; } }
",
    )
    .unwrap();
    let unsupported = run_example("generate_gpui", &["--viewport", "1300x520"], &html, &css);
    fs::remove_dir_all(&directory).unwrap();

    for (viewport, width, height, output) in snapshots {
        assert!(
            output.status.success(),
            "{viewport}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let rust = String::from_utf8(output.stdout).unwrap();
        assert!(rust.contains(&format!(".w(gpui::px({width}.0))")), "{rust}");
        assert!(
            rust.contains(&format!(".h(gpui::px({height}.0))")),
            "{rust}"
        );
    }
    for (output, conditions) in [
        (
            unspecified,
            &[
                (2, "only screen and (min-width: 1300px)"),
                (3, "(orientation: landscape) and (max-height: 520px)"),
            ][..],
        ),
        (
            unsupported,
            &[
                (1, "prefers-reduced-motion"),
                (2, "(max-width: 480px), (max-height: 520px)"),
            ][..],
        ),
    ] {
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let errors = String::from_utf8_lossy(&output.stderr);
        for &(line, condition) in conditions {
            let location = format!("{}:{line}:1:", css.display());
            assert!(
                errors.lines().any(|line| line.starts_with(&location)
                    && line.contains("source loss")
                    && line.contains(condition)),
                "{errors}"
            );
        }
    }
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

#[test]
fn radio_groups_and_labels_survive_scene_json_and_reference_html() {
    use css_gpui::ir::{ControlKind, NodeKind, SceneIr};

    let directory = std::env::temp_dir().join(format!("css-gpui-radio-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let html = directory.join("radios.html");
    let css = directory.join("radios.css");
    fs::write(
        &html,
        "<main>
            <label for='dark'>Dark</label>
            <input type='radio' name='theme' id='light' value='light' checked>
            <input type='radio' name='theme' id='dark' value='dark' checked='false'>
            <input type='radio' name='motion' id='off'>
            <input type='radio' name='motion' id='on'>
        </main>",
    )
    .unwrap();
    fs::write(
        &css,
        "input { appearance: none; background-color: #ffffff; }
         input:checked { background-color: #5599dd; }
         #dark:checked { background-color: #253655; }
         #on { background-color: #eeeeee !important; }",
    )
    .unwrap();

    let direct = run_example("generate_gpui", &[], &html, &css);
    assert!(
        direct.status.success(),
        "{}",
        String::from_utf8_lossy(&direct.stderr)
    );
    let inspected = run_example("inspect", &[], &html, &css);
    assert!(inspected.status.success());
    let serialized = directory.join("scene.json");
    fs::write(&serialized, &inspected.stdout).unwrap();
    let replayed = run_example("generate_gpui", &[], Path::new("--scene-json"), &serialized);
    fs::remove_dir_all(&directory).unwrap();
    assert!(
        replayed.status.success(),
        "{}",
        String::from_utf8_lossy(&replayed.stderr)
    );
    assert_eq!(replayed.stdout, direct.stdout);

    let scene = SceneIr::from_json(std::str::from_utf8(&inspected.stdout).unwrap()).unwrap();
    assert_eq!(scene.schema_version, 7);
    let radios: Vec<_> = scene
        .nodes
        .iter()
        .filter_map(|node| match &node.control_kind {
            Some(ControlKind::Radio(radio)) => Some((
                &node.id,
                radio.name.as_str(),
                radio.value.as_deref(),
                radio.checked,
                radio
                    .checked_paint
                    .background_color
                    .as_ref()
                    .map(|color| color.0.as_str()),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        radios
            .iter()
            .map(|(_, name, value, checked, fill)| (*name, *value, *checked, *fill))
            .collect::<Vec<_>>(),
        [
            ("theme", Some("light"), false, Some("#5599dd")),
            ("theme", Some("dark"), true, Some("#253655")),
            ("motion", None, false, Some("#5599dd")),
            ("motion", None, false, None),
        ]
    );
    let dark_id = radios[1].0;
    assert!(scene.nodes.iter().any(|node| {
        matches!(&node.kind, NodeKind::RadioLabel { target } if target == dark_id)
    }));
    let reference = css_gpui::emitter::emit(&scene);
    assert!(reference.html.contains(&format!("for=\"{dark_id}\"")));
    assert!(reference.html.contains(&format!(
        "id=\"{dark_id}\" name=\"theme\" value=\"dark\" checked"
    )));
    assert!(reference.css.contains(&format!(
        ".{dark_id}:checked {{\n  background-color: #253655;"
    )));
}

#[test]
fn strict_generation_reports_unsupported_radio_markup_and_checked_selectors() {
    let directory =
        std::env::temp_dir().join(format!("css-gpui-radio-losses-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let html = directory.join("radios.html");
    let css = directory.join("radios.css");
    fs::write(
        &html,
        "<main>
          <input type='radio' name='one' id='good' checked>
          <input type='radio' name='one' id='bad' disabled>
          <input type='radio' name='unreset' id='unreset'>
          <input type='radio' id='unnamed'>
          <label><input type='radio' name='implicit'>Implicit</label>
          <label for='bad'>Disabled</label>
          <label for='missing'>Unresolved</label>
          <form><input type='radio' name='form-owned'></form>
          <input type='checkbox'>
        </main>",
    )
    .unwrap();
    fs::write(
        &css,
        "#good { appearance: none; }
         #good:checked + label { background-color: #abcdef; }
         #good:hover { background-color: #abcdef; }
         #good:focus { background-color: #abcdef; }
         #good:checked:hover { background-color: #abcdef; }
         #good:checked { width: 40px; }
         input[type='checkbox']:checked { background-color: #abcdef; }",
    )
    .unwrap();

    let strict = run_example("generate_gpui", &[], &html, &css);
    fs::remove_dir_all(&directory).unwrap();
    assert!(!strict.status.success());
    assert!(strict.stdout.is_empty());
    let errors = String::from_utf8_lossy(&strict.stderr);
    for (source, line_number, behavior) in [
        (&html, 3, "disabled"),
        (&html, 4, "appearance: none"),
        (&html, 5, "nonempty literal name"),
        (&html, 6, "explicit label"),
        (&html, 7, "radio label association"),
        (&html, 8, "unresolved label"),
        (&html, 9, "form ownership"),
        (&html, 10, "checkbox"),
        (&css, 2, ":checked + label"),
        (&css, 3, ":hover"),
        (&css, 4, ":focus"),
        (&css, 5, ":checked:hover"),
        (&css, 6, "width"),
        (&css, 7, "checkbox"),
    ] {
        let location = format!("{}:{line_number}:", source.display());
        assert!(
            errors.lines().any(|line| line.starts_with(&location)
                && line.contains("source loss")
                && line.contains(behavior)),
            "{errors}"
        );
    }
}

#[test]
fn strict_generation_preserves_bounded_aspect_ratios() {
    let fixture = Path::new("benchmarks/css-compat/probes/aspect-ratio");
    let html = fixture.join("probe.html");
    let css = fixture.join("probe.css");

    let source_html = fs::read_to_string(&html).unwrap();
    let source_css = fs::read_to_string(&css).unwrap();
    let graph = css_gpui::SceneSourceGraph::from_strings_at_viewport_size(
        "aspect-ratio",
        &source_html,
        &source_css,
        400.0,
        600.0,
    )
    .unwrap();
    let resolved = css_gpui::resolver::resolve_scene_at_viewport(&graph, 400.0, 600.0).unwrap();
    let scene =
        css_gpui::extractor::extract_ir(&css_gpui::layout::compute_layout(&resolved), &graph)
            .unwrap();
    let decoded = css_gpui::ir::SceneIr::from_json(&scene.to_json().unwrap()).unwrap();
    assert_eq!(decoded.schema_version, 8);
    let phone = &decoded.nodes[1].layout;
    assert_eq!(phone.aspect_ratio, Some(941.0 / 1672.0));
    assert!(phone.width.is_some());
    assert_eq!(phone.height, None);
    assert!(
        css_gpui::emitter::emit(&decoded)
            .css
            .contains("aspect-ratio: ")
    );

    for (viewport, expected) in [
        ("400x600", [(208.2, 344.4), (90.0, 60.0), (88.0, 44.0)]),
        ("500x700", [(255.25, 428.0), (105.0, 70.0), (88.0, 44.0)]),
    ] {
        let output = run_example("generate_gpui", &["--viewport", viewport], &html, &css);
        assert!(
            output.status.success(),
            "{viewport}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let rust = String::from_utf8(output.stdout).unwrap();
        let calls: Vec<_> = rust.lines().map(str::trim).collect();
        for (width, height) in expected {
            let width = format!(".w(gpui::px({width:?}))");
            let height = format!(".h(gpui::px({height:?}))");
            assert!(
                calls
                    .windows(2)
                    .any(|pair| pair == [width.as_str(), height.as_str()]),
                "{viewport}: expected {width} and {height} in {rust}"
            );
        }
    }
}

#[test]
fn strict_generation_reports_unsupported_aspect_ratio_sources() {
    let directory =
        std::env::temp_dir().join(format!("css-gpui-aspect-ratio-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let html = directory.join("probe.html");
    let css = directory.join("probe.css");
    fs::write(
        &html,
        "<main><div id='mixed'></div><div id='invalid'></div><div id='indefinite'></div>
        <div id='prefilled'></div><div id='flex'><div id='item'></div></div>
        <div id='content'>Text</div><div id='stretched'></div><div id='unknown-size'></div>
        <div id='invalid-var'></div><div id='constrained'></div></main>",
    )
    .unwrap();
    fs::write(
        &css,
        "\
main { position: relative; }
#mixed { width: 40px; aspect-ratio: auto 2; }
#invalid { width: 40px; aspect-ratio: 1 / 0; }
#indefinite { aspect-ratio: 2; }
#prefilled { width: 40px; height: 40px; aspect-ratio: 2; }
#flex { display: flex; }
#item { width: 40px; aspect-ratio: 2; }
#content { width: 40px; aspect-ratio: 2; }
#stretched { position: absolute; top: 0; bottom: 0; left: 0; width: 40px; aspect-ratio: 2; }
#unknown-size { height: 40px; width: 50%; aspect-ratio: 2; }
#invalid-var { width: 40px; aspect-ratio: var(--missing); }
#constrained { width: 40px; min-height: 100px; aspect-ratio: 2; }
#content::before { width: 40px; aspect-ratio: 2; }
",
    )
    .unwrap();

    let output = run_example("generate_gpui", &["--viewport", "500x700"], &html, &css);
    fs::remove_dir_all(&directory).unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let errors = String::from_utf8_lossy(&output.stderr);
    for (line, reason) in [
        (2, "positive numeric"),
        (3, "positive numeric"),
        (4, "one resolved length and one auto"),
        (5, "one resolved length and one auto"),
        (7, "require a block container"),
        (8, "without in-flow content"),
        (9, "require top and left offsets"),
        (10, "one resolved length and one auto"),
        (11, "positive finite numeric ratio"),
        (12, "min/max size constraints"),
        (13, "supported static element selector"),
    ] {
        let location = format!("{}:{line}:", css.display());
        assert!(
            errors.lines().any(|line| line.starts_with(&location)
                && line.contains("source loss")
                && line.contains("aspect-ratio")
                && line.contains(reason)),
            "expected {reason:?} at {location}\n{errors}"
        );
    }
}
