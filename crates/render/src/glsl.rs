//! Vanilla's post-process programs are GLSL 1.20 (`attribute`, `varying`, loose `uniform`s,
//! `texture2D`, `gl_FragColor`). naga reads GLSL 4.50 only, so this rewrites both stages of a
//! program into 4.50 that shares one bind group layout.

/// One program's two stages as GLSL 4.50, with what the rewrite moved out of the sources.
pub struct Program {
    pub vertex: String,
    pub fragment: String,
    /// Binding `1 + 2 * i` is the texture and `2 + 2 * i` the sampler of `samplers[i]`.
    pub samplers: Vec<String>,
    /// GLSL initializers of uniforms (`uniform vec3 Gray = vec3(0.3, 0.59, 0.11);`): the value
    /// a uniform keeps when the program JSON does not list it.
    pub inits: Vec<(String, Vec<f32>)>,
}

/// The block that holds every non-sampler uniform of both stages, at binding 0.
pub const BLOCK: &str = "PostUniforms_";

/// The declarations of one stage, and the rest of its source.
#[derive(Default)]
struct Decls {
    /// `(type, name, initializer)` of uniforms other than samplers.
    uniforms: Vec<(String, String, Option<String>)>,
    samplers: Vec<String>,
    /// `(type, name)` of varyings.
    varyings: Vec<(String, String)>,
    body: String,
}

/// Vanilla writes one declaration per line, so a line-wise scan finds them all.
fn scan(src: &str) -> Decls {
    let mut d = Decls::default();
    for line in src.lines() {
        let t = line.trim();
        let mut words = t.strip_suffix(';').unwrap_or(t).split_whitespace();
        match words.next() {
            Some("#version" | "#extension" | "attribute") => {}
            Some("uniform") => {
                let ty = words.next().unwrap_or_default().to_owned();
                let rest = words.collect::<Vec<_>>().join(" ");
                let (name, init) = match rest.split_once('=') {
                    Some((n, i)) => (n.trim().to_owned(), Some(i.trim().to_owned())),
                    None => (rest.trim().to_owned(), None),
                };
                if ty == "sampler2D" {
                    d.samplers.push(name);
                } else {
                    d.uniforms.push((ty, name, init));
                }
            }
            Some("varying") => {
                let ty = words.next().unwrap_or_default().to_owned();
                d.varyings
                    .push((ty, words.next().unwrap_or_default().to_owned()));
            }
            _ => {
                d.body.push_str(line);
                d.body.push('\n');
            }
        }
    }
    d
}

/// Appends the entries of `from` whose name `to` does not have yet.
fn union<T: Clone>(to: &mut Vec<T>, from: &[T], name: impl Fn(&T) -> &str) {
    for x in from {
        if !to.iter().any(|y| name(y) == name(x)) {
            to.push(x.clone());
        }
    }
}

/// Both stages declare the same uniform block and texture and sampler pairs, so one bind group
/// serves the program. Render targets are stored top row first, so every sample flips `v` back
/// to GL's bottom-up rows.
pub fn translate(vsh: &str, fsh: &str) -> Program {
    let (v, f) = (scan(vsh), scan(fsh));
    let mut uniforms = v.uniforms.clone();
    union(&mut uniforms, &f.uniforms, |u| &u.1);
    let mut samplers = v.samplers.clone();
    union(&mut samplers, &f.samplers, |s| s);
    let mut varyings = v.varyings.clone();
    union(&mut varyings, &f.varyings, |v| &v.1);

    let mut head = String::from("#version 450 core\n");
    if !uniforms.is_empty() {
        head += &format!("layout(set = 0, binding = 0, std140) uniform {BLOCK} {{\n");
        for (ty, name, _) in &uniforms {
            head += &format!("    {ty} {name};\n");
        }
        head += "};\n";
    }
    for (i, s) in samplers.iter().enumerate() {
        let b = 1 + 2 * i;
        head += &format!(
            "layout(set = 0, binding = {b}) uniform texture2D {s}_tex;\n\
             layout(set = 0, binding = {}) uniform sampler {s}_smp;\n\
             #define {s} {s}_tex, {s}_smp\n",
            b + 1
        );
    }
    head += "vec4 tex2d_(texture2D t, sampler s, vec2 uv) {\n    \
             return texture(sampler2D(t, s), vec2(uv.x, 1.0 - uv.y));\n}\n";
    let stage = |d: &Decls, io: &str, extra: &str| {
        let mut s = head.clone() + extra;
        for (ty, name) in &d.varyings {
            let loc = varyings
                .iter()
                .position(|v| &v.1 == name)
                .unwrap_or_default();
            s += &format!("layout(location = {loc}) {io} {ty} {name};\n");
        }
        s + &body(&d.body)
    };
    Program {
        vertex: stage(&v, "out", "layout(location = 0) in vec4 Position;\n"),
        fragment: stage(&f, "in", "layout(location = 0) out vec4 fragColor_;\n"),
        samplers,
        inits: uniforms
            .into_iter()
            .filter_map(|(_, name, init)| Some((name, numbers(&init?))))
            .collect(),
    }
}

/// The literals of an initializer such as `vec3(-1.0,  0.0, 0.5)` or `4.0`.
fn numbers(init: &str) -> Vec<f32> {
    let args = init.split_once('(').map_or(init, |(_, a)| a);
    args.split(|c: char| matches!(c, ',' | ')') || c.is_whitespace())
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// The statements with GLSL 1.20 names replaced: `texture2D(` calls, `gl_FragColor`,
/// `__VERSION__` (so version branches take their 1.20 path) and `sampler2D` parameters.
fn body(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(i) = rest.find(|c: char| c.is_ascii_alphabetic() || c == '_') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let (word, after) = rest.split_at(end);
        rest = after;
        match word {
            "texture2D" if after.trim_start().starts_with('(') => out.push_str("tex2d_"),
            "gl_FragColor" => out.push_str("fragColor_"),
            "__VERSION__" => out.push_str("120"),
            "sampler2D" => {
                let t = after.trim_start();
                let n = t
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(t.len());
                let name = &t[..n];
                out.push_str(&format!(
                    "\n#define {name} {name}_tex, {name}_smp\ntexture2D {name}_tex, sampler {name}_smp"
                ));
                rest = &t[n..];
            }
            // Words that GLSL 4.50 reserves and 1.20 shaders use as names (`vec4 sample` in `blur.fsh`).
            "sample" | "patch" | "subroutine" | "smooth" | "flat" | "noperspective" | "layout"
            | "buffer" | "shared" | "coherent" | "volatile" | "restrict" | "readonly"
            | "writeonly" | "precise" | "filter" | "common" | "partition" | "active"
            | "resource" => {
                out.push_str(word);
                out.push('_');
            }
            w => out.push_str(w),
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const VSH: &str = "#version 120\nattribute vec4 Position;\nuniform mat4 ProjMat;\n\
        uniform vec2 OutSize;\nvarying vec2 texCoord;\nvoid main(){\n\
        gl_Position = ProjMat * vec4(Position.xy, 0.0, 1.0);\ntexCoord = Position.xy / OutSize;\n}\n";
    const FSH: &str = "#version 120\nuniform sampler2D DiffuseSampler;\n\
        uniform vec3 Gray = vec3(0.3, 0.59, 0.11);\nvarying vec2 texCoord;\n\
        vec3 f(sampler2D tex, vec2 p) { return texture2D(tex, p).rgb; }\n\
        void main(){\nvec4 sample = texture2D(DiffuseSampler, texCoord);\n\
        #if __VERSION__ >= 130\nbad;\n#endif\n\
        gl_FragColor = vec4(f(DiffuseSampler, texCoord) * Gray * sample.a, 1.0);\n}\n";

    fn parse(src: &str, stage: naga::ShaderStage) -> naga::Module {
        naga::front::glsl::Frontend::default()
            .parse(&naga::front::glsl::Options::from(stage), src)
            .unwrap_or_else(|e| panic!("{}\n{src}", e.emit_to_string(src)))
    }

    #[test]
    fn both_stages_parse_with_one_block() {
        let p = translate(VSH, FSH);
        assert_eq!(p.samplers, ["DiffuseSampler"]);
        assert_eq!(p.inits, [("Gray".to_owned(), vec![0.3, 0.59, 0.11])]);
        for (src, stage) in [
            (&p.vertex, naga::ShaderStage::Vertex),
            (&p.fragment, naga::ShaderStage::Fragment),
        ] {
            let m = parse(src, stage);
            let block = m
                .global_variables
                .iter()
                .find(|(_, g)| g.space == naga::AddressSpace::Uniform)
                .map(|(_, g)| &m.types[g.ty].inner);
            let Some(naga::TypeInner::Struct { members, .. }) = block else {
                panic!("no uniform block")
            };
            let names: Vec<_> = members.iter().map(|m| m.name.as_deref()).collect();
            assert_eq!(names, [Some("ProjMat"), Some("OutSize"), Some("Gray")]);
        }
    }

    #[test]
    fn initializers_splat_and_sign() {
        assert_eq!(numbers("4.0"), [4.0]);
        assert_eq!(numbers("vec3(-1.0,  0.0, 0.5)"), [-1.0, 0.0, 0.5]);
    }
}
