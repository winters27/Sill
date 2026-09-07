<script lang="ts">
  /**
   * Marking up a picture.
   *
   * Three things this gets right that the first version did not, all of which
   * showed the moment a real capture was opened in it.
   *
   * **The window is sized to the picture**, not the other way round. A five
   * hundred pixel capture sat adrift in a fixed eight hundred pixel window,
   * and the fix is not to stretch it: enlarging a screenshot past its own
   * pixels only makes it blurry and stops the marks lining up with what they
   * mark. A small capture gets a small window and fills it. A capture larger
   * than the screen is shrunk to fit, which is the only direction that is
   * ever right.
   *
   * **The window is opaque.** It inherits a page that is transparent on
   * purpose, because the launcher's acrylic needs the desktop to reach it. An
   * editor is not a launcher, and the desktop showing through the edges of one
   * reads as a broken window rather than a deliberate one.
   *
   * **A shape can be picked up again.** Drawing something you cannot then
   * move, recolour or delete means one wrong stroke costs everything after it,
   * because undo is the only way back and it takes the good marks with it.
   *
   * The picture itself is still never drawn on. Shapes are a list painted over
   * it each frame, which is what makes all of that affordable.
   */
  import { onMount } from "svelte";
  import "$lib/theme/theme.css";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { currentMonitor, getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import MarkupIcon, { type MarkIcon } from "$lib/components/MarkupIcon.svelte";
  import {
    arrowHead,
    boxOf,
    COLOURS,
    croppedTo,
    fitted,
    CAN_FILL,
    HIDE_BLOCK,
    TOOL_KEYS,
    roomFor,
    moved,
    nextNumber,
    pickedAt,
    renumbered,
    worthKeeping,
    type Point,
    type Shape,
    type Step,
    type Tool,
  } from "$lib/markup";
  import {
    cancelMarkup,
    finishMarkup,
    markupImage,
    pinShot,
    saveMarkup,
    uploadMarkup,
  } from "$lib/capture";
  import { applyAppearance, getPreferences } from "$lib/settings";
  import { hint } from "$lib/hint";

  let picture = $state<HTMLImageElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let stage = $state<HTMLDivElement | null>(null);
  let shapes = $state<Shape[]>([]);
  /**
   * What undo has taken back, newest last, for redo to put in front again.
   *
   * Emptied by anything new, which is what every editor does: once a different
   * mark exists, the marks that were undone no longer describe this picture and
   * putting one back would land it after work it used to precede.
   */
  let undone = $state<Step[]>([]);
  let drawing = $state<Shape | null>(null);
  /** Which shape is picked up, or -1. */
  let chosen = $state(-1);
  /** Where a move began, so the offset can be worked out as it goes. */
  let moving = $state<Point | null>(null);

  let tool = $state<Tool | "select">("box");
  let colour = $state(COLOURS[0].value);
  let weight = $state(4);
  /** Whether the next box or ellipse is solid rather than an outline. */
  let fill = $state(false);
  /** Whether the next click reads a colour out of the picture. */
  let dropping = $state(false);
  /** Whether an upload is in flight, so a second click cannot start another. */
  let uploading = $state(false);
  /**
   * A colour chosen from the picker, kept beside the six offered.
   *
   * One slot rather than a growing list. Somebody working on one screenshot
   * wants the colour they just picked, and a strip that grows every time it is
   * used turns the quietest row in the window into the busiest.
   */
  let spare = $state<string | null>(null);
  let status = $state("");
  let typing = $state<{ at: Point; value: string } | null>(null);
  let typingField = $state<HTMLInputElement | null>(null);

  /** The size the picture is shown at, which everything converts through. */
  let shown = $state({ width: 0, height: 0 });

  /**
   * The part of the picture being kept, or nothing for all of it.
   *
   * The picture is not trimmed. Cropping sets this and everything reads
   * through it, so a crop can be adjusted or lifted afterwards and the pixels
   * outside it are still there if somebody changes their mind. Marks stay in
   * the picture's own coordinates, which is what stops them sliding when the
   * crop moves.
   */
  let crop = $state<{ x: number; y: number; w: number; h: number } | null>(null);
  /** Where the badges start counting. */
  let stepFrom = $state(1);

  const TOOLS: { id: Tool | "select"; icon: MarkIcon; hint: string }[] = [
    { id: "select", icon: "select", hint: "Pick a mark up to move or delete" },
    { id: "box", icon: "box", hint: "Draw a rectangle" },
    { id: "ellipse", icon: "ellipse", hint: "Draw an ellipse" },
    { id: "line", icon: "line", hint: "Draw a straight line" },
    { id: "arrow", icon: "arrow", hint: "Point at something" },
    { id: "pen", icon: "pen", hint: "Draw freehand" },
    { id: "highlight", icon: "highlight", hint: "Wash over it in colour" },
    { id: "hide", icon: "hide", hint: "Cover it up, permanently" },
    { id: "text", icon: "text", hint: "Type a label" },
    { id: "step", icon: "step", hint: "Drop a numbered badge" },
    { id: "crop", icon: "crop", hint: "Trim the picture" },
  ];

  /**
   * The key that reaches a tool, for its tooltip.
   *
   * Read off `TOOL_KEYS` rather than written beside each hint, so the tooltip
   * and the key that actually works cannot say different things.
   */
  function keyFor(id: Tool | "select"): string | undefined {
    return Object.keys(TOOL_KEYS).find((key) => TOOL_KEYS[key] === id)?.toUpperCase();
  }

  $effect(() => {
    if (typing && typingField) typingField.focus();
  });

  async function load() {
    const uri = await markupImage();
    if (!uri) {
      status = "There is no picture to mark up";
      return;
    }

    const image = new Image();
    image.src = uri;
    await image.decode();

    picture = image;
    shapes = [];
    drawing = null;
    typing = null;
    chosen = -1;
    crop = null;
    status = "";

    await sizeToPicture(image);
    resize();
  }

  /**
   * Makes the window the size the picture wants.
   *
   * The window is sized to the picture rather than the picture stretched to
   * the window. Blowing a screenshot up past its own pixels only makes it
   * blurry and stops the marks lining up with what they mark, so a small
   * capture gets a small window and fills it.
   */
  async function sizeToPicture(image: HTMLImageElement) {
    const window = getCurrentWindow();

    try {
      const screen = await currentMonitor();
      const scale = await window.scaleFactor();

      // The monitor reports physical pixels and a window is set in logical
      // ones, so the working area has to come back through the scale or a
      // 150% display looks half the size it is.
      const usable = screen
        ? { width: screen.size.width / scale, height: screen.size.height / scale }
        : { width: 1280, height: 800 };

      // Room for the taskbar and the window's own edges, so it never opens
      // with its footer underneath the shelf.
      const room = roomFor(
        { width: image.naturalWidth, height: image.naturalHeight },
        { width: usable.width - 80, height: usable.height - 120 },
      );

      await window.setSize(new LogicalSize(room.width, room.height));
      await window.center();
    } catch (err) {
      // Said rather than swallowed. A window that will not resize is still a
      // usable window, but a silent catch here is how the first version looked
      // like it was ignoring the picture's size on purpose.
      status = `could not fit the window to the picture: ${err}`;
    }
  }

  /**
   * Works out how big to draw the picture, and repaints.
   *
   * Set explicitly rather than left to CSS. The pointer has to be converted
   * through this exact number, so it is one value both the layout and the
   * arithmetic read rather than one the browser decided and the other guessed.
   */
  /** The part of the picture on screen, which is the crop or all of it. */
  function visible(): { x: number; y: number; w: number; h: number } {
    return (
      crop ?? {
        x: 0,
        y: 0,
        w: picture?.naturalWidth ?? 0,
        h: picture?.naturalHeight ?? 0,
      }
    );
  }

  /**
   * The colour of one pixel of the picture, as it was captured.
   *
   * Read from a canvas holding nothing but the picture, not from the one on
   * screen. The visible canvas has every mark composited onto it, so a dropper
   * reading that one would pick up the last red box somebody drew rather than
   * the thing underneath it, which is never what was meant.
   *
   * Drawn on demand and let go afterwards: a second full-size copy of a
   * whole-screen capture is tens of megabytes, and this is used a few times a
   * session at most.
   */
  function pixelAt(point: Point): string | null {
    if (!picture) return null;

    const x = Math.floor(point.x);
    const y = Math.floor(point.y);
    if (x < 0 || y < 0 || x >= picture.naturalWidth || y >= picture.naturalHeight) return null;

    // One pixel wide, so nothing is copied but the pixel being asked about.
    const sample = document.createElement("canvas");
    sample.width = 1;
    sample.height = 1;

    const pen = sample.getContext("2d", { willReadFrequently: false });
    if (!pen) return null;

    pen.drawImage(picture, x, y, 1, 1, 0, 0, 1, 1);
    const [r, g, b] = pen.getImageData(0, 0, 1, 1).data;

    return `#${[r, g, b].map((one) => one.toString(16).padStart(2, "0")).join("")}`;
  }

  function resize() {
    if (!picture || !stage) return;

    const room = stage.getBoundingClientRect();
    const area = visible();
    shown = fitted(
      { width: area.w, height: area.h },
      // Room to breathe, so the picture is not jammed against the panel edges.
      { width: room.width - 48, height: room.height - 48 },
    );

    paint();
  }

  /** The picture's own pixels per shown pixel. */
  function ratio(): number {
    if (!picture || shown.width <= 0) return 1;
    return visible().w / shown.width;
  }

  function at(event: PointerEvent): Point {
    if (!canvas) return { x: 0, y: 0 };
    const box = canvas.getBoundingClientRect();
    const scale = ratio();
    const area = visible();

    // Plus the crop's origin, because marks are kept in the picture's own
    // coordinates rather than the cropped view's. That is what lets a crop be
    // moved or lifted afterwards without every mark sliding with it.
    return {
      x: area.x + (event.clientX - box.left) * scale,
      y: area.y + (event.clientY - box.top) * scale,
    };
  }

  function down(event: PointerEvent) {
    if (!picture) return;
    const point = at(event);
    canvas?.setPointerCapture(event.pointerId);

    // Before every tool, and it disarms itself. Picking a colour is one act,
    // not a mode: a dropper left armed turns the next mark somebody meant to
    // draw into another colour change.
    if (dropping) {
      const found = pixelAt(point);
      dropping = false;
      if (found) restyle({ colour: found });
      return;
    }

    if (tool === "select") {
      // The slack is in the picture's pixels, so picking something up is as
      // easy on a shrunk-down capture as on a full-size one.
      chosen = pickedAt(shapes, point, 8 * ratio());
      moving = chosen >= 0 ? point : null;
      paint();
      return;
    }

    if (tool === "text") {
      typing = { at: point, value: "" };
      return;
    }

    // A badge is placed rather than dragged, so it is finished on the way down.
    if (tool === "step") {
      shapes = [
        ...shapes,
        {
          tool: "step",
          colour,
          weight,
          points: [point],
          number: nextNumber(shapes, stepFrom),
        },
      ];
      undone = [];
      chosen = shapes.length - 1;
      paint();
      return;
    }

    chosen = -1;
    drawing = {
      tool,
      colour,
      weight,
      points: [point, point],
      // Carried only where it means something, so a line or an arrow never
      // arrives holding a flag nothing reads.
      ...(CAN_FILL.includes(tool) ? { fill } : {}),
    };
  }

  function move(event: PointerEvent) {
    const point = at(event);

    if (moving && chosen >= 0) {
      const shape = shapes[chosen];
      if (!shape) return;

      shapes[chosen] = moved(shape, point.x - moving.x, point.y - moving.y);
      shapes = shapes;
      moving = point;
      paint();
      return;
    }

    if (!drawing) return;

    if (drawing.tool === "pen") {
      drawing.points.push(point);
    } else {
      drawing.points[1] = point;
    }

    drawing = drawing;
    paint();
  }

  function up() {
    moving = null;

    if (!drawing) return;

    // A crop is not a mark. It trims what is shown rather than being drawn on
    // it, so it never joins the list.
    if (drawing.tool === "crop") {
      const [from, to] = drawing.points;
      const area = croppedTo(boxOf(from, to), {
        width: picture?.naturalWidth ?? 0,
        height: picture?.naturalHeight ?? 0,
      });

      drawing = null;

      if (area) {
        crop = area;
        undone = [];
        // Back to drawing: leaving the crop tool selected invites a second
        // crop nobody wanted on the next click.
        tool = "box";
        resize();
      } else {
        paint();
      }

      return;
    }

    if (worthKeeping(drawing)) {
      shapes = [...shapes, drawing];
      undone = [];
      // Left picked up, so its colour and weight can be changed straight away.
      chosen = shapes.length - 1;
    }

    drawing = null;
    paint();
  }

  /** Puts the whole picture back. */
  function uncrop() {
    crop = null;
    resize();
  }

  function commitText() {
    if (!typing) return;

    const shape: Shape = {
      tool: "text",
      colour,
      weight,
      points: [typing.at],
      text: typing.value,
    };

    if (worthKeeping(shape)) {
      shapes = [...shapes, shape];
      undone = [];
    }
    typing = null;
    paint();
  }

  function undo() {
    // A crop is the most recent thing done when there is one, so it is what
    // undo takes back. Anything else would leave no way to lift it by keyboard.
    if (crop) {
      undone = [...undone, { did: "crop", crop }];
      uncrop();
      return;
    }

    const last = shapes[shapes.length - 1];
    if (!last) return;

    undone = [...undone, { did: "mark", shape: last }];
    shapes = renumbered(shapes.slice(0, -1), stepFrom);
    chosen = -1;
    paint();
  }

  /**
   * Puts back the last thing undone.
   *
   * The stack holds crops and marks in one list because they interleave: undo a
   * crop and then a box, and redo has to give back the box first. Two stacks
   * could not say which came first.
   */
  function redo() {
    const step = undone[undone.length - 1];
    if (!step) return;

    undone = undone.slice(0, -1);

    if (step.did === "crop") {
      crop = step.crop;
      resize();
      return;
    }

    shapes = renumbered([...shapes, step.shape], stepFrom);
    chosen = -1;
    paint();
  }

  function clear() {
    shapes = [];
    chosen = -1;
    crop = null;
    // Clearing is not a step that can be walked back one at a time, and a redo
    // stack that survived it would put marks back into a picture somebody
    // deliberately emptied.
    undone = [];
    resize();
  }

  function removeChosen() {
    if (chosen < 0) return;

    // Renumbered, or deleting the second of four leaves one, three, four.
    shapes = renumbered(
      shapes.filter((_, at) => at !== chosen),
      stepFrom,
    );
    chosen = -1;
    paint();
  }

  /** Recolours or resizes whatever is picked up, or sets it for the next one. */
  function restyle(next: { colour?: string; weight?: number; fill?: boolean }) {
    if (next.colour !== undefined) {
      colour = next.colour;
      // A colour that is not one of the six gets the spare swatch, so what was
      // just dropped or picked is still one click away for the next mark.
      if (!COLOURS.some((swatch) => swatch.value === next.colour)) spare = next.colour;
    }
    if (next.weight !== undefined) weight = next.weight;
    if (next.fill !== undefined) fill = next.fill;

    if (chosen < 0 || !shapes[chosen]) return;

    // A fill on a shape that cannot take one would be a flag nothing reads and
    // a difference the picture never shows, so it is dropped rather than set.
    const picked = shapes[chosen];
    if (next.fill !== undefined && !CAN_FILL.includes(picked.tool)) {
      const { fill: _dropped, ...rest } = next;
      shapes[chosen] = { ...picked, ...rest };
    } else {
      shapes[chosen] = { ...picked, ...next };
    }

    shapes = shapes;
    paint();
  }

  function paint() {
    if (!canvas || !picture) return;

    const area = visible();
    canvas.width = area.w;
    canvas.height = area.h;

    const pen = canvas.getContext("2d");
    if (!pen) return;

    // Only the kept part of the picture is drawn, and everything after it is
    // shifted by the crop's origin so marks in the picture's coordinates land
    // where they belong in the cropped one.
    pen.drawImage(picture, area.x, area.y, area.w, area.h, 0, 0, area.w, area.h);
    pen.save();
    pen.translate(-area.x, -area.y);

    shapes.forEach((shape, at) => {
      draw(pen, shape);
      if (at === chosen) outline(pen, shape);
    });

    if (drawing) {
      // The crop being dragged is shown as a frame rather than drawn as a
      // mark: it is a choice about the picture, not something on it.
      if (drawing.tool === "crop") {
        pending(pen, drawing);
      } else {
        draw(pen, drawing);
      }
    }

    pen.restore();
  }

  /** The crop rectangle being dragged, before it is applied. */
  function pending(pen: CanvasRenderingContext2D, shape: Shape) {
    const [from, to] = shape.points;
    const box = boxOf(from, to);
    const area = visible();

    pen.save();
    // Everything outside it dimmed, which is the same language the area
    // picker uses on the screen itself.
    pen.fillStyle = "rgb(0 0 0 / 0.5)";
    pen.beginPath();
    pen.rect(area.x, area.y, area.w, area.h);
    pen.rect(box.x, box.y, box.w, box.h);
    pen.fill("evenodd");

    pen.strokeStyle = "#0a84ff";
    pen.lineWidth = Math.max(1, 1.5 * ratio());
    pen.strokeRect(box.x, box.y, box.w, box.h);
    pen.restore();
  }

  /** A dashed box around whatever is picked up, so it is obvious which it is. */
  function outline(pen: CanvasRenderingContext2D, shape: Shape) {
    const xs = shape.points.map((at) => at.x);
    const ys = shape.points.map((at) => at.y);
    const pad = Math.max(6, shape.weight * 2) * ratio();

    pen.save();
    pen.strokeStyle = "#0a84ff";
    // Scaled to the picture, or the marching ants are invisible on a large
    // capture shown small.
    pen.lineWidth = Math.max(1, 1.5 * ratio());
    pen.setLineDash([6 * ratio(), 4 * ratio()]);
    pen.strokeRect(
      Math.min(...xs) - pad,
      Math.min(...ys) - pad,
      Math.max(...xs) - Math.min(...xs) + pad * 2,
      Math.max(...ys) - Math.min(...ys) + pad * 2,
    );
    pen.restore();
  }

  function draw(pen: CanvasRenderingContext2D, shape: Shape) {
    pen.save();
    pen.strokeStyle = shape.colour;
    pen.fillStyle = shape.colour;
    pen.lineWidth = shape.weight;
    pen.lineCap = "round";
    pen.lineJoin = "round";

    const [from, to] = shape.points;

    switch (shape.tool) {
      case "box": {
        const box = boxOf(from, to);
        // Filled and then stroked, so a filled box still has its own edge and
        // reads at the same weight as an outlined one beside it.
        if (shape.fill) pen.fillRect(box.x, box.y, box.w, box.h);
        pen.strokeRect(box.x, box.y, box.w, box.h);
        break;
      }

      case "ellipse": {
        const box = boxOf(from, to);
        pen.beginPath();
        pen.ellipse(box.x + box.w / 2, box.y + box.h / 2, box.w / 2, box.h / 2, 0, 0, Math.PI * 2);
        if (shape.fill) pen.fill();
        pen.stroke();
        break;
      }

      case "line": {
        pen.beginPath();
        pen.moveTo(from.x, from.y);
        pen.lineTo(to.x, to.y);
        pen.stroke();
        break;
      }

      case "arrow": {
        pen.beginPath();
        pen.moveTo(from.x, from.y);
        pen.lineTo(to.x, to.y);
        pen.stroke();

        const [left, right] = arrowHead(from, to, shape.weight * 4);
        pen.beginPath();
        pen.moveTo(to.x, to.y);
        pen.lineTo(left.x, left.y);
        pen.lineTo(right.x, right.y);
        pen.closePath();
        pen.fill();
        break;
      }

      case "pen": {
        pen.beginPath();
        pen.moveTo(shape.points[0].x, shape.points[0].y);
        for (const point of shape.points.slice(1)) pen.lineTo(point.x, point.y);
        pen.stroke();
        break;
      }

      case "highlight": {
        // Multiply, so what is underneath still reads through it. A flat
        // translucent fill washes the text out instead of marking it.
        pen.globalCompositeOperation = "multiply";
        pen.globalAlpha = 0.4;
        pen.lineWidth = shape.weight * 4;
        pen.beginPath();
        pen.moveTo(from.x, from.y);
        pen.lineTo(to.x, to.y);
        pen.stroke();
        break;
      }

      case "hide": {
        hide(pen, boxOf(from, to), shape.weight * HIDE_BLOCK);
        break;
      }

      case "step": {
        /*
         * A filled disc with the number in it.
         *
         * Filled rather than outlined: a badge sits on top of a screenshot and
         * has to be legible over whatever is behind it, and an outline lets
         * the picture through the middle of the digit.
         */
        const radius = Math.max(10, shape.weight * 3.5);
        const [middle] = shape.points;

        pen.beginPath();
        pen.arc(middle.x, middle.y, radius, 0, Math.PI * 2);
        pen.fill();

        // A ring in the same dark the text labels are outlined with, so a
        // badge on a light picture still has an edge.
        pen.strokeStyle = "rgba(0,0,0,0.35)";
        pen.lineWidth = Math.max(1, radius / 10);
        pen.stroke();

        pen.fillStyle = readableOn(shape.colour);
        pen.font = `600 ${Math.round(radius * 1.15)}px system-ui, sans-serif`;
        pen.textAlign = "center";
        pen.textBaseline = "middle";
        // A hair below centre: a digit's optical middle is not its box's.
        pen.fillText(String(shape.number ?? 1), middle.x, middle.y + radius * 0.06);
        break;
      }

      case "crop": {
        // Applied rather than drawn. Never reaches here.
        break;
      }

      case "text": {
        const size = Math.max(12, shape.weight * 6);
        pen.font = `${size}px system-ui, sans-serif`;
        pen.textBaseline = "top";

        // Outlined behind itself, so a label stays readable over a busy
        // screenshot without needing a box drawn around it.
        pen.strokeStyle = "rgba(0,0,0,0.65)";
        pen.lineWidth = Math.max(2, size / 8);
        pen.strokeText(shape.text ?? "", from.x, from.y);
        pen.fillText(shape.text ?? "", from.x, from.y);
        break;
      }
    }

    pen.restore();
  }

  /**
   * Black or white, whichever can be read on a given colour.
   *
   * The usual relative-luminance sum. A yellow badge with white digits on it
   * is unreadable, and yellow is one of the six colours offered.
   */
  function readableOn(colour: string): string {
    const hex = colour.replace("#", "");
    const to = (at: number) => parseInt(hex.slice(at, at + 2), 16) / 255;
    const channel = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);

    const light =
      0.2126 * channel(to(0)) + 0.7152 * channel(to(2)) + 0.0722 * channel(to(4));

    return light > 0.45 ? "#000000" : "#ffffff";
  }

  /**
   * Covers an area by averaging it into blocks.
   *
   * Blocks rather than a blur, deliberately. A blur is a filter somebody can
   * partly undo; averaging a block throws the pixels away and leaves nothing
   * to recover. If this is used to hide a password it has to actually hide it.
   */
  function hide(
    pen: CanvasRenderingContext2D,
    box: { x: number; y: number; w: number; h: number },
    block: number,
  ) {
    // Clamped to the picture: `getImageData` outside it throws, which is what
    // a drag that ran off the edge used to do.
    const x = Math.max(0, Math.floor(box.x));
    const y = Math.max(0, Math.floor(box.y));
    const w = Math.min(Math.ceil(box.w), (canvas?.width ?? 0) - x);
    const h = Math.min(Math.ceil(box.h), (canvas?.height ?? 0) - y);
    if (w < 1 || h < 1) return;

    const size = Math.max(4, Math.round(block));
    const area = pen.getImageData(x, y, w, h);

    for (let py = 0; py < area.height; py += size) {
      for (let px = 0; px < area.width; px += size) {
        let r = 0;
        let g = 0;
        let b = 0;
        let seen = 0;

        for (let dy = 0; dy < size && py + dy < area.height; dy++) {
          for (let dx = 0; dx < size && px + dx < area.width; dx++) {
            const at = ((py + dy) * area.width + (px + dx)) * 4;
            r += area.data[at];
            g += area.data[at + 1];
            b += area.data[at + 2];
            seen++;
          }
        }

        if (seen === 0) continue;
        r = Math.round(r / seen);
        g = Math.round(g / seen);
        b = Math.round(b / seen);

        for (let dy = 0; dy < size && py + dy < area.height; dy++) {
          for (let dx = 0; dx < size && px + dx < area.width; dx++) {
            const at = ((py + dy) * area.width + (px + dx)) * 4;
            area.data[at] = r;
            area.data[at + 1] = g;
            area.data[at + 2] = b;
            area.data[at + 3] = 255;
          }
        }
      }
    }

    pen.putImageData(area, x, y);
  }

  async function done() {
    if (!canvas) return;

    try {
      status = await finishMarkup(exported());
    } catch (err) {
      status = `${err}`;
    }
  }

  /**
   * Writes the picture to a file, leaving the editor open.
   *
   * Deliberately not a second way of finishing. Somebody who saves a copy
   * usually wants to keep working on it, and closing here would take the marks
   * with it.
   */
  async function save() {
    if (!canvas) return;

    try {
      const path = await saveMarkup(exported());
      // Nothing chosen is somebody changing their mind, and a message about it
      // would be the application arguing with them.
      if (path) status = `Saved to ${path}`;
    } catch (err) {
      status = `${err}`;
    }
  }

  /**
   * Leaves the picture floating on top of everything, and closes the editor.
   *
   * Unlike saving, this does finish. A pin and the editor are two windows
   * showing the same picture, and leaving both up is one window too many for
   * something whose whole purpose is to sit beside another program.
   */
  async function pin() {
    if (!canvas) return;

    try {
      await pinShot(exported());
      await cancelMarkup();
    } catch (err) {
      status = `${err}`;
    }
  }

  /**
   * Sends the picture to whatever service was named, and copies the link.
   *
   * The editor stays open. An upload that failed and an upload that worked
   * look the same once the window has gone, and this is the one action here
   * that reaches a machine somebody else owns.
   */
  async function share() {
    if (!canvas || uploading) return;

    uploading = true;
    status = "Uploading";

    try {
      status = `Copied ${await uploadMarkup(exported())}`;
    } catch (err) {
      // Said in full. The refusals name the settings row that fixes them, and
      // truncating that would leave somebody with "no upload service" and
      // nowhere to go.
      status = `${err}`;
    } finally {
      uploading = false;
    }
  }

  /** The picture as it should leave this window, marks and all. */
  function exported(): string {
    // Nothing picked up, or the marching ants end up in the picture.
    chosen = -1;
    paint();

    return canvas?.toDataURL("image/png") ?? "";
  }

  function key(event: KeyboardEvent) {
    if (typing) {
      if (event.key === "Enter") {
        event.preventDefault();
        commitText();
      } else if (event.key === "Escape") {
        event.preventDefault();
        typing = null;
      }
      return;
    }

    if (event.key === "Escape") {
      event.preventDefault();
      // Escape lets go of a mark before it closes the window, so it is never
      // one keystroke from losing the work.
      if (chosen >= 0) {
        chosen = -1;
        paint();
        return;
      }
      void cancelMarkup();
      return;
    }

    if ((event.key === "Delete" || event.key === "Backspace") && chosen >= 0) {
      event.preventDefault();
      removeChosen();
      return;
    }

    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
      event.preventDefault();
      // Shift+Ctrl+Z is the other half of the same key on most keyboards, and
      // is what somebody who has never looked for a redo key will try first.
      if (event.shiftKey) redo();
      else undo();
      return;
    }

    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "y") {
      event.preventDefault();
      redo();
      return;
    }

    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
      // Ahead of the tool keys, and prevented, or the browser's own save
      // dialog opens over the one Rust is about to put up.
      event.preventDefault();
      void save();
      return;
    }

    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault();
      void done();
      return;
    }

    // A tool key last, and only on its own. A bare letter is only a tool while
    // nothing is being typed, which the guard at the top of this function has
    // already established, and a modifier means it was meant as a chord.
    if (event.ctrlKey || event.metaKey || event.altKey) return;

    const wanted = TOOL_KEYS[event.key.toLowerCase()];
    if (wanted) {
      event.preventDefault();
      tool = wanted;
    }
  }

  onMount(() => {
    let off: UnlistenFn | undefined;

    // The same theme as everything else. Without it this window is the one
    // place in Sill wearing the stylesheet's defaults.
    void getPreferences()
      .then((prefs) => {
        applyAppearance(prefs);

        // Where it starts, which is a setting. Everything is still changeable
        // once it is open; this only decides what is already chosen.
        tool = (prefs.screenshot?.tool as Tool) ?? tool;
        colour = prefs.screenshot?.colour ?? colour;
        weight = prefs.screenshot?.weight ?? weight;
        stepFrom = prefs.screenshot?.stepFrom ?? stepFrom;
      })
      .catch(() => {
        // A window with the default palette is still a usable window, so this
        // is judged not worth the status surface: nothing here claims to be
        // showing somebody's settings, it just starts from the same place a
        // machine with no settings saved would.
      });

    void load();
    void listen("sill://markup", () => void load()).then((stop) => (off = stop));

    // The picture is fitted to the window, so it has to be refitted when the
    // window changes size.
    const watching = new ResizeObserver(() => resize());
    if (stage) watching.observe(stage);

    return () => {
      off?.();
      watching.disconnect();
    };
  });
</script>

<svelte:window onkeydown={key} />

<div class="markup">
  <TitleBar />

  <header>
    <div class="group">
      {#each TOOLS as option (option.id)}
        <button
          class="icon"
          class:on={tool === option.id}
          use:hint={keyFor(option.id) ? `${option.hint} (${keyFor(option.id)})` : option.hint}
          aria-label={option.hint}
          aria-pressed={tool === option.id}
          onclick={() => (tool = option.id)}
        >
          <MarkupIcon name={option.icon} />
        </button>
      {/each}
    </div>

    <div class="group right">
      <button
        class="icon"
        use:hint={"Undo the last mark (Ctrl Z)"}
        aria-label="Undo the last mark"
        onclick={undo}
        disabled={shapes.length === 0 && !crop}
      >
        <MarkupIcon name="undo" />
      </button>
      <button
        class="icon"
        use:hint={"Put back what was undone (Ctrl Y)"}
        aria-label="Put back what was undone"
        onclick={redo}
        disabled={undone.length === 0}
      >
        <MarkupIcon name="redo" />
      </button>
      {#if crop}
        <!-- Only while there is one, because there is nothing to undo
             otherwise and a permanently dead button is noise. -->
        <button
          class="icon"
          use:hint={"Show the whole picture again"}
          aria-label="Show the whole picture again"
          onclick={uncrop}
        >
          <MarkupIcon name="crop" />
        </button>
      {/if}
      <button
        class="icon"
        use:hint={"Remove every mark"}
        aria-label="Remove every mark"
        onclick={clear}
        disabled={shapes.length === 0}
      >
        <MarkupIcon name="clear" />
      </button>
      <!--
        Nine marks in a row read as one undifferentiated strip. The four to
        the left change the picture; the four to the right decide where it
        goes. A hairline is what this design system separates things with.
      -->
      <span class="split"></span>

      <button
        class="icon"
        use:hint={"Upload it and copy the link"}
        aria-label="Upload it and copy the link"
        onclick={() => void share()}
        disabled={uploading}
      >
        <MarkupIcon name="share" />
      </button>
      <button
        class="icon"
        use:hint={"Leave it floating on top of everything"}
        aria-label="Leave it floating on top of everything"
        onclick={() => void pin()}
      >
        <MarkupIcon name="pin" />
      </button>
      <button
        class="icon"
        use:hint={"Save it to a file (Ctrl S)"}
        aria-label="Save it to a file"
        onclick={() => void save()}
      >
        <MarkupIcon name="save" />
      </button>
      <button
        class="icon"
        use:hint={"Close without keeping it"}
        aria-label="Close without keeping it"
        onclick={() => void cancelMarkup()}
      >
        <MarkupIcon name="close" />
      </button>
      <button class="keep" use:hint={"Copy the marked-up picture"} onclick={() => void done()}>
        <MarkupIcon name="copy" />
        Copy
        <span class="sill-key on-accent">Ctrl</span><span class="sill-key on-accent">↵</span>
      </button>
    </div>
  </header>

  <div class="stage" bind:this={stage}>
    {#if picture}
      <div class="frame" style:width="{shown.width}px" style:height="{shown.height}px">
        <canvas
          bind:this={canvas}
          style:width="{shown.width}px"
          style:height="{shown.height}px"
          class:picking={tool === "select"}
          class:dropping
          onpointerdown={down}
          onpointermove={move}
          onpointerup={up}
          onpointercancel={up}
        ></canvas>

        {#if typing}
          <input
            class="typing"
            bind:value={typing.value}
            bind:this={typingField}
            style:left="{typing.at.x / ratio()}px"
            style:top="{typing.at.y / ratio()}px"
            style:color={colour}
            placeholder="Type, then Enter"
            onblur={commitText}
          />
        {/if}
      </div>
    {/if}
  </div>

  <footer>
    <div class="group">
      {#each COLOURS as swatch (swatch.value)}
        <button
          class="swatch"
          class:on={colour === swatch.value}
          style:background={swatch.value}
          use:hint={swatch.name}
          aria-label={swatch.name}
          aria-pressed={colour === swatch.value}
          onclick={() => restyle({ colour: swatch.value })}
        ></button>
      {/each}

      <!--
        The seventh swatch is whatever was dropped or picked, and it only
        appears once there is one. An empty slot sitting in the row from the
        first frame is a control that looks broken until it is used.
      -->
      {#if spare}
        <button
          class="swatch"
          class:on={colour === spare}
          style:background={spare}
          use:hint={spare}
          aria-label="The colour taken out of the picture, {spare}"
          aria-pressed={colour === spare}
          onclick={() => restyle({ colour: spare as string })}
        ></button>
      {/if}

      <button
        class="icon"
        class:on={dropping}
        use:hint={"Take the next colour out of the picture"}
        aria-label="Take the next colour out of the picture"
        aria-pressed={dropping}
        onclick={() => (dropping = !dropping)}
      >
        <MarkupIcon name="dropper" />
      </button>

      <!--
        The browser's own picker, which is the one somebody already knows how
        to use and the only one that reaches a colour Sill did not think of.
      -->
      <label class="custom" use:hint={"Any other colour"}>
        <MarkupIcon name="palette" />
        <input
          type="color"
          value={colour}
          aria-label="Any other colour"
          oninput={(e) => restyle({ colour: e.currentTarget.value })}
        />
      </label>
    </div>

    <label class="weight">
      <MarkupIcon name="pen" size={14} />
      <input
        type="range"
        min="1"
        max="12"
        value={weight}
        aria-label="Stroke width"
        oninput={(e) => restyle({ weight: Number(e.currentTarget.value) })}
      />
      <span class="number">{weight}</span>
    </label>

    <!--
      Only while it means something. A fill switch beside the arrow tool is a
      control that does nothing, and one that does nothing here reads as one
      that is broken rather than as one that does not apply.
    -->
    {#if CAN_FILL.includes(tool as Tool) || (chosen >= 0 && CAN_FILL.includes(shapes[chosen].tool))}
      <button
        class="icon"
        class:on={fill}
        use:hint={"Fill the shape instead of outlining it"}
        aria-label="Fill the shape instead of outlining it"
        aria-pressed={fill}
        onclick={() => restyle({ fill: !fill })}
      >
        <MarkupIcon name="fill" />
      </button>
    {/if}

    <!--
      Keys are drawn as keys, using the same `.sill-key` cap the launcher and
      the tray menu use. Writing "Ctrl+Z" as prose in the one window that
      happens to be newest is how a design language stops being one.
    -->
    <p class="note">
      {#if status}
        {status}
      {:else if chosen >= 0}
        Drag to move it, <span class="sill-key">Del</span> to remove it
      {:else if tool === "select"}
        Click a mark to pick it up
      {:else if tool === "crop"}
        Drag the part to keep
      {:else if tool === "step"}
        Click to drop badge {nextNumber(shapes, stepFrom)}
      {:else if undone.length > 0}
        <span class="sill-key">Ctrl</span><span class="sill-key">Y</span> puts it back
      {:else}
        <span class="sill-key">Ctrl</span><span class="sill-key">Z</span> undoes,
        <span class="sill-key">Ctrl</span><span class="sill-key">↵</span> copies
      {/if}
    </p>
  </footer>
</div>

<style>
  .markup {
    display: flex;
    flex-direction: column;
    /* Pinned to the viewport rather than sized to it. `100vh` and the window's
       actual client height are not always the same number, and the difference
       came out as a footer cut off along the bottom edge. */
    position: fixed;
    inset: 0;
    /* Stated, not inherited. The page under this is transparent on purpose so
       the launcher's acrylic reaches it, and an editor that lets the desktop
       through its edges reads as a broken window. */
    background: var(--core-background);
    color: var(--text-1);
  }

  header,
  footer {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    padding: var(--space-2) var(--space-3);
    flex: none;
  }

  header {
    border-bottom: 1px solid var(--hairline);
  }

  footer {
    border-top: 1px solid var(--hairline);
  }

  .group {
    display: flex;
    gap: var(--space-1);
    align-items: center;
  }

  .right {
    margin-left: auto;
  }

  .split {
    width: 1px;
    height: var(--control-height);
    margin: 0 var(--space-1);
    background: var(--hairline);
  }

  .icon {
    display: grid;
    place-items: center;
    width: 30px;
    height: var(--control-height);
    padding: 0;
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-2);
    cursor: default;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .icon:hover:not(:disabled) {
    background: var(--fill-1);
    color: var(--text-1);
  }

  .icon.on {
    background: var(--accent);
    color: var(--core-background);
  }

  .icon:disabled {
    opacity: 0.35;
  }

  .keep {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    height: var(--control-height);
    padding: 0 var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--accent);
    color: var(--core-background);
    font: inherit;
    font-size: var(--text-meta);
    cursor: default;
  }

  .stage {
    flex: 1;
    display: grid;
    place-items: center;
    min-height: 0;
    /* Fitted to this box, so there is never anything to scroll to. */
    overflow: hidden;
  }

  .frame {
    position: relative;
    border-radius: var(--radius-md);
    /* The picture's own edge, which a dark screenshot on a dark window
       otherwise does not have. */
    box-shadow: var(--ring-outside-strong), var(--elevation-popover);
    overflow: hidden;
  }

  canvas {
    display: block;
    cursor: crosshair;
  }

  canvas.picking {
    cursor: default;
  }

  /* Last, so it wins over `.picking`: the dropper is armed over every tool. */
  canvas.dropping {
    cursor: copy;
  }

  /*
    The browser's colour input is a button in every engine and cannot be
    restyled into one, so the swatch is the label around it and the input is
    stretched over it invisibly. Clicking anywhere on the mark opens the
    picker, which is what the label is for.
  */
  .custom {
    position: relative;
    display: grid;
    place-items: center;
    width: var(--control-height);
    height: var(--control-height);
    border-radius: var(--radius-sm);
    color: var(--text-2);
    cursor: default;
  }

  .custom:hover {
    color: var(--text-1);
    background: var(--fill-1);
  }

  .custom input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    border: 0;
    opacity: 0;
    cursor: default;
  }

  .swatch {
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    box-shadow: var(--ring-shade);
    cursor: default;
  }

  .swatch.on {
    /* The toolbar is painted in --core-background rather than the settings
       window's secondary surface, so the ring's gap follows it. */
    --ring-gap: var(--core-background);
    box-shadow: var(--ring-shade), var(--focus-ring-gapped);
  }

  .weight {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    color: var(--text-2);
  }

  .weight input {
    width: 110px;
  }

  .number {
    min-width: 2ch;
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
  }

  .note {
    display: flex;
    gap: var(--space-1);
    align-items: center;
    margin: 0 0 0 auto;
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  /*
   * A key cap sitting on the accent, which is a lighter ground than the one
   * `.sill-key` is drawn for. Without this the cap is a dark smudge on a
   * bright button, which is the same mistake as writing the key as prose.
   */
  .keep :global(.sill-key.on-accent) {
    background: var(--shade-1);
    box-shadow: none;
    color: inherit;
  }

  .typing {
    position: absolute;
    padding: var(--space-half) var(--space-1);
    border: 1px dashed currentColor;
    border-radius: var(--radius-sm);
    background: var(--shade-3);
    font: inherit;
    outline: none;
  }
</style>
