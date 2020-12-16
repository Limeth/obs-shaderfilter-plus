//original by Charles 'Surn' Fettinger for obs-shaderfilter
//Converted and made linux compatible by John Jerome 'Wishdream' Romero for obs-shaderfilter-plus 10/2020
//uv_scale and uv_offset aren't applied/ignored entirely from original

#pragma shaderfilter set speed__description Speed
#pragma shaderfilter set min_growth_pixels__description Min Growth Pixels
#pragma shaderfilter set max_growth_pixels__description Max Growth Pixels

uniform float speed = 1.0;
uniform float min_growth_pixels = -2.0;
uniform float max_growth_pixels = 2.0;

BuiltinVertData builtin_shader_vertex(BuiltinVertData v_in)
{
	BuiltinVertData vert_out;

  float2 uv_pixel_interval = float2(1.0f / builtin_uv_size.x, 1.0f / builtin_uv_size.y);

	float3 pos = v_in.pos.xyz;
	float3 direction_from_center = float3((v_in.uv.x - 0.5) * uv_pixel_interval.y / uv_pixel_interval.x, v_in.uv.y - 0.5, 0);
	float3 min_pos = pos + direction_from_center * min_growth_pixels / 2;
	float3 max_pos = pos + direction_from_center * max_growth_pixels / 2;

	float t = (1 + sin(builtin_elapsed_time * speed)) / 2;
	float3 current_pos = min_pos * (1 - t) + max_pos * t;

	vert_out.pos = mul(float4(current_pos, 1.0), ViewProj);
	vert_out.uv = v_in.uv;
	return vert_out;
}

float4 render(float2 uv)
{
  return image.Sample(builtin_texture_sampler, uv);
}
