struct Globals {
    view_proj: mat4x4<f32>,
    sky: vec4<f32>,
    screen: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct WorldIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct WorldOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_world(v: WorldIn) -> WorldOut {
    var o: WorldOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    return o;
}

@fragment
fn fs_world(i: WorldOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (t.a < 0.5) {
        discard;
    }
    return vec4<f32>(t.rgb * i.color.rgb, i.color.a);
}

@fragment
fn fs_flat(i: WorldOut) -> @location(0) vec4<f32> {
    return i.color;
}

struct HudIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_hud(v: HudIn) -> WorldOut {
    var o: WorldOut;
    o.clip = vec4<f32>(v.pos.x / g.screen.x * 2.0 - 1.0, 1.0 - v.pos.y / g.screen.y * 2.0, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    return o;
}
