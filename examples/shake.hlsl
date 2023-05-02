// Shake Effect By John Jerome 'Wishdream' Romero (https://github.com/Wishdream)  1/2021
// Rewritten and based from shake effect By Charles Fettinger (https://github.com/Oncorporation)

#pragma shaderfilter set magnitude__description Magnitude
#pragma shaderfilter set magnitude__min -1
#pragma shaderfilter set magnitude__max 1
#pragma shaderfilter set random_position__description Random Position
#pragma shaderfilter set rotation_speed__description Rotation Speed
#pragma shaderfilter set rotation_distance__description Rotation Distance
#pragma shaderfilter set worble__description Worble
#pragma shaderfilter set min_worble_size__description Min Worble Pixel Size
#pragma shaderfilter set max_worble_size__description Max Worble Pixel Size

uniform float magnitude = 0.05;
uniform bool random_position = false;
uniform float rotation_speed = 2;
uniform float rotation_distance = 0.25;

uniform bool worble = false;
uniform float worble_speed = 5;
uniform float min_worble_size = -100.0;
uniform float max_worble_size = 100.0;

//random values in range if 0.0 to 1.0
float noise_gen(float n){
  return frac(sin(n) * 43758.5453f);
}

BuiltinVertData builtin_shader_vertex(BuiltinVertData v_in)
{
  BuiltinVertData vert_out;
  float2 uv_pixel_interval = float2(1.0f / builtin_uv_size.x, 1.0f / builtin_uv_size.y);

  // Position defaults
  float3 pos = v_in.pos.xyz;
  float t;
  float s;

  // Random defaults * magnitude
  float2 rand2;
  float noise;
  rand2.x = noise_gen(builtin_elapsed_time) * 2 - 1.0f;
  rand2.y = noise_gen(builtin_elapsed_time / 2) * 2 - 1.0f;
  rand2 *= magnitude;

  if (random_position)
  {
      t = rand2.x;
      s = rand2.y;
  }
  else
  {
      t = sin(builtin_elapsed_time * rotation_speed) * rotation_distance;
      s = cos(builtin_elapsed_time * rotation_speed) * rotation_distance;
  }

  float3 direction_from_center = float3((v_in.uv.x - 0.5 + rand2.x) * uv_pixel_interval.y / uv_pixel_interval.x, v_in.uv.y - 0.5 + rand2.y, 1);
  float3 min_pos;
  float3 max_pos;
  float tvec;
  float svec;

  if (worble)
  {
    tvec = sin(builtin_elapsed_time * worble_speed) * 0.5;
    svec = cos(builtin_elapsed_time * worble_speed) * 0.5;
    min_pos = pos + direction_from_center * min_worble_size * 0.5;
    max_pos = pos + direction_from_center * max_worble_size * 0.5;
  }
  else
  {
    tvec = t;
    svec = s;
    min_pos = pos + direction_from_center * 0.5;
    max_pos = min_pos;
  }

  pos = min_pos * (1 - tvec) + max_pos * tvec;
  pos.y = (min_pos.y * (1 - svec) + max_pos.y * svec);
  float2 offset = float2(s + rand2.y, t + rand2.x);
  vert_out.pos = mul(float4(pos, 1), ViewProj);
	vert_out.uv = v_in.uv + offset;
	return vert_out;
}

float4 render(float2 uv)
{
  return image.Sample(builtin_texture_sampler, uv);
}
