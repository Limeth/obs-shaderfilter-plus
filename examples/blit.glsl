// A minimal shader example.
vec4 render(vec2 uv) {
    return image.Sample(builtin_texture_sampler, uv);
}
