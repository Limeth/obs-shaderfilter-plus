/* Radial Blur
 */

#pragma shaderfilter set density__description Density
#pragma shaderfilter set density__step 0.01
#pragma shaderfilter set density__default 0.2
#pragma shaderfilter set density__min 0.0
#pragma shaderfilter set density__max 0.5
#pragma shaderfilter set density__slider true

uniform float density;

vec4 render(vec2 uv) {
	int samples = 30;

    vec2 deltaTexCoord = uv - vec2(0.5,0.5);
	vec2 texCoo = uv;
	deltaTexCoord *= 1.0 / float(samples) * density;
	vec4 sample = vec4(1.0);
	float decay = 1.0;
  
	for(int i=0; i < samples ; i++) {
		texCoo -= deltaTexCoord;
		sample += texture2D(image, texCoo);
		}

	return vec4(sample/float(samples));
}
