/* Simple Box Blur
 */

#pragma shaderfilter set amount__description Amount
#pragma shaderfilter set amount__default 2
#pragma shaderfilter set amount__min 0
#pragma shaderfilter set amount__max 8
#pragma shaderfilter set amount__slider true

#pragma shaderfilter set multiplier__description Multiplier
#pragma shaderfilter set multiplier__default 5.0
#pragma shaderfilter set multiplier__min 1.0
#pragma shaderfilter set multiplier__max 20.0
#pragma shaderfilter set multiplier__slider true

uniform int amount;
uniform float multiplier;

vec4 render(vec2 uv) {
	vec2 pix = vec2(1.0 / builtin_uv_size.x, 1.0 / builtin_uv_size.y) * multiplier;
	vec4 sum = vec4(0);
	
	int i, j;
	
	for(i = -amount; i <= amount; i++){
		for(j = -amount; j <= amount; j++){
			sum += texture2D(image, uv + vec2(float(i), float(j)) * pix);
		}
	}
	return sum / pow(amount * 2 + 1, 2);
}
