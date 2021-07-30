#pragma shaderfilter set opacity__description Opacity
#pragma shaderfilter set opacity__default 0.3
#pragma shaderfilter set opacity__min 0
#pragma shaderfilter set opacity__max 1
#pragma shaderfilter set opacity__step 0.01
#pragma shaderfilter set opacity__slider true
uniform float opacity;

#pragma shaderfilter set speed__description Speed
#pragma shaderfilter set speed__default 1
#pragma shaderfilter set speed__min 0
#pragma shaderfilter set speed__max 10
#pragma shaderfilter set speed__step 0.01
#pragma shaderfilter set speed__slider true
uniform float speed;

float4 render(float2 uv){
    float time = builtin_elapsed_time;

    float r = sin(uv.x + time * speed) * 0.5 + 0.5;
    float g = sin(uv.y - time * speed) * 0.5 + 0.5;
    float b = sin(time * speed) * 0.5 + 0.5;
    return opacity * float4(r, g, b, 1.0) + (1 - opacity) * image.Sample(builtin_texture_sampler, uv);
}
