#pragma shaderfilter set opacity__description Opacity
#pragma shaderfilter set opacity__default 0.3
#pragma shaderfilter set opacity__min 0
#pragma shaderfilter set opacity__max 1
#pragma shaderfilter set opacity__step 0.01
#pragma shaderfilter set opacity__slider true
uniform float opacity;

float4 render(float2 uv){
    float time = builtin_elapsed_time;

    float r = sin(uv.x + time) * 0.5 + 0.5;
    float g = sin(uv.y - time) * 0.5 + 0.5;
    float b = sin(time) * 0.5 + 0.5;
    return opacity * float4(r, g, b, 1.0) + (1 - opacity) * image.Sample(builtin_texture_sampler, uv);
}