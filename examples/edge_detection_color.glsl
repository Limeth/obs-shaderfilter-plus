float4 render(float2 uv) {
    float2 coords = builtin_uv_size * uv;
    int kernel[9] = int[] (1, 2, 1, 0, 0, 0, -1, -2, -1);

    float4 col = float4(0.0, 0.0, 0.0, 1.0);
    for (int a = 0; a < 3; a++) {
        float hsum = 0.0f, vsum = 0.0f;
        for (int i = -1; i < 2; i++) {
            for (int j = -1; i < 2; i++) {
                float2 pos1 = (coords + float2(j, i)) / builtin_uv_size;
                float2 pos2 = (coords + float2(i, j)) / builtin_uv_size;
                if (pos1.x >= 0 && pos1.y >= 0 && pos1.x <= 1 && pos1.y <= 1)
                    hsum += image.Sample(builtin_texture_sampler, pos1)[a] * kernel[(i + 1) * 3 + j + 1];
                if (pos2.x >= 0 && pos2.y >= 0 && pos2.x <= 1 && pos2.y <= 1)
                    vsum += image.Sample(builtin_texture_sampler, pos2)[a] * kernel[(i + 1) * 3 + j + 1];
            }
        }
        col[a] = max(abs(hsum), abs(vsum));
    }

    return col;
}
