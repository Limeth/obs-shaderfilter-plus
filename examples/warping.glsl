/* Warping de ShaderToy
*/
#pragma shaderfilter set size__description Size
#pragma shaderfilter set size__default 3.0
#pragma shaderfilter set size__min 0.1
#pragma shaderfilter set size__max 5.0
#pragma shaderfilter set size__slider true

uniform float size;

float4 render( float2 uv )
{
	float4 image_color = image.Sample(builtin_texture_sampler, uv);

	//~ vec2 warp = texture( image, uv*0.1 + builtin_elapsed_time*vec2(0.04,0.03) ).xz;
    float freq = size*sin(0.5*builtin_elapsed_time);
    vec2 warp = 0.5000*cos( uv.xy*1.0*freq + vec2(0.0,1.0) + builtin_elapsed_time ) +
                0.2500*cos( uv.yx*2.3*freq + vec2(1.0,2.0) + builtin_elapsed_time ) +
                0.1250*cos( uv.xy*4.1*freq + vec2(5.0,3.0) + builtin_elapsed_time ) +
                0.0625*cos( uv.yx*7.9*freq + vec2(3.0,4.0) + builtin_elapsed_time );

	float2 st = uv + warp*0.5;
	float4 new_image = image.Sample(builtin_texture_sampler, st);
	//~ new_image = vec4( texture( image_color, st ).xyz, 1.0 );
	return new_image;
}


// https://www.shadertoy.com/view/Xsl3zn
// Created by inigo quilez - iq/2013
// License Creative Commons Attribution-NonCommercial-ShareAlike 3.0 Unported License.

// make 1 to see a procedural warp/deformation
//~ #define PROCEDURAL 0

//~ void mainImage( out vec4 fragColor, in vec2 fragCoord )
//~ {
	//~ vec2 uv = fragCoord/iResolution.xy;

//~ #if PROCEDURAL==0
    //~ vec2 warp = texture( iChannel0, uv*0.1 + iTime*vec2(0.04,0.03) ).xz;
//~ #else    
    //~ float freq = 3.0*sin(0.5*iTime);
    //~ vec2 warp = 0.5000*cos( uv.xy*1.0*freq + vec2(0.0,1.0) + iTime ) +
                //~ 0.2500*cos( uv.yx*2.3*freq + vec2(1.0,2.0) + iTime) +
                //~ 0.1250*cos( uv.xy*4.1*freq + vec2(5.0,3.0) + iTime ) +
                //~ 0.0625*cos( uv.yx*7.9*freq + vec2(3.0,4.0) + iTime );
//~ #endif
    
	//~ vec2 st = uv + warp*0.5;

	//~ fragColor = vec4( texture( iChannel0, st ).xyz, 1.0 );
//~ }
