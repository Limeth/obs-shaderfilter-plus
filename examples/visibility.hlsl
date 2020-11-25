// This example demonstrates the usage of `builtin_elapsed_time_since_shown`
// which is the number of seconds since the source was made visible.
float4 render(float2 uv) {
    float4 image_color = image.Sample(builtin_texture_sampler, uv);
    return lerp(float4(0.0, 0.0, 0.0, 1.0), image_color, builtin_elapsed_time_since_shown / 5.0);
}
