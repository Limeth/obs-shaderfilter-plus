// Configure builtin uniforms
#pragma shaderfilter set main dampening_factor_attack 0.0
#pragma shaderfilter set main dampening_factor_release 0.2
uniform texture2d builtin_texture_fft_main;

// Define configurable variables
uniform float4 fft_color;

float remap(float x, vec2 from, vec2 to) {
    float normalized = (x - from[0]) / (from[1] - from[0]);
    return normalized * (to[1] - to[0]) + to[0];
}

vec4 render(vec2 uv) {
    float fft_frequency = uv.x;
    float fft_amplitude = builtin_texture_fft_main.Sample(builtin_texture_sampler, vec2(fft_frequency, 0.5)).r;
    float fft_db = 20.0 * log(fft_amplitude / 0.5) / log(10.0);
    float fft_db_remapped = remap(fft_db, vec2(-50, -0), vec2(0, 1));
    float value = float(1.0 - uv.y < fft_db_remapped);
    vec3 color = image.Sample(builtin_texture_sampler, uv).rgb;

    return vec4(mix(color, fft_color.rgb, fft_color.a * value), 1.0);
}
