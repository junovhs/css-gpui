# Compiler design

The starting point is SEMUI's historical Scene IR compiler at commit `e946774eb9f743b38a477d911b4c7d26e123d7a0`. This import contains its Rust compiler modules, their tests, and the source HTML/CSS for eleven fixtures. The HTML emitter gives us a reference implementation of the target interface. Historical identifiers in the imported code refer to that snapshot.

| Stage | Code | Responsibility |
| --- | --- | --- |
| Read the source | `src/source_graph/`, `src/source_graph.rs` | Parse elements, text, selectors, and declarations. Keep document order and CSS spans. |
| Resolve styles | `src/resolver/`, `src/diagnostics/` | Apply the supported cascade, shorthands, and inheritance. Identify recognized unsupported selectors, properties, and values. |
| Build a scene | `src/layout/`, `src/extractor/`, `src/ir/` | Normalize declared pixel geometry and box sizing. Carry hierarchy, text, control kind, paint, type, and supported flex properties into Scene IR. |
| Emit a target | `src/target/`, `src/emitter/` | Give each emitter a resolved scene. Declare capabilities and losses. The existing emitter produces HTML and CSS. |

## Preserve the reason an element moves

Scene IR already carries flex direction, gap, and some alignment. Its justification enum currently has one value, `Center`. The toolbar requires `SpaceBetween` to survive diagnostics, extraction, serialization, the reference emitter, and the eventual GPUI emitter. An explicit pixel width in the design can remain a length. Space distributed by a flex parent should remain a rule. An absent intrinsic width should reach the backend as an unconstrained dimension.

The GPUI emitter should consume Scene IR through `TargetEmitter` and generate readable Rust. Pin a GPUI revision when the first emitter lands and compile output against it. Keep source-level losses distinct from target capability gaps. A dropped CSS value upstream is invisible to an emitter that only sees the scene.

## Verify what users see

SEMUI's browser comparisons exposed defaults such as button padding and borders that an internal round trip missed. We carried over the fixture inputs and Rust tests. Browser-to-GPUI comparison still needs its own harness. Run the browser from the approved source, render the generated component with the same font, scale, and viewport, and compare matched element bounds and images. A JSON round trip is valuable for schema regressions. It cannot tell us whether a native button sits in the right place.

The inherited parser handles a declared subset. For example, CSS at-rules raise a parse error, and compound selector support is limited. The diagnostics pass operates on parsed rules. A clean run on arbitrary web content is insufficient evidence of fidelity. Expand the support contract one behavior and one fixture at a time.
