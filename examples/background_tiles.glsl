#pragma shaderfilter set opacity__description Opacity
#pragma shaderfilter set opacity__default 0.3
#pragma shaderfilter set opacity__min 0
#pragma shaderfilter set opacity__max 1
#pragma shaderfilter set opacity__step 0.01
#pragma shaderfilter set opacity__slider true
uniform float opacity;

#pragma shaderfilter set size__description Size
#pragma shaderfilter set size__default 100
#pragma shaderfilter set size__min 1
#pragma shaderfilter set size__max 250
#pragma shaderfilter set size__step 1
#pragma shaderfilter set size__slider true
uniform float size;

#pragma shaderfilter set speed__description Speed
#pragma shaderfilter set speed__default 0.5
#pragma shaderfilter set speed__min 0
#pragma shaderfilter set speed__max 10
#pragma shaderfilter set speed__step 0.01
#pragma shaderfilter set speed__slider true
uniform float speed;

// https://www.shadertoy.com/view/wscGWl
// Credits to reyemxela

float rand(float2 co){ return fract(sin(dot(co.xy ,float2(12.9898,78.233))) * 43758.5453); } // random noise

float getCellBright(float2 id) {
    return sin((builtin_elapsed_time+2.)*rand(id)*2.)*.5+.5; // returns 0. to 1.
}

float4 render(float2 uv) {
    float2 pos = uv;

    float mx = max(builtin_uv_size.x, builtin_uv_size.y);
    uv = uv * builtin_uv_size / mx;

    float time = builtin_elapsed_time*speed;

    uv *= size; // grid size

    float2 id = floor(uv); // id numbers for each cell
    float2 gv = fract(uv)-.5; // uv within each cell, from -.5 to .5

    float3 color = float3(0.);

    float randBright = getCellBright(id);

    float3 colorShift = float3(rand(id)*.1); // subtle random color offset per cell

    color = 0.6 + 0.5*cos(time + (id.xyx*.025) + float3(4,2,1) + colorShift); // RGB with color offset

    float shadow = 0.;
    shadow += smoothstep(.0, .7,  gv.x*min(0., (getCellBright(float2(id.x-1., id.y)) - getCellBright(id)))); // left shadow
    shadow += smoothstep(.0, .7, -gv.y*min(0., (getCellBright(float2(id.x, id.y+1.)) - getCellBright(id)))); // top shadow

    color -= shadow*.4;

    color *= 1. - (randBright*.2);

    return opacity * float4(color, 1.0) * float4(0.7, 0.7, 0.7, 1.0) + (1 - opacity) * image.Sample(builtin_texture_sampler, pos);

}
