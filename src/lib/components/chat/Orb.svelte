<script lang="ts">
  /**
   * The assistant's face: light moving inside a glass sphere.
   *
   * One fragment shader on a tiny canvas. Domain-warped noise churns a body
   * mixed from the theme's accent and its ground, with a brighter current
   * running through it and a glass highlight on the rim, lensed toward the
   * edge so it reads as fluid in a container rather than a picture of one.
   *
   * ## What it costs at rest: nothing
   *
   * The frame loop runs only while `live`. When the turn ends one settled
   * frame is painted and the loop is cancelled, so a window with a finished
   * conversation in it has a still picture and no timer. Reduced motion is a
   * single still frame throughout. Without WebGL the CSS gradient underneath
   * is the face and the canvas is never touched.
   *
   * ## Why the colours are read, not written
   *
   * Every colour comes from `--orb-*` in `theme.css`, read off the element
   * when the loop starts and handed to the shader as uniforms. A theme change
   * is picked up on the next live turn, and no theme has to know this exists.
   */
  import { onMount } from "svelte";

  interface Props {
    /** Inline beside a line of text, or the larger one an empty chat opens with. */
    size?: "inline" | "hero";
    /** Whether the model is working right now, which is when this moves. */
    live?: boolean;
  }

  let { size = "inline", live = false }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let drawn = $state(false);

  /** The loop's controls, set once the canvas has a context. */
  let start: (() => void) | null = null;
  let stop: (() => void) | null = null;

  $effect(() => {
    if (live) start?.();
    else stop?.();
  });

  const VERT = "attribute vec2 a; void main(){ gl_Position=vec4(a,0.,1.); }";

  const FRAG = `
precision highp float;
uniform vec2 u_res;
uniform float u_time;
uniform float u_breathe;
uniform vec3 u_deep;
uniform vec3 u_light;
uniform vec3 u_glass;
uniform float u_feather;

float hash(vec2 p){ return fract(sin(dot(p, vec2(127.1,311.7)))*43758.5453123); }
float noise(vec2 p){
  vec2 i=floor(p), f=fract(p);
  vec2 u=f*f*f*(f*(f*6.-15.)+10.);
  return mix(mix(hash(i),hash(i+vec2(1.,0.)),u.x),
             mix(hash(i+vec2(0.,1.)),hash(i+vec2(1.,1.)),u.x),u.y);
}
float fbm(vec2 p){
  float v=0., a=.55;
  for(int i=0;i<3;i++){ v+=a*noise(p); p=p*2.02+vec2(7.3,3.1); a*=.45; }
  return v;
}
void main(){
  vec2 p=(gl_FragCoord.xy*2.-u_res)/min(u_res.x,u_res.y);
  p*=1.+.08*u_breathe;
  float r=length(p);
  float t=u_time;
  float z=sqrt(max(0., 1.-r*r));
  vec2 q=p*(1.15-.4*z);
  vec2 w=vec2(fbm(q*1.0+vec2(t*.16,-t*.11)),
              fbm(q*1.0+vec2(-t*.13,t*.17)+vec2(5.2,1.3)));
  float body=fbm(q*1.1+2.4*w+vec2(t*.05));
  float vein=smoothstep(.48,.92,fbm(q*.8+2.0*w+vec2(2.7,9.1)+vec2(t*.04,-t*.03)));
  float milk=smoothstep(.5,.95,fbm(q*.8-1.8*w+vec2(11.,4.)+vec2(-t*.03,t*.04)));
  vec3 col=mix(u_deep, u_light, smoothstep(.05,.85,body));
  col=mix(col, u_light, vein*.5);
  col=mix(col, u_glass, milk*.35);
  col*=.82+.3*z;
  col+=col*.3*(1.-smoothstep(0.,1.,r));
  col*=1.-.18*smoothstep(.4,1.,r)*clamp(-p.y,0.,1.);
  col+=u_glass*pow(max(0.,1.-length(p-vec2(-.38,.45))/1.15),3.5)*.4;
  float alpha=1.-smoothstep(1.-u_feather,1.,r);
  gl_FragColor=vec4(col*alpha, alpha);
}`;

  /**
   * A CSS colour as three floats.
   *
   * A 2D canvas normalises whatever the stylesheet computed, including the
   * `color(srgb ...)` a `color-mix` resolves to, into a form this can read.
   */
  function rgb(value: string, fallback: [number, number, number]): [number, number, number] {
    try {
      const probe = document.createElement("canvas").getContext("2d");
      if (!probe) return fallback;
      probe.fillStyle = value;
      const norm = String(probe.fillStyle);

      const hex = norm.match(/^#([0-9a-f]{6})/i);
      if (hex) {
        const n = Number.parseInt(hex[1], 16);
        return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
      }

      const parts = norm.match(/rgba?\(([^)]+)\)/);
      if (parts) {
        const [r, g, b] = parts[1].split(",").map((one) => Number.parseFloat(one));
        return [r / 255, g / 255, b / 255];
      }
    } catch {
      // Fall through to the fallback below.
    }
    return fallback;
  }

  onMount(() => {
    const cv = canvas;
    if (!cv) return;

    let gl: WebGLRenderingContext | null = null;
    try {
      gl = cv.getContext("webgl", { premultipliedAlpha: true, alpha: true });
    } catch {
      gl = null;
    }
    // No WebGL: the gradient underneath is the face.
    if (!gl) return;
    const ctx = gl;

    let program: WebGLProgram;
    try {
      const compile = (type: number, src: string) => {
        const shader = ctx.createShader(type);
        if (!shader) throw new Error("no shader");
        ctx.shaderSource(shader, src);
        ctx.compileShader(shader);
        if (!ctx.getShaderParameter(shader, ctx.COMPILE_STATUS)) {
          throw new Error(ctx.getShaderInfoLog(shader) ?? "shader");
        }
        return shader;
      };
      const linked = ctx.createProgram();
      if (!linked) throw new Error("no program");
      ctx.attachShader(linked, compile(ctx.VERTEX_SHADER, VERT));
      ctx.attachShader(linked, compile(ctx.FRAGMENT_SHADER, FRAG));
      ctx.linkProgram(linked);
      if (!ctx.getProgramParameter(linked, ctx.LINK_STATUS)) {
        throw new Error(ctx.getProgramInfoLog(linked) ?? "link");
      }
      program = linked;
    } catch {
      return;
    }

    ctx.useProgram(program);
    const buffer = ctx.createBuffer();
    ctx.bindBuffer(ctx.ARRAY_BUFFER, buffer);
    ctx.bufferData(ctx.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), ctx.STATIC_DRAW);
    const at = ctx.getAttribLocation(program, "a");
    ctx.enableVertexAttribArray(at);
    ctx.vertexAttribPointer(at, 2, ctx.FLOAT, false, 0, 0);

    const uTime = ctx.getUniformLocation(program, "u_time");
    const uBreathe = ctx.getUniformLocation(program, "u_breathe");
    const uRes = ctx.getUniformLocation(program, "u_res");
    const uDeep = ctx.getUniformLocation(program, "u_deep");
    const uLight = ctx.getUniformLocation(program, "u_light");
    const uGlass = ctx.getUniformLocation(program, "u_glass");
    const uFeather = ctx.getUniformLocation(program, "u_feather");

    const paintColours = () => {
      const style = getComputedStyle(cv);
      const read = (name: string, fallback: [number, number, number]) =>
        rgb(style.getPropertyValue(name).trim(), fallback);
      ctx.uniform3fv(uDeep, read("--orb-deep", [0.2, 0.28, 0.32]));
      ctx.uniform3fv(uLight, read("--orb-light", [0.78, 0.88, 0.91]));
      ctx.uniform3fv(uGlass, read("--orb-glass", [0.9, 0.9, 0.9]));
    };

    /*
     * Drawn at least twice the size it is shown at.
     *
     * A sphere a few tens of pixels across has too few pixels for noise to
     * read as fluid and for its rim to read as a curve; drawn at 2x or 3x
     * and scaled down by the browser it is the same picture, sampled
     * finely. The cost is a few thousand extra fragments per frame, only
     * while a turn is live.
     */
    const fit = () => {
      const dpr = Math.min(Math.max(window.devicePixelRatio || 1, 2), 3);
      const rect = cv.getBoundingClientRect();
      const w = Math.max(1, Math.round(rect.width * dpr));
      const h = Math.max(1, Math.round(rect.height * dpr));
      if (cv.width !== w || cv.height !== h) {
        cv.width = w;
        cv.height = h;
        ctx.viewport(0, 0, w, h);
        ctx.uniform2f(uRes, w, h);
        // The rim softens over about two device pixels whatever the size,
        // so a small orb is not a hard-edged disc and a large one is not
        // blurred.
        ctx.uniform1f(uFeather, Math.max(0.04, 2.5 / Math.min(w, h)));
      }
    };

    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;

    // Time accumulates through the pace rather than being scaled by it, so
    // a change of pace bends the motion rather than jumping the clock. It
    // persists across turns so the fluid picks up where it settled.
    let acc = 7;
    let last = 0;
    let breathe = 0;
    let frame = 0;
    let watching: ResizeObserver | null = null;

    const paint = (now: number, pace: number, target: number) => {
      if (last) acc += ((now - last) / 1000) * pace;
      last = now;
      breathe += (target - breathe) * 0.06;
      const pulse = 0.5 + 0.5 * Math.sin((now / 1000) * 3.93);
      ctx.uniform1f(uTime, acc);
      ctx.uniform1f(uBreathe, breathe * pulse);
      ctx.drawArrays(ctx.TRIANGLES, 0, 3);
    };

    const once = () => {
      fit();
      paintColours();
      last = 0;
      breathe = 0;
      paint(performance.now(), 0, 0);
      drawn = true;
    };

    const loop = (now: number) => {
      paint(now, 1.6, 1);
      frame = requestAnimationFrame(loop);
    };

    start = () => {
      if (frame || still) {
        if (still) once();
        return;
      }
      fit();
      paintColours();
      watching = new ResizeObserver(fit);
      watching.observe(cv);
      last = 0;
      frame = requestAnimationFrame(loop);
      drawn = true;
    };

    stop = () => {
      if (!frame) return;
      cancelAnimationFrame(frame);
      frame = 0;
      watching?.disconnect();
      watching = null;
      // One settled frame, so it does not freeze mid-breath.
      last = 0;
      breathe = 0;
      paint(performance.now(), 0, 0);
    };

    once();
    if (live) start();

    return () => {
      stop?.();
      start = null;
      stop = null;
      ctx.getExtension("WEBGL_lose_context")?.loseContext();
    };
  });
</script>

<span class="orb" class:hero={size === "hero"} class:drawn aria-hidden="true">
  <canvas bind:this={canvas}></canvas>
</span>

<style>
  .orb {
    position: relative;
    display: inline-block;
    flex: none;
    width: var(--orb-inline);
    height: var(--orb-inline);
    border-radius: 50%;
    /* The face when there is no WebGL, and the ground until the first frame. */
    background: radial-gradient(
      circle at 32% 28%,
      var(--orb-light),
      var(--orb-deep) 62%,
      color-mix(in srgb, var(--orb-deep) 55%, transparent) 100%
    );
  }

  .hero {
    width: var(--orb-hero);
    height: var(--orb-hero);
  }

  /* Once the canvas has drawn, the gradient stands down: it would only bleed
     round the edge of the sphere. */
  .drawn {
    background: none;
  }

  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
