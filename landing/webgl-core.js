// Raymarched 3D reactor core, rendered on the GPU.
//
// Progressive enhancement, deliberately: the 2D canvas core (core.js) stays the
// baseline and this only takes over once a WebGL context, the shaders, and the
// first frame have all actually succeeded. If anything fails — no WebGL, a
// software renderer, a shader compile error, reduced-motion — the 2D core keeps
// running and the page is no worse off than before.
//
// No library. Three.js would be ~150 KB gzipped for one shader, which would
// undo the point of a ~28 KB page. This is a single fullscreen triangle pair and
// a signed-distance-field march, so the whole effect is one fragment shader.

const VERT = `#version 300 es
in vec2 p;
void main() { gl_Position = vec4(p, 0.0, 1.0); }
`;

// The reactor: a torus ring around an emissive core, lit by fresnel rim light
// and swept by a rotating band. Step count is capped low — this runs behind
// text, so it has to be cheap enough to never cost a frame.
const FRAG = `#version 300 es
precision highp float;
out vec4 fragColor;

uniform vec2  uRes;
uniform float uTime;
uniform vec3  uAccent;
uniform vec3  uAccent2;
uniform float uMotion;   // 0 = frozen (reduced motion), 1 = animating

const int   STEPS = 56;
const float MAXD  = 9.0;
const float EPS   = 0.0016;

mat2 rot(float a) { return mat2(cos(a), -sin(a), sin(a), cos(a)); }

float sdTorus(vec3 p, vec2 t) {
  vec2 q = vec2(length(p.xz) - t.x, p.y);
  return length(q) - t.y;
}

float sdSphere(vec3 p, float r) { return length(p) - r; }

// Smooth union, so the ring and core read as one cast object.
float smin(float a, float b, float k) {
  float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

float scene(vec3 p) {
  float t = uTime * uMotion;
  vec3 q = p;
  q.xz *= rot(t * 0.28);
  q.xy *= rot(0.5 + sin(t * 0.19) * 0.16);

  float ring  = sdTorus(q, vec2(1.15, 0.085));
  float inner = sdTorus(q * 1.0, vec2(0.72, 0.045));
  float core  = sdSphere(q, 0.34);

  float d = min(ring, inner);
  return smin(d, core, 0.28);
}

vec3 normalAt(vec3 p) {
  vec2 e = vec2(0.0012, 0.0);
  return normalize(vec3(
    scene(p + e.xyy) - scene(p - e.xyy),
    scene(p + e.yxy) - scene(p - e.yxy),
    scene(p + e.yyx) - scene(p - e.yyx)
  ));
}

void main() {
  vec2 uv = (gl_FragCoord.xy - 0.5 * uRes) / min(uRes.x, uRes.y);

  vec3 ro = vec3(0.0, 0.0, -3.2);
  vec3 rd = normalize(vec3(uv, 1.35));

  float d = 0.0;
  float glow = 0.0;   // accumulated proximity, a cheap stand-in for bloom
  bool hit = false;
  vec3 p = ro;

  for (int i = 0; i < STEPS; i++) {
    p = ro + rd * d;
    float s = scene(p);
    // Anything we pass close to contributes light, which is what sells the
    // volumetric feel without a second pass.
    glow += 0.014 / (0.06 + s * s * 14.0);
    if (s < EPS) { hit = true; break; }
    d += s * 0.86;
    if (d > MAXD) break;
  }

  vec3 col = vec3(0.0);

  if (hit) {
    vec3 n = normalAt(p);
    vec3 v = -rd;
    float fres = pow(1.0 - max(dot(n, v), 0.0), 2.6);
    vec3 key = normalize(vec3(0.6, 0.8, -0.5));
    float diff = max(dot(n, key), 0.0);

    // A band sweeping the surface: the "scan" that makes it feel alive.
    float sweep = smoothstep(0.86, 1.0, sin(p.y * 5.0 - uTime * uMotion * 1.7) * 0.5 + 0.5);

    col += uAccent  * (0.16 + diff * 0.42);
    col += uAccent2 * fres * 0.85;
    col += uAccent  * sweep * 0.35;
  }

  // Glow survives whether or not we hit, so the core bleeds into the dark.
  col += mix(uAccent, uAccent2, 0.35) * glow * 0.6;

  // Vignette keeps the centre bright and lets the page text stay readable.
  col *= 1.0 - 0.55 * dot(uv, uv);

  // Filmic-ish rolloff, then a touch of gamma.
  col = col / (1.0 + col);
  col = pow(col, vec3(0.82));

  fragColor = vec4(col, 1.0);
}
`;

function compile(gl, type, src) {
  const sh = gl.createShader(type);
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    gl.deleteShader(sh);
    return null;
  }
  return sh;
}

/** Reads a brand colour out of the stylesheet so the shader can't drift from it. */
function cssColor(name, fallback) {
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const hex = /^#([0-9a-f]{6})$/i.exec(raw);
  if (!hex) return fallback;
  const n = parseInt(hex[1], 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

function init() {
  const canvas = document.getElementById("hero-webgl");
  if (!canvas) return;

  const gl =
    canvas.getContext("webgl2", { antialias: false, alpha: false, powerPreference: "low-power" }) ||
    canvas.getContext("webgl", { antialias: false, alpha: false });
  if (!gl) return; // no WebGL: the 2D core stays

  const isGl2 = typeof WebGL2RenderingContext !== "undefined" && gl instanceof WebGL2RenderingContext;
  // WebGL1 needs the GLSL ES 1.00 dialect; rather than ship two shaders, we
  // simply decline and let the 2D core handle those machines.
  if (!isGl2) return;

  const vs = compile(gl, gl.VERTEX_SHADER, VERT);
  const fs = compile(gl, gl.FRAGMENT_SHADER, FRAG);
  if (!vs || !fs) return;

  const prog = gl.createProgram();
  gl.attachShader(prog, vs);
  gl.attachShader(prog, fs);
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) return;
  gl.useProgram(prog);

  // Two triangles covering clip space.
  const buf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(
    gl.ARRAY_BUFFER,
    new Float32Array([-1, -1, 3, -1, -1, 3]),
    gl.STATIC_DRAW,
  );
  const loc = gl.getAttribLocation(prog, "p");
  gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);

  const uRes = gl.getUniformLocation(prog, "uRes");
  const uTime = gl.getUniformLocation(prog, "uTime");
  const uMotion = gl.getUniformLocation(prog, "uMotion");
  gl.uniform3fv(gl.getUniformLocation(prog, "uAccent"), cssColor("--accent", [0.22, 0.78, 0.96]));
  gl.uniform3fv(gl.getUniformLocation(prog, "uAccent2"), cssColor("--accent-2", [0.34, 0.87, 0.71]));

  const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  gl.uniform1f(uMotion, reduce ? 0.0 : 1.0);

  // Render at a capped DPR: this is a soft background, and full retina here buys
  // nothing visible while costing real frames on integrated GPUs.
  const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
  const resize = () => {
    const w = Math.max(1, Math.round(canvas.clientWidth * dpr));
    const h = Math.max(1, Math.round(canvas.clientHeight * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
      gl.viewport(0, 0, w, h);
    }
    gl.uniform2f(uRes, canvas.width, canvas.height);
  };

  const draw = (ms) => {
    resize();
    gl.uniform1f(uTime, ms / 1000);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  };

  // Prove it works before committing: render one frame, and only then retire the
  // 2D core. A context that exists but draws nothing is worse than no upgrade.
  draw(0);
  if (gl.getError() !== gl.NO_ERROR) return;

  document.documentElement.classList.add("has-webgl");

  if (reduce) return; // one static frame is the whole effect under reduced motion

  let running = true;
  let raf = 0;
  const loop = (ms) => {
    if (!running) return;
    draw(ms);
    raf = requestAnimationFrame(loop);
  };
  raf = requestAnimationFrame(loop);

  // Don't burn GPU on a hidden tab or a scrolled-past hero.
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      running = false;
      cancelAnimationFrame(raf);
    } else if (!running) {
      running = true;
      raf = requestAnimationFrame(loop);
    }
  });

  if ("IntersectionObserver" in window) {
    new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (e.isIntersecting && !running) {
          running = true;
          raf = requestAnimationFrame(loop);
        } else if (!e.isIntersecting && running) {
          running = false;
          cancelAnimationFrame(raf);
        }
      }
    }).observe(canvas);
  }
}

init();
