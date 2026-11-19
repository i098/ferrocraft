struct Globals {
    view_proj: mat4x4<f32>,
    // xyz: eye position, w: gamma
    eye: vec4<f32>,
    fog_color: vec4<f32>,
    // x: fog start (negative: exponential fog), y: fog end (or density), z: sun brightness, w: torch flicker factor
    fog: vec4<f32>,
    screen: vec4<f32>,
    // rgb: tint of a hit entity (`setBrightness`), a: its strength
    hurt: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct WorldIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) light: vec2<f32>,
};
struct WorldOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) light: vec2<f32>,
    @location(3) world: vec3<f32>,
};

@vertex
fn vs_world(v: WorldIn) -> WorldOut {
    var o: WorldOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.light = v.light;
    o.world = v.pos;
    return o;
}

// A line of `drawSelectionBoundingBox` (`glLineWidth(2.0)`) as a 2 px wide quad: `pos` is this
// end, `uv` and `light.x` the far end, `light.y` the side (-1 or 1). `g.screen.xy` is the target size.
@vertex
fn vs_thick(v: WorldIn) -> WorldOut {
    var a = g.view_proj * vec4<f32>(v.pos, 1.0);
    var b = g.view_proj * vec4<f32>(v.uv, v.light.x, 1.0);
    var o: WorldOut;
    o.uv = vec2<f32>(0.0);
    o.color = v.color;
    o.light = vec2<f32>(1.0);
    o.world = v.pos;
    // Clip each end to the near plane so the direction stays finite.
    if (a.w < 0.001 && b.w < 0.001) {
        o.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        return o;
    }
    if (a.w < 0.001) {
        a = a + (b - a) * ((0.001 - a.w) / (b.w - a.w));
    } else if (b.w < 0.001) {
        b = a + (b - a) * ((0.001 - a.w) / (b.w - a.w));
    }
    let pa = a.xy / a.w;
    let d = (b.xy / b.w - pa) * g.screen.xy;
    var n = vec2<f32>(0.0);
    if (dot(d, d) > 0.0) {
        n = normalize(vec2<f32>(-d.y, d.x));
    }
    // One pixel to each side is two ndc units per `screen` pixels.
    let ndc = pa + n * v.light.y * 2.0 / g.screen.xy;
    o.clip = vec4<f32>(ndc * a.w, a.z, a.w);
    return o;
}

// OverworldProvider.lightBrightnessTable with level normalised to 0..1.
fn brightness(level: f32) -> f32 {
    let f = 1.0 - level;
    return (1.0 - f) / (f * 3.0 + 1.0);
}

// EntityRenderer.updateLightmap for the overworld.
fn lightmap(sky: f32, block: f32) -> vec3<f32> {
    let sun = g.fog.z * 0.95 + 0.05;
    let f1 = brightness(sky) * sun;
    let f2 = brightness(block) * g.fog.w;
    let f3 = f1 * (g.fog.z * 0.65 + 0.35);
    let f6 = f2 * ((f2 * 0.6 + 0.4) * 0.6 + 0.4);
    let f7 = f2 * (f2 * f2 * 0.6 + 0.4);
    var c = vec3<f32>(f3 + f2, f3 + f6, f1 + f7) * 0.96 + 0.03;
    // A Wither (screen.w is `skyDarkness`): red x0.7, green and blue x0.6 at full darkness.
    if (g.screen.w > 0.0) {
        c = c * (1.0 - g.screen.w) + c * vec3<f32>(0.7, 0.6, 0.6) * g.screen.w;
    }
    c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    // Night Vision (screen.z is its brightness, 0 without the effect): scale toward the brightest channel at 1.
    if (g.screen.z > 0.0) {
        let scale = min(1.0 / c.r, min(1.0 / c.g, 1.0 / c.b));
        c = c * (1.0 - g.screen.z) + c * scale * g.screen.z;
    }
    let inv = vec3<f32>(1.0) - c;
    let m = c * (1.0 - g.eye.w) + (vec3<f32>(1.0) - inv * inv * inv * inv) * g.eye.w;
    return clamp(m * 0.96 + 0.03, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_world(i: WorldOut) -> @location(0) vec4<f32> {
    // Vertex alpha 3 marks the `CUTOUT` layer: no mip levels. Both samples run in uniform control flow.
    let unmipped = i.color.a > 2.5;
    let t = select(textureSample(tex, samp, i.uv), textureSampleLevel(tex, samp, i.uv, 0.0), unmipped);
    // Vertex alpha 2 marks Fast-graphics leaves: a solid block, drawn without the alpha test.
    let solid = i.color.a > 1.5 && !unmipped;
    if (t.a < 0.1 && !solid) {
        discard;
    }
    // Entities flashing red carry +2 on the block light (`entity::HURT_FLAG`).
    let hurt = i.light.y > 1.5;
    let block = select(i.light.y, i.light.y - 2.0, hurt);
    var rgb = t.rgb * i.color.rgb * lightmap(i.light.x, block);
    if (hurt) {
        // `setBrightness`: GL_INTERPOLATE of the red constant over the lit texture by 0.3.
        rgb = mix(rgb, g.hurt.rgb, g.hurt.a);
    }
    let d = length(i.world - g.eye.xyz);
    var fog = clamp((g.fog.y - d) / (g.fog.y - g.fog.x), 0.0, 1.0);
    if (g.fog.x < 0.0) {
        // In a liquid: exponential fog of density g.fog.y.
        fog = exp(-g.fog.y * d);
    }
    rgb = mix(g.fog_color.rgb, rgb, fog);
    return vec4<f32>(rgb, select(t.a * i.color.a, 1.0, solid || unmipped));
}

// Entity fire ignores lighting but keeps world fog and alpha testing.
@fragment
fn fs_fire(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (t.a < 0.1) {
        discard;
    }
    let d = length(i.world - g.eye.xyz);
    let fog = clamp((g.fog.y - d) / (g.fog.y - g.fog.x), 0.0, 1.0);
    return vec4<f32>(mix(g.fog_color.rgb, t.rgb, fog), t.a);
}

// `Render.renderShadow`: the shadow texture with the vertex alpha, blended, under world fog.
@fragment
fn fs_shadow(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    let d = length(i.world - g.eye.xyz);
    let fog = clamp((g.fog.y - d) / (g.fog.y - g.fog.x), 0.0, 1.0);
    return vec4<f32>(mix(g.fog_color.rgb, t.rgb, fog), t.a * i.color.a);
}

// The hand glint: like fs_sky, drawn with an equal depth test so it follows the item's shape.
@fragment
fn fs_glint(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    return vec4<f32>(t.rgb * i.color.rgb, t.a * i.color.a);
}

@fragment
fn fs_flat(i: WorldOut) -> @location(0) vec4<f32> {
    return i.color;
}

// Sky, stars, sun and moon: no light map, no fog.
@fragment
fn fs_sky(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    return vec4<f32>(t.rgb * i.color.rgb, t.a * i.color.a);
}

// `renderSky` plane and dark discs: vertex color in the world fog, linear from 0 (`setupFog(-1)`), no light map.
@fragment
fn fs_sky_fog(i: WorldOut) -> @location(0) vec4<f32> {
    let d = length(i.world - g.eye.xyz);
    var fog = clamp((g.fog.y - d) / g.fog.y, 0.0, 1.0);
    if (g.fog.x < 0.0) {
        fog = exp(-g.fog.y * d);
    }
    return vec4<f32>(mix(g.fog_color.rgb, i.color.rgb, fog), i.color.a);
}

// Clouds: vertex color and alpha in the world fog (`renderClouds` runs after `setupFog(0)`), no light map.
@fragment
fn fs_cloud(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    let d = length(i.world - g.eye.xyz);
    let fog = clamp((g.fog.y - d) / (g.fog.y - g.fog.x), 0.0, 1.0);
    return vec4<f32>(mix(g.fog_color.rgb, t.rgb * i.color.rgb, fog), t.a * i.color.a);
}


// End portal star field: the texture times the vertex color in the world fog, no light map and no alpha test.
@fragment
fn fs_portal(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    let d = length(i.world - g.eye.xyz);
    let fog = clamp((g.fog.y - d) / (g.fog.y - g.fog.x), 0.0, 1.0);
    return vec4<f32>(mix(g.fog_color.rgb, t.rgb * i.color.rgb, fog), t.a * i.color.a);
}

// Beacon beam: no light map and no fog (`disableFog`, `disableLighting`), `alphaFunc(GREATER, 0.1)`.
@fragment
fn fs_beam(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    let a = t.a * i.color.a;
    if (a <= 0.1) {
        discard;
    }
    return vec4<f32>(t.rgb * i.color.rgb, a);
}

// Block crack overlay: no light map and no fog; `alphaFunc(GREATER, 0.1)` on the faded alpha.
@fragment
fn fs_crack(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    let a = t.a * i.color.a;
    if (a <= 0.1) {
        discard;
    }
    return vec4<f32>(t.rgb * i.color.rgb, a);
}
