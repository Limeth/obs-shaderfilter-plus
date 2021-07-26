// This example demonstrates the usage of `builtin_elapsed_time_since_shown`
// which is the number of seconds since the source was made visible.
float4 render(float2 uv) {
    float4 image_color = image.Sample(builtin_texture_sampler, uv);
    float active_time;

    if (uv.x < 1.0 / 3.0) {
        // Left third: Modulate alpha by time since shown.
        // The timer is reset by toggling the visibility of the source this
        // filter is applied to.
        active_time = builtin_elapsed_time_since_shown;
    } else if (uv.x > 2.0 / 3.0) {
        // Right third: Modulate alpha by time since enabled.
        // The timer is reset by toggling the visibility of the filter itself.
        active_time = builtin_elapsed_time_since_enabled;
    } else {
        // Middle third: Modulate alpha by minimum of both.
        active_time = min(builtin_elapsed_time_since_shown, builtin_elapsed_time_since_enabled);
    }

    float alpha_coefficient = active_time / 10.0;

    image_color.a *= min(1.0, alpha_coefficient);

    return image_color;
}
