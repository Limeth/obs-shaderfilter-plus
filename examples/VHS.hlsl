//based on https://www.shadertoy.com/view/Ms3XWH converted by Exeldro  v 1.0
//original by Charles 'Surn' Fettinger for obs-shaderfilter 9/2020
//converted and made linux compatible by John Jerome 'Wishdream' Romero for obs-shaderfilter-plus 10/2020

#pragma shaderfilter set range__description Range
#pragma shaderfilter set noiseQuality__description Noise Quality
#pragma shaderfilter set noiseIntensity__description Noise Intensity
#pragma shaderfilter set offsetIntensity__description Offset Intensity
#pragma shaderfilter set colorOffsetIntensity__description Color Offset Intensity
#pragma shaderfilter set lumaMin__description Luma Minimum
#pragma shaderfilter set lumaMinSmooth__description Luma Minimum Smooth
#pragma shaderfilter set Alpha_Percentage__description Alpha (%)
#pragma shaderfilter set Alpha_Percentage__min 0
#pragma shaderfilter set Alpha_Percentage__max 100
#pragma shaderfilter set Alpha_Percentage__slider true
#pragma shaderfilter set Apply_To_Image__description Apply to Image
#pragma shaderfilter set Replace_Image_Color__description Replace Image Color
#pragma shaderfilter set Color_To_Replace__description Color to Replace
#pragma shaderfilter set Apply_To_Specific_Color__description Apply to Specific Color

uniform float range = 0.05;
uniform float noiseQuality = 250.0;
uniform float noiseIntensity = 0.88;
uniform float offsetIntensity = 0.02;
uniform float colorOffsetIntensity = 1.3;
uniform float lumaMin = 0.01;
uniform float lumaMinSmooth = 0.04;
uniform float Alpha_Percentage = 100; //<Range(0.0,100.0)>
uniform bool Apply_To_Image;
uniform bool Replace_Image_Color;
uniform float4 Color_To_Replace;
uniform bool Apply_To_Specific_Color;

float rand(float2 co)
{
    return frac(sin(dot(co.xy, float2(12.9898, 78.233))) * 43758.5453);
}

float verticalBar(float pos, float uvY, float offset)
{
    float edge0 = (pos - range);
    float edge1 = (pos + range);

    float x = smoothstep(edge0, pos, uvY) * offset;
    x -= smoothstep(pos, edge1, uvY) * offset;
    return x;
}

float4 render(float2 st)
{
    float2 uv = st;
    for (float i = 0.0; i < 0.71; i += 0.1313)
    {
        float d = fmod((builtin_elapsed_time * i), 1.7);
        float o = sin(1.0 - tan(builtin_elapsed_time * 0.24 * i));
        o *= offsetIntensity;
        uv.x += verticalBar(d, uv.y, o);
    }
    float uvY = uv.y;
    uvY *= noiseQuality;
    uvY = float(int(uvY)) * (1.0 / noiseQuality);
    float noise = rand(float2(builtin_elapsed_time * 0.00001, uvY));
    uv.x += noise * noiseIntensity / 100.0;

    float2 offsetR = float2(0.006 * sin(builtin_elapsed_time), 0.0) * colorOffsetIntensity;
    float2 offsetG = float2(0.0073 * (cos(builtin_elapsed_time * 0.97)), 0.0) * colorOffsetIntensity;

    float r = image.Sample(builtin_texture_sampler, uv + offsetR).r;
    float g = image.Sample(builtin_texture_sampler, uv + offsetG).g;
    float b = image.Sample(builtin_texture_sampler, uv).b;

    float4 rgba = float4(r, g, b, 1.0);

    float4 color;
    float4 original_color;
    if (Apply_To_Image)
    {
        color = image.Sample(builtin_texture_sampler, st);
        original_color = color;
        float luma_dot = dot(color, float4(0.30, 0.59, 0.11, 1.0));
        float4 luma = float4(luma_dot, luma_dot, luma_dot, luma_dot);
        if (Replace_Image_Color)
            color = luma;
        rgba = lerp(original_color, rgba * color, clamp(Alpha_Percentage * .01, 0, 1.0));

    }
    if (Apply_To_Specific_Color)
    {
        color = image.Sample(builtin_texture_sampler, st);
        original_color = color;
        color = (distance(color.rgb, Color_To_Replace.rgb) <= 0.075) ? rgba : color;
        rgba = lerp(original_color, color, clamp(Alpha_Percentage * .01, 0, 1.0));
    }

    return rgba;
}
