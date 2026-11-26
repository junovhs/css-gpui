// Run in the browser after the original application renders its sample day.
export function snapshotScene() {
    const viewport = { x: 0, y: 0, width: innerWidth, height: innerHeight };
    const nodes = [];
    const losses = new Map();
    const edges = { top: 0, right: 0, bottom: 0, left: 0 };
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 1;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    const usedColors = new Map();

    function color(css) {
        if (!usedColors.has(css)) {
            context.clearRect(0, 0, 1, 1);
            context.fillStyle = css;
            context.fillRect(0, 0, 1, 1);
            const [r, g, b, alpha] = context.getImageData(0, 0, 1, 1).data;
            const hex = [r, g, b, ...(alpha < 255 ? [alpha] : [])]
                .map(value => value.toString(16).padStart(2, "0")).join("");
            usedColors.set(css, alpha ? "#" + hex : null);
        }
        return usedColors.get(css);
    }

    function inFrame(rect) {
        return rect.width > 0 && rect.height > 0
            && rect.x < viewport.width && rect.y < viewport.height
            && rect.x + rect.width > 0 && rect.y + rect.height > 0;
    }

    function addNode(path, rect, paint = {}, text, typography, parent_id = "n0") {
        nodes.push({
            id: "n" + nodes.length,
            parent_id,
            kind: "box",
            layout: {
                position: parent_id ? "absolute" : "static",
                display: "block",
                box_sizing: "border_box",
                left: rect.x,
                top: rect.y,
                width: rect.width,
                height: rect.height,
                margin: edges,
                padding: edges,
            },
            paint,
            text_content: text,
            typography,
            source: { doc_id: 0, dom_path: path },
        });
    }

    function loss(path, property, value) {
        const key = path + "\0" + property;
        losses.set(key, { path, property, value });
    }

    // The captured fixed viewport uses the browser's used layout boxes.
    // The Scene IR carries native rectangles and text, independently of the PNG.
    const documentBackground = color(getComputedStyle(document.documentElement).backgroundColor) || "#ffffff";
    addNode("/html/body", viewport, { background_color: documentBackground }, undefined, undefined, null);

    function box(element, path, style, rect) {
        const paint = {};
        const fill = color(style.backgroundColor);
        if (fill) paint.background_color = fill;
        if (style.backgroundImage !== "none") loss(path, "background-image", style.backgroundImage);
        if (style.boxShadow !== "none") loss(path, "box-shadow", style.boxShadow);

        const sides = ["Top", "Right", "Bottom", "Left"].map(side => ({
            width: parseFloat(style["border" + side + "Width"]),
            color: color(style["border" + side + "Color"]),
            style: style["border" + side + "Style"],
        }));
        const [top, right, bottom, left] = sides;
        const sameBorder = sides.every(side => side.width === top.width && side.color === top.color && side.style === top.style);
        if (sameBorder && top.width && top.color) paint.border = { width: top.width, color: top.color };
        const radii = ["TopLeft", "TopRight", "BottomRight", "BottomLeft"].map(side => style["border" + side + "Radius"]);
        if (radii.some(radius => radius !== "0px")) {
            if (radii.every(radius => radius === radii[0]) && /^[-\d.]+(?:px|%)$/.test(radii[0])) {
                paint.border_radius = radii[0].endsWith("%")
                    ? Math.min(rect.width, rect.height) * parseFloat(radii[0]) / 100
                    : parseFloat(radii[0]);
                if (radii[0].endsWith("%") && rect.width !== rect.height) {
                    loss(path, "elliptical border-radius", radii[0]);
                }
            } else {
                loss(path, "corner-specific border-radius", radii.join(" "));
            }
        }
        if (Object.keys(paint).length) addNode(path, rect, paint);
        if (sides.some(side => side.width && side.style !== "solid")) {
            loss(path, "border-style", sides.map(side => side.style).join(" "));
        }
        if (!sameBorder) {
            const { x, y, width, height } = rect;
            const strips = [
                { x, y, width, height: top.width },
                { x: x + width - right.width, y, width: right.width, height },
                { x, y: y + height - bottom.width, width, height: bottom.width },
                { x, y, width: left.width, height },
            ];
            for (let index = 0; index < sides.length; index++) {
                const side = sides[index];
                if (side.width && side.color) addNode(path, strips[index], { background_color: side.color });
            }
        }

        for (const [property, value, initial] of [
            ["opacity", style.opacity, "1"],
            ["filter", style.filter, "none"],
            ["text-shadow", style.textShadow, "none"],
            ["transform", style.transform, "none"],
            ["mask-image", style.maskImage, "none"],
            ["clip-path", style.clipPath, "none"],
            ["mix-blend-mode", style.mixBlendMode, "normal"],
            ["z-index", style.zIndex, "auto"],
        ]) {
            if (value !== initial) loss(path, property, value);
        }

        for (const pseudo of ["::before", "::after"]) {
            const content = getComputedStyle(element, pseudo).content;
            if (content !== "none" && content !== "normal") loss(path + pseudo, "generated content", content);
        }
    }

    function textNode(node, path, style) {
        const text = node.nodeValue;
        const content = text.trim();
        const ink = color(style.color);
        if (!content || !ink) return;
        const range = document.createRange();
        range.setStart(node, text.length - text.trimStart().length);
        range.setEnd(node, text.trimEnd().length);
        const boxes = Array.from(range.getClientRects());
        if (!boxes.some(inFrame)) return;
        if (boxes.length !== 1) loss(path, "multiple text fragments", String(boxes.length));
        const rect = range.getBoundingClientRect();
        const families = style.fontFamily.split(",").map(name => name.trim().replace(/^['"]|['"]$/g, ""));
        const nativeFamilies = families.filter(name => !/^(?:ui-.+|system-ui|sans-serif|serif|monospace)$/.test(name));
        if (!nativeFamilies.length) nativeFamilies.push(families.includes("monospace") ? "Consolas" : "Segoe UI");
        if (style.letterSpacing !== "normal" && style.letterSpacing !== "0px") loss(path, "letter-spacing", style.letterSpacing);
        if (style.textDecorationLine !== "none") loss(path, "text-decoration", style.textDecorationLine);
        if (style.fontStyle !== "normal") loss(path, "font-style", style.fontStyle);

        const collapsed = /^(normal|nowrap|pre-line)$/.test(style.whiteSpace) ? content.replace(/\s+/g, " ") : content;
        const visibleText = style.textTransform === "uppercase" ? collapsed.toUpperCase()
            : style.textTransform === "lowercase" ? collapsed.toLowerCase() : collapsed;
        if (!["none", "uppercase", "lowercase"].includes(style.textTransform)) loss(path, "text-transform", style.textTransform);
        addNode(path, rect, {}, visibleText, {
            font_family: nativeFamilies,
            font_size: parseFloat(style.fontSize),
            font_weight: Math.round(Number(style.fontWeight)) || 400,
            line_height: { kind: "length", value: rect.height },
            color: ink,
        });
    }

    function walk(element, path) {
        const style = getComputedStyle(element);
        if (style.display === "none" || style.visibility !== "visible" || Number(style.opacity) === 0) return;
        const rect = element.getBoundingClientRect();
        if (inFrame(rect)) {
            if (["svg", "img", "canvas", "video", "input", "textarea"].includes(element.localName)) {
                loss(path, "element artwork", element.localName);
                return;
            }
            box(element, path, style, rect);
        } else if (["svg", "img", "canvas", "video", "input", "textarea"].includes(element.localName)) {
            return;
        }
        const indices = new Map();
        for (const child of element.childNodes) {
            const name = child.nodeType === Node.ELEMENT_NODE ? child.localName : "text()";
            if (child.nodeType !== Node.ELEMENT_NODE && child.nodeType !== Node.TEXT_NODE) continue;
            const index = (indices.get(name) || 0) + 1;
            indices.set(name, index);
            const childPath = path + "/" + name + "[" + index + "]";
            if (child.nodeType === Node.ELEMENT_NODE) walk(child, childPath);
            else textNode(child, childPath, style);
        }
    }

    walk(document.body, "/html/body");
    return {
        scene: {
            schema_version: 1, scene_id: "noceremony-today", corpus: "v0.1",
            execution_mode: "static", nodes,
        },
        losses: [...losses.values()],
        fonts: [...document.fonts].filter(font => font.status === "loaded")
            .map(font => ({ family: font.family, weight: font.weight })),
    };
}
