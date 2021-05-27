/* Chromatic Aberration
 */

#pragma shaderfilter set chromab_dist__description Distance
#pragma shaderfilter set chromab_dist__default 2
#pragma shaderfilter set chromab_dist__min 0
#pragma shaderfilter set chromab_dist__max 8
#pragma shaderfilter set chromab_dist__slider true

uniform float chromab_dist;

vec4 render(vec2 uv) {

	vec3 new = vec3(0.0);
	vec3 distance = vec3(1.0-(chromab_dist*0.01), 1.0-(chromab_dist*0.02), 1.0-(chromab_dist*0.03));
	
	new.r = vec3(texture2D(image, (uv - vec2(0.50,0.50)) * distance[0] + vec2(0.50,0.50))).r;
	new.g = vec3(texture2D(image, (uv - vec2(0.50,0.50)) * distance[1] + vec2(0.50,0.50))).g;
	new.b = vec3(texture2D(image, (uv - vec2(0.50,0.50)) * distance[2] + vec2(0.50,0.50))).b;
	
	return vec4(new, 1.0);
}
