#pragma shaderfilter set threshold__description Threshold
#pragma shaderfilter set threshold__default 1
#pragma shaderfilter set threshold__min 0.01
#pragma shaderfilter set threshold__max 10
#pragma shaderfilter set threshold__step 0.01
#pragma shaderfilter set threshold__slider true
uniform float threshold;

float avg(float4 col) {
    return (col.r + col.g + col.b) / 3.0;
}

float4 render(float2 uv) {
    float2 coords = builtin_uv_size * uv;
    int kernel[9] = int[] (1, 2, 1, 0, 0, 0, -1, -2, -1);

    float hsum = 0.0f, vsum = 0.0f;
    for (int i = -1; i < 2; i++) {
        for (int j = -1; i < 2; i++) {
            float2 pos1 = (coords + float2(j, i)) / builtin_uv_size;
            float2 pos2 = (coords + float2(i, j)) / builtin_uv_size;
            if (pos1.x >= 0 && pos1.y >= 0 && pos1.x <= 1 && pos1.y <= 1)
                hsum += avg(image.Sample(builtin_texture_sampler, pos1)) * kernel[(i + 1) * 3 + j + 1];
            if (pos2.x >= 0 && pos2.y >= 0 && pos2.x <= 1 && pos2.y <= 1)
                vsum += avg(image.Sample(builtin_texture_sampler, pos2)) * kernel[(i + 1) * 3 + j + 1];
        }
    }

    float res = max(abs(hsum), abs(vsum));
    if (res >= threshold / 10.0)
        res = 1;
    return float4(1 - res, 1 - res, 1 - res, 1.0) * image.Sample(builtin_texture_sampler, uv);
}
