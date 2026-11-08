// 2D overlay: window-pixel positions, texture times vertex color. Texels with alpha <= 0.1 are
// cut like the vanilla alpha test; solid rectangles keep any alpha, as vanilla draws them untested.
struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    fog_color: vec4<f32>,
    fog: vec4<f32>,
    screen: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct GuiIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct GuiOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_gui(v: GuiIn) -> GuiOut {
    var o: GuiOut;
    o.clip = vec4<f32>(v.pos.x / g.screen.x * 2.0 - 1.0, 1.0 - v.pos.y / g.screen.y * 2.0, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    return o;
}

@fragment
fn fs_gui(i: GuiOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (t.a <= 0.1) {
        discard;
    }
    return t * i.color;
}
