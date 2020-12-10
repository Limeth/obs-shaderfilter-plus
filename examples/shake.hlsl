// Shake Effect By Charles Fettinger (https://github.com/Oncorporation)  2/2019
// Added some randomization based upon random_scale input
// Converted and made linux compatible by John Jerome 'Wishdream' Romero for obs-shaderfilter-plus 10/2020
// uv_scale and uv_offset aren't applied/ignored entirely from original
// rotate pixels ay not work may not work as intended

#pragma shaderfilter set speed__description Speed
#pragma shaderfilter set random_scale__description Random Scale
#pragma shaderfilter set worble__description Worble
#pragma shaderfilter set min_growth_pixels__description Min Growth Pixels
#pragma shaderfilter set max_growth_pixels__description Max Growth Pixels
#pragma shaderfilter set randomize_movement__description Randomize Movement

uniform float speed = 1.0;
uniform float random_scale = 0.25;
uniform bool worble = false;
uniform float min_growth_pixels = -2.0;
uniform float max_growth_pixels = 2.0;
uniform bool randomize_movement = false;

float noise2D(float2 uv)
{
  float value = dot(uv, float2(12.9898 , 78.233 ));
	return frac(sin(value) * 43758.5453);
}
//noise values in range if 0.0 to 1.0

float noise3D(float x, float y, float z) {
    float ptr = 0.0f;
    return frac(sin(x*112.9898f + y*179.233f + z*237.212f) * 43758.5453f);
}

BuiltinVertData builtin_shader_vertex(BuiltinVertData v_in)
{
	BuiltinVertData vert_out;
  float rand_f = noise2D(v_in.uv);
  float2 uv_pixel_interval = float2(1.0f / builtin_uv_size.x, 1.0f / builtin_uv_size.y);

	float3 pos = v_in.pos.xyz;
	float t;
	float s;
	float noise;

	if (randomize_movement)
	{
		t = (rand_f * 2) - 1.0f;
    s = (1 - rand_f * 2) - 1.0f;
		noise = clamp( rand_f * random_scale,-0.99, 0.99);
	}
	else
	{
		t = (1 + sin(builtin_elapsed_time * speed)) / 2;
		s = (1 + cos(builtin_elapsed_time * speed)) / 2;
		noise = clamp(noise3D(t,s,100) * random_scale,-0.99, 0.99);
	}

	float3 direction_from_center = float3((v_in.uv.x - 0.5 + noise) * uv_pixel_interval.y / uv_pixel_interval.x, v_in.uv.y - 0.5 + noise, 1);
	float3 min_pos;
	float3 max_pos;
    if (worble)
    {
        min_pos = pos + direction_from_center * min_growth_pixels * 0.5;
        max_pos = pos + direction_from_center * max_growth_pixels * 0.5;
    }
    else
    {
    	min_pos = pos + direction_from_center * 0.5;
		max_pos = min_pos;
    }

	float3 current_pos = min_pos * (1 - t) + max_pos * t;
	//current_pos.x = v_in.pos.x + (t * min_pos.x);
	current_pos.y = (min_pos.y * (1 - s) + max_pos.y * s);
	//current_pos.y = v_in.pos.y + (s * min_pos.y);
	//current_pos.z = min_pos.z * (1 - s) + max_pos.z * s;

	float2 offset = float2(1 - t + noise, 1 - s + noise);

	vert_out.pos = mul(float4(current_pos, 1), ViewProj);

	//float2 scale = uv_scale;
	//scale += dot(pos - current_pos, 1);

	vert_out.uv = v_in.uv  + offset;
	return vert_out;
}

float4 render(float2 uv)
{
  return image.Sample(builtin_texture_sampler, uv);
}
