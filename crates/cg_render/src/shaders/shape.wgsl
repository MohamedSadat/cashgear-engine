// Debug shapes and background fills: coloured triangles in world space.

struct View {
    view_proj: mat4x4<f32>,
    target_size: vec2<f32>,
    snap: f32,
    _padding: f32,
};

@group(0) @binding(0) var<uniform> view: View;

struct VertexInput {
    @location(0) position: vec2<f32>,
    // sRGB, straight alpha.
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = view.view_proj * vec4<f32>(in.position, 0.0, 1.0);
    out.color = vec4<f32>(srgb_to_linear(in.color.rgb), in.color.a);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
