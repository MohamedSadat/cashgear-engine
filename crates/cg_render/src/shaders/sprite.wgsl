// Instanced sprites: each instance is the unit square transformed into the world.

struct View {
    view_proj: mat4x4<f32>,
    // Size in pixels of the image being drawn into, for pixel snapping.
    target_size: vec2<f32>,
    // 1.0 to snap each sprite's origin corner to a pixel boundary, 0.0 not to.
    snap: f32,
    _padding: f32,
};

@group(0) @binding(0) var<uniform> view: View;
@group(1) @binding(0) var sprite_texture: texture_2d<f32>;
@group(1) @binding(1) var sprite_sampler: sampler;

struct Instance {
    @location(0) x_axis: vec2<f32>,
    @location(1) y_axis: vec2<f32>,
    @location(2) translation: vec2<f32>,
    // Texture coordinates at the left, bottom, right and top edges.
    @location(3) uv: vec4<f32>,
    // sRGB tint, straight alpha.
    @location(4) color: vec4<f32>,
    // sRGB flash colour; alpha is how far to mix towards it.
    @location(5) flash: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // Linear flash colour and amount.
    @location(2) flash: vec4<f32>,
};

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32, instance: Instance) -> VertexOutput {
    // Two triangles over the unit square.
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let corner = corners[index];
    let world = instance.translation + instance.x_axis * corner.x + instance.y_axis * corner.y;
    var clip = view.view_proj * vec4<f32>(world, 0.0, 1.0);

    if view.snap > 0.5 {
        // Shift the whole sprite so its origin corner lands on a pixel boundary. Axis-aligned
        // sprites with whole-texel sizes then map texels to pixels exactly.
        let origin = view.view_proj * vec4<f32>(instance.translation, 0.0, 1.0);
        let pixel = (origin.xy * 0.5 + 0.5) * view.target_size;
        let snapped = floor(pixel + 0.5);
        clip = vec4<f32>(clip.xy + (snapped - pixel) / view.target_size * 2.0, clip.zw);
    }

    var out: VertexOutput;
    out.position = clip;
    out.uv = mix(instance.uv.xy, instance.uv.zw, corner);
    out.color = vec4<f32>(srgb_to_linear(instance.color.rgb), instance.color.a);
    out.flash = vec4<f32>(srgb_to_linear(instance.flash.rgb), instance.flash.a);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(sprite_texture, sprite_sampler, in.uv) * in.color;
    // A flash amount of 0 leaves the colour exactly as it is.
    return vec4<f32>(mix(color.rgb, in.flash.rgb, in.flash.a), color.a);
}
