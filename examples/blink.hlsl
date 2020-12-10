//original by Charles 'Surn' Fettinger for obs-shaderfilter
//Converted and made linux compatible by John Jerome 'Wishdream' Romero for obs-shaderfilter-plus 10/2020

#pragma shaderfilter set speed__description Speed

uniform float speed = 0.5;

float4 render(float2 uv)
{
	float4 color = image.Sample(builtin_texture_sampler, uv);
	float t = builtin_elapsed_time * speed;
	return float4(color.r, color.g, color.b, color.a * (1 + sin(t)) / 2);
}
