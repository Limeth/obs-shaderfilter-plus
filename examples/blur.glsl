#pragma shaderfilter set kernel_size__description Kernel size
#pragma shaderfilter set kernel_size__default 0
#pragma shaderfilter set kernel_size__min 0
#pragma shaderfilter set kernel_size__max 20
#pragma shaderfilter set kernel_size__step 1
#pragma shaderfilter set kernel_size__slider true
uniform int kernel_size;

float4 render(float2 uv) {
	float2 coords = builtin_uv_size * uv;

	float4 col = float4(0.0, 0.0, 0.0, 0.0);
	int count = 0;
	for (int i = -kernel_size; i <= kernel_size; i++) {
		for (int j = -kernel_size; j <= kernel_size; j++) {
			float2 pos = (coords + float2(i, j)) / builtin_uv_size;
			if (pos.x >= 0 && pos.y >= 0 && pos.x <= 1 && pos.y <= 1)
				count++;
			col += image.Sample(builtin_texture_sampler, pos);
		}
	}
	col /= count;

	return col;
}
