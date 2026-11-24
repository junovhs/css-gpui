# Founding brief

AI assistants are already good at designing UI with CSS. A designer can ask for a denser toolbar, a quieter secondary action, or more space around a title, then judge the result in a browser. We want that iteration loop for native Rust applications built with GPUI. Today a developer who approves the prototype still has to reconstruct the design by hand. That handoff is the work css-gpui should do.

## The authoring loop

1. Give an assistant the component's purpose, example content, and supported HTML/CSS features. Have it produce a small HTML tree and a stylesheet.
2. Open the component in a browser. Try different widths and realistic copy. Revise the CSS until the behavior is right.
3. Convert the approved source into ordinary GPUI Rust. Review layout, styling, and any reported losses in the same change as the generated code.
4. Wire application data and actions in Rust. Keep the approved source so a later design change can produce a reviewable diff.

The model makes design decisions at authoring time. Conversion should be deterministic and should run without a model or API key. The shipped application should render with GPUI. The browser earns its place as the design preview and independent reference during development.

## What makes the output worth keeping

A row with a trailing action should still work when the window grows or the title changes. Generated Rust should express that relationship where a GPUI developer would expect to find it. A second conversion of the same input should produce the same source. A developer should be able to own the result, including the choice to stop regenerating it.

The first audience is someone building a GPUI app who already has a CSS prototype. One component that survives a resize will teach us more than a long table of property translations. A precise diagnostic is also a useful outcome for an AI author. It can revise one declaration and try again.

## Product boundary

The first release targets static component structure, text, styling, and layout. A button in the source gives us a control to represent. Its label and placement alone cannot tell us which application action to dispatch. JavaScript behavior, application state inference, and whole-site migration are outside the first milestone.
