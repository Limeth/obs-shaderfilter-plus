// Configure builtin uniforms
// These macros are optional, but improve the user experience
#pragma shaderfilter set main__mix__description Main Mix/Track
#pragma shaderfilter set main__channel__description Main Channel
#pragma shaderfilter set main__dampening_factor_attack 0.0
#pragma shaderfilter set main__dampening_factor_release 0.0
uniform texture2d builtin_texture_fft_main;

// Define configurable variables
// These macros are optional, but improve the user experience
#pragma shaderfilter set fft_color__description FFT Color
#pragma shaderfilter set fft_color__default 7FFF00FF
uniform float4 fft_color;

float remap(float x, float2 from, float2 to) {
    float normalized = (x - from[0]) / (from[1] - from[0]);
    return normalized * (to[1] - to[0]) + to[0];
}

float4 render(float2 uv) {
    float fft_frequency = uv.x;
    float fft_amplitude = builtin_texture_fft_main.Sample(builtin_texture_sampler, float2(fft_frequency, 0.5)).r;
    float fft_db = 20.0 * log(fft_amplitude / 0.5) / log(10.0);
    float fft_db_remapped = remap(fft_db, float2(-50, -0), float2(0, 1));
    float value = float(1.0 - uv.y < fft_db_remapped);
    float3 color = image.Sample(builtin_texture_sampler, uv).rgb;

    return float4(lerp(color, fft_color.rgb, fft_color.a * value), 1.0);
}
