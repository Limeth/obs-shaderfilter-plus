/* Wave, from NetherEran
*/

#pragma shaderfilter set vel__description Velocity
#pragma shaderfilter set vel__step 0.001
#pragma shaderfilter set vel__default 10.0
#pragma shaderfilter set vel__min 0.0
#pragma shaderfilter set vel__max 50.0
#pragma shaderfilter set vel__slider true

#pragma shaderfilter set amp__description Amplitude
#pragma shaderfilter set amp__step 0.001
#pragma shaderfilter set amp__default 0.05
#pragma shaderfilter set amp__min 0.0
#pragma shaderfilter set amp__max 0.1
#pragma shaderfilter set amp__slider true

#pragma shaderfilter set freq__description Frequency
#pragma shaderfilter set freq__step 0.001
#pragma shaderfilter set freq__default 10.0
#pragma shaderfilter set freq__min 0.0
#pragma shaderfilter set freq__max 40.0
#pragma shaderfilter set freq__slider true

uniform float vel;
uniform float amp;
uniform float freq;

float4 render(float2 uv) {
	float2 newuv;
	float time = builtin_elapsed_time_since_shown;
	//~ newuv[0] = uv[0] + cos((builtin_elapsed_time_since_shown + uv[0]) * 0.5) * (1 - uv[1]) * 0.05;
	newuv[0] = uv[0] + sin(uv[1] * freq + time * vel ) * amp;
	newuv[1] = uv[1];
	float4 image_color = image.Sample(builtin_texture_sampler, newuv);
	return image_color;
}
