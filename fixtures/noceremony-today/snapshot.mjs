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

    function addNode(path, rect, paint = {}, content = {}, parent_id = "n0") {
        nodes.push({
            id: "n" + nodes.length,
            parent_id,
            kind: content.svg ? { svg: content.svg } : content.singleLine ? "text" : "box",
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
            text_content: content.text,
            typography: content.typography,
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
    addNode("/html/body", viewport, { background_color: documentBackground }, undefined, null);

    function gradientStops(source, length) {
        // Chromium resolves colors to rgb(a). Keep other CSS forms as losses.
        const pattern = /\s*(rgba?\([^)]*\)|transparent)(?:\s+([-+.\d]+)(%|px))?\s*(,|$)/gy;
        const stops = [];
        while (pattern.lastIndex < source.length) {
            const match = pattern.exec(source);
            if (!match) return;
            const [, css, value, unit] = match;
            const rgba = /^rgba\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*([\d.]+)\s*\)$/.exec(css);
            const hex = rgba
                ? "#" + rgba.slice(1, 4).map(channel => Number(channel).toString(16).padStart(2, "0")).join("")
                : color(css);
            const alpha = rgba ? Number(rgba[4]) : hex ? 1 : 0;
            const position = unit ? Number(value) / (unit === "%" ? 100 : length) : null;
            if (position !== null && !Number.isFinite(position)) return;
            stops.push({ hex, alpha, position });
        }
        if (stops.length < 2) return;
        const visible = stops.filter(stop => stop.alpha > 0);
        if (!visible.length) return [];
        if (new Set(visible.map(stop => stop.hex)).size > 1
            && new Set(stops.map(stop => stop.alpha)).size > 1) return;
        // Preserve the visible channel when interpolating into transparency.
        for (const stop of stops) {
            if (!stop.alpha) stop.hex = visible[0].hex;
        }
        stops[0].position ??= 0;
        stops.at(-1).position ??= 1;
        let anchor = 0;
        for (let index = 1; index < stops.length; index++) {
            if (stops[index].position === null) continue;
            const from = stops[anchor].position;
            const to = Math.max(stops[index].position, from);
            for (let step = anchor + 1; step <= index; step++) {
                stops[step].position = from + (to - from) * (step - anchor) / (index - anchor);
            }
            anchor = index;
        }
        return stops;
    }

    function gradientBox(path, style, rect, radius, clip) {
        const source = /^(repeating-)?linear-gradient\((.*)\)$/.exec(style.backgroundImage);
        if (!source || !["auto", "auto auto"].includes(style.backgroundSize)
            || style.backgroundPositionX !== "0%" || style.backgroundPositionY !== "0%"
            || style.backgroundClip !== "border-box" || style.maskImage !== "none") return false;

        let args = source[2];
        const angle = /^([-+.\d]+)deg,\s*/.exec(args);
        const direction = /^to (top|right|bottom|left),\s*/.exec(args);
        const degrees = angle ? Number(angle[1]) : direction
            ? ({ top: 0, right: 90, bottom: 180, left: 270 })[direction[1]] : 180;
        if (!Number.isFinite(degrees)) return false;
        if (angle || direction) args = args.slice((angle || direction)[0].length);

        const radians = degrees * Math.PI / 180;
        const dx = Math.sin(radians);
        const dy = -Math.cos(radians);
        const { width, height } = rect;
        const length = Math.abs(width * dx) + Math.abs(height * dy);
        const stops = gradientStops(args, length);
        if (!stops) return false;
        if (!stops.length) return true;
        const first = stops[0].position;
        const last = stops.at(-1).position;
        if (first === last) return false;
        const at = position => [
            width / 2 + dx * length * (position - 0.5),
            height / 2 + dy * length * (position - 0.5),
        ];
        const [x1, y1] = at(first);
        const [x2, y2] = at(last);
        const repeat = source[1] ? ' spreadMethod="repeat"' : "";
        const stopMarkup = stops.map(stop => {
            const offset = (stop.position - first) / (last - first);
            return `<stop offset="${offset}" stop-color="${stop.hex}" stop-opacity="${stop.alpha}"/>`;
        }).join("");
        const visible = clip ? {
            x: Math.max(rect.x, clip.x),
            y: Math.max(rect.y, clip.y),
            width: Math.min(rect.x + width, clip.x + clip.width) - Math.max(rect.x, clip.x),
            height: Math.min(rect.y + height, clip.y + clip.height) - Math.max(rect.y, clip.y),
        } : rect;
        if (!inFrame(visible)) return true;
        const viewBox = [visible.x - rect.x, visible.y - rect.y, visible.width, visible.height].join(" ");
        const markup = `<svg xmlns="http://www.w3.org/2000/svg" width="${visible.width * devicePixelRatio}"
            height="${visible.height * devicePixelRatio}" viewBox="${viewBox}"><defs>
            <linearGradient id="fill" gradientUnits="userSpaceOnUse" x1="${x1}" y1="${y1}"
                x2="${x2}" y2="${y2}"${repeat}>${stopMarkup}</linearGradient></defs>
            <rect width="${width}" height="${height}" rx="${radius || 0}" opacity="${Number(style.opacity)}" fill="url(#fill)"/></svg>`;
        addNode(path, visible, {}, { svg: markup });
        return true;
    }

    function box(path, style, rect, resolvedTransform = false, clip) {
        const paint = {};
        const fill = color(style.backgroundColor);
        if (fill) paint.background_color = fill;
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
        if (style.backgroundImage !== "none"
            && (sides.some(side => side.width) || !gradientBox(path, style, rect, paint.border_radius, clip))) {
            loss(path, "background-image", style.backgroundImage);
        }
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
            ["transform", resolvedTransform ? "none" : style.transform, "none"],
            ["mask-image", style.maskImage, "none"],
            ["clip-path", style.clipPath, "none"],
            ["mix-blend-mode", style.mixBlendMode, "normal"],
            ["z-index", style.zIndex, "auto"],
        ]) {
            if (value !== initial) loss(path, property, value);
        }

    }

    function fullyClipped(style) {
        const coordinates = /^rect\(([-\d.]+)px, ([-\d.]+)px, ([-\d.]+)px, ([-\d.]+)px\)$/.exec(style.clip);
        return ((style.position === "absolute" || style.position === "fixed") && coordinates
                && (Number(coordinates[2]) <= Number(coordinates[4]) || Number(coordinates[3]) <= Number(coordinates[1])))
            || style.clipPath === "inset(50%)";
    }

    function pseudoBox(element, owner, style, path, name) {
        const pseudo = getComputedStyle(element, name);
        if (pseudo.content === "none" || pseudo.content === "normal" || pseudo.display === "none"
            || pseudo.visibility !== "visible" || Number(pseudo.opacity) === 0 || fullyClipped(pseudo)) return;

        const pseudoPath = path + name;
        if (pseudo.content !== '""') {
            loss(pseudoPath, "generated content", pseudo.content);
            return;
        }
        const hasPaint = color(pseudo.backgroundColor)
            || pseudo.backgroundImage !== "none"
            || pseudo.boxShadow !== "none"
            || ["Top", "Right", "Bottom", "Left"].some(side => parseFloat(pseudo["border" + side + "Width"]) > 0);
        if (!hasPaint) return;

        const pixels = value => /^[-\d.]+px$/.test(value) ? parseFloat(value) : NaN;
        const width = pixels(pseudo.width);
        const height = pixels(pseudo.height);
        const left = pixels(pseudo.left);
        const top = pixels(pseudo.top);
        const right = pixels(pseudo.right);
        const bottom = pixels(pseudo.bottom);
        if (pseudo.position !== "absolute" || style.position === "static"
            || !Number.isFinite(width) || !Number.isFinite(height)
            || (!Number.isFinite(left) && !Number.isFinite(right))
            || (!Number.isFinite(top) && !Number.isFinite(bottom))) {
            loss(pseudoPath, "generated box", pseudo.position);
            return;
        }

        const ownerLeft = owner.x + pixels(style.borderLeftWidth);
        const ownerTop = owner.y + pixels(style.borderTopWidth);
        const ownerRight = owner.x + owner.width - pixels(style.borderRightWidth);
        const ownerBottom = owner.y + owner.height - pixels(style.borderBottomWidth);
        const inlineExtra = pseudo.boxSizing === "border-box" ? 0
            : pixels(pseudo.paddingLeft) + pixels(pseudo.paddingRight)
                + pixels(pseudo.borderLeftWidth) + pixels(pseudo.borderRightWidth);
        const blockExtra = pseudo.boxSizing === "border-box" ? 0
            : pixels(pseudo.paddingTop) + pixels(pseudo.paddingBottom)
                + pixels(pseudo.borderTopWidth) + pixels(pseudo.borderBottomWidth);
        const rect = {
            width: width + inlineExtra,
            height: height + blockExtra,
            x: 0,
            y: 0,
        };
        rect.x = Number.isFinite(left)
            ? ownerLeft + left + (pixels(pseudo.marginLeft) || 0)
            : ownerRight - right - (pixels(pseudo.marginRight) || 0) - rect.width;
        rect.y = Number.isFinite(top)
            ? ownerTop + top + (pixels(pseudo.marginTop) || 0)
            : ownerBottom - bottom - (pixels(pseudo.marginBottom) || 0) - rect.height;
        if (pseudo.transform !== "none") {
            const transform = new DOMMatrixReadOnly(pseudo.transform);
            if (!transform.is2D || transform.b !== 0 || transform.c !== 0) {
                loss(pseudoPath, "transform", pseudo.transform);
                return;
            }
            const [originX, originY] = pseudo.transformOrigin.split(/\s+/);
            const used = (value, length) => value.endsWith("%")
                ? length * parseFloat(value) / 100
                : pixels(value);
            const transformX = used(originX, rect.width);
            const transformY = used(originY, rect.height);
            rect.x += transform.e + (1 - transform.a) * transformX;
            rect.y += transform.f + (1 - transform.d) * transformY;
            rect.width *= transform.a;
            rect.height *= transform.d;
            if (rect.width < 0) {
                rect.x += rect.width;
                rect.width = -rect.width;
            }
            if (rect.height < 0) {
                rect.y += rect.height;
                rect.height = -rect.height;
            }
        }
        const clip = ["hidden", "clip"].includes(style.overflowX) && ["hidden", "clip"].includes(style.overflowY)
            ? { x: ownerLeft, y: ownerTop, width: ownerRight - ownerLeft, height: ownerBottom - ownerTop }
            : undefined;
        if (inFrame(rect)) box(pseudoPath, pseudo, rect, true, clip);
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
        const typography = {
            font_family: nativeFamilies,
            font_size: parseFloat(style.fontSize),
            font_weight: Math.round(Number(style.fontWeight)) || 400,
            line_height: { kind: "length", value: rect.height },
            color: ink,
        };
        addNode(path, rect, {}, { text: visibleText, typography, singleLine: boxes.length === 1 });
    }

    function visibleSvg(element, path, rect) {
        const namespace = "http://www.w3.org/2000/svg";
        const allowed = new Set([
            "svg", "g", "defs", "clipPath", "mask", "linearGradient", "radialGradient", "stop",
            "path", "rect", "circle", "ellipse", "polygon", "polyline", "line",
        ]);
        const artwork = [element, ...element.querySelectorAll("*")];
        if (artwork.some(part => !allowed.has(part.localName))) {
            loss(path, "SVG artwork", "unsupported SVG element");
            return;
        }
        const presentation = [
            "fill", "fill-opacity", "fill-rule", "stroke", "stroke-width", "stroke-opacity",
            "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-dasharray",
            "stroke-dashoffset", "stop-color", "stop-opacity", "opacity", "clip-rule",
            "clip-path", "mask", "display", "visibility", "vector-effect",
        ];
        const copy = element.cloneNode(true);
        const copies = [copy, ...copy.querySelectorAll("*")];
        for (const [index, original] of artwork.entries()) {
            const clone = copies[index];
            for (const attribute of [...clone.attributes]) {
                if (/^(?:on|data-)/i.test(attribute.name) || ["class", "style", "href", "xlink:href"].includes(attribute.name)) {
                    clone.removeAttributeNode(attribute);
                } else if (/url\(/i.test(attribute.value) && !/^url\(#[-\w:.]+\)$/.test(attribute.value)) {
                    loss(path, "SVG attribute reference", attribute.name);
                    return;
                }
            }
            const style = getComputedStyle(original);
            for (const property of presentation) {
                let value = style.getPropertyValue(property);
                if (!value) continue;
                if (/^currentcolor$/i.test(value)) value = style.color;
                if (value.includes("url(")) {
                    const match = /^url\("?(?:[^#"]*)#([-\w:.]+)"?\)$/.exec(value);
                    if (!match || !artwork.some(part => part.id === match[1])) {
                        loss(path, "SVG computed reference", property);
                        return;
                    }
                    value = "url(#" + match[1] + ")";
                }
                clone.setAttribute(property, value);
            }
        }
        if (!copy.hasAttribute("viewBox")) {
            copy.setAttribute("viewBox", "0 0 " + rect.width + " " + rect.height);
        }
        copy.setAttribute("width", String(rect.width * devicePixelRatio));
        copy.setAttribute("height", String(rect.height * devicePixelRatio));
        const markup = new XMLSerializer().serializeToString(copy);
        if (copy.namespaceURI !== namespace) {
            loss(path, "SVG namespace", copy.namespaceURI);
            return;
        }
        addNode(path, rect, {}, { svg: markup });
    }

    function walk(element, path) {
        const style = getComputedStyle(element);
        if (style.display === "none" || style.visibility !== "visible" || Number(style.opacity) === 0 || fullyClipped(style)) return;
        const rect = element.getBoundingClientRect();
        if (element.localName === "svg") {
            if (inFrame(rect)) visibleSvg(element, path, rect);
            return;
        }
        if (inFrame(rect)) {
            if (["img", "canvas", "video", "input", "textarea"].includes(element.localName)) {
                loss(path, "element artwork", element.localName);
                return;
            }
            box(path, style, rect);
            pseudoBox(element, rect, style, path, "::before");
            if (Number(getComputedStyle(element, "::after").zIndex) < 0) {
                pseudoBox(element, rect, style, path, "::after");
            }
        } else if (["img", "canvas", "video", "input", "textarea"].includes(element.localName)) {
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
        if (inFrame(rect) && !(Number(getComputedStyle(element, "::after").zIndex) < 0)) {
            pseudoBox(element, rect, style, path, "::after");
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
