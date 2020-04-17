// A minimal shader example.
float4 render(float2 uv) {
    return image.Sample(builtin_texture_sampler, uv);
}
