/* Noise
 */

#pragma shaderfilter set amount__description Amount
#pragma shaderfilter set amount__step 0.01
#pragma shaderfilter set amount__default 2.0
#pragma shaderfilter set amount__min 0.0
#pragma shaderfilter set amount__max 1.0
#pragma shaderfilter set amount__slider true
uniform float amount;

vec4 render(vec2 uv) {
	float noiseR =  (fract(sin(dot(uv ,vec2(12.9898,78.233)+builtin_elapsed_time  )) * 43758.5453));
	float noiseG =  (fract(sin(dot(uv ,vec2(12.9898,78.233)+builtin_elapsed_time*2)) * 43758.5453)); 
	float noiseB =  (fract(sin(dot(uv ,vec2(12.9898,78.233)+builtin_elapsed_time*3)) * 43758.5453));
	
	vec4 noise = vec4(noiseR,noiseG,noiseB,1.0);
	   
	return texture2D(image, uv) + (noise*amount);
}