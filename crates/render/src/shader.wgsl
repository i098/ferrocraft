struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    fog_color: vec4<f32>,
    // x: fog start, y: fog end, z: sun brightness, w: torch flicker factor
    fog: vec4<f32>,
    screen: vec4<f32>,
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

// OverworldProvider.lightBrightnessTable with level normalised to 0..1.
fn brightness(level: f32) -> f32 {
    let f = 1.0 - level;
    return (1.0 - f) / (f * 3.0 + 1.0);
}

// EntityRenderer.updateLightmap for the overworld at gamma 0.
fn lightmap(sky: f32, block: f32) -> vec3<f32> {
    let f2 = brightness(sky) * g.fog.z;
    let f3 = brightness(block) * g.fog.w;
    let f4 = f3 * ((f3 * 0.6 + 0.4) * 0.6 + 0.4);
    let f5 = f3 * (f3 * f3 * 0.6 + 0.4);
    return clamp(vec3<f32>(f2 + f3, f2 + f4, f2 + f5) * 0.96 + 0.03, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_world(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (t.a < 0.1) {
        discard;
    }
    var rgb = t.rgb * i.color.rgb * lightmap(i.light.x, i.light.y);
    let d = length(i.world - g.eye.xyz);
    let fog = clamp((g.fog.y - d) / (g.fog.y - g.fog.x), 0.0, 1.0);
    rgb = mix(g.fog_color.rgb, rgb, fog);
    return vec4<f32>(rgb, t.a * i.color.a);
}

// Sky, stars, sun, moon and clouds: no light map, no fog.
@fragment
fn fs_sky(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    return vec4<f32>(t.rgb * i.color.rgb, t.a * i.color.a);
}

