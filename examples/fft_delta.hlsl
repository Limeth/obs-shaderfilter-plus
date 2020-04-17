// Configure builtin uniforms
// These macros are optional, but improve the user experience
#pragma shaderfilter set main__mix__description Main Mix/Track
#pragma shaderfilter set main__channel__description Main Channel
#pragma shaderfilter set main__dampening_factor_attack 0.0
#pragma shaderfilter set main__dampening_factor_release 0.0001
uniform texture2d builtin_texture_fft_main;
uniform texture2d builtin_texture_fft_main_previous;

float remap(float x, float2 from, float2 to) {
    float normalized = (x - from[0]) / (from[1] - from[0]);
    return normalized * (to[1] - to[0]) + to[0];
}

float remap_amplitude(float fft_amplitude) {
    float fft_db = 20.0 * log(fft_amplitude / 0.5) / log(10.0);

    return remap(fft_db, float2(-50, -0), float2(0, 1));
}

bool below_db(float2 uv, float fft_amplitude) {
    return 1.0 - uv.y < remap_amplitude(fft_amplitude);
}

float4 render(float2 uv) {
    float3 color = image.Sample(builtin_texture_sampler, uv).rgb;
    float fft_frequency = uv.x;
    float fft_amplitude = builtin_texture_fft_main.Sample(builtin_texture_sampler, float2(fft_frequency, 0.5)).r;
    float fft_amplitude_previous = builtin_texture_fft_main_previous.Sample(builtin_texture_sampler, float2(fft_frequency, 0.5)).r;
    float value = float(below_db(uv, fft_amplitude));
    float value_previous = float(below_db(uv, fft_amplitude_previous));

    float difference = value - value_previous;
    float rising = float(difference > 0);
    float falling = float(difference < 0);

    float4 fft_color = float4(falling, rising, 0.0, abs(difference));

    return float4(lerp(color, fft_color.rgb, fft_color.a), 1.0);
}
