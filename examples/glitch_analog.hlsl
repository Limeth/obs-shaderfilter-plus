// analog glitch shader by Charles Fettinger for obs-shaderfilter plugin 3/2019
// https://github.com/Oncorporation/obs-shaderfilter
// Converted and made linux compatible by John Jerome 'Wishdream' Romero for obs-shaderfilter-plus 10/2020

#pragma shaderfilter set scan_line_jitter_displacement__description Scanline Jitter Displacement
#pragma shaderfilter set scan_line_jitter_threshold_percent__description Scanline Jitter Threshold (%)
#pragma shaderfilter set scan_line_jitter_threshold_percent__min 0
#pragma shaderfilter set scan_line_jitter_threshold_percent__max 100
#pragma shaderfilter set scan_line_jitter_threshold_percent__slider true
#pragma shaderfilter set vertical_jump_amount__description Vertical Jump Amount
#pragma shaderfilter set vertical_speed__description Vertical Speed
#pragma shaderfilter set horizontal_shake__description Horizontal Shake
#pragma shaderfilter set color_drift_amount__description Color Drift Amount
#pragma shaderfilter set color_drift_speed__description Color Drift Speed
#pragma shaderfilter set pulse_speed_percent__min 0
#pragma shaderfilter set pulse_speed_percent__max 100
#pragma shaderfilter set pulse_speed_percent__slider true
#pragma shaderfilter set pulse_speed_percent__description Pulse Speed (%)
#pragma shaderfilter set alpha_percent__description Alpha (%)
#pragma shaderfilter set alpha_percent__min 0
#pragma shaderfilter set alpha_percent__max 100
#pragma shaderfilter set alpha_percent__slider true
#pragma shaderfilter set rotate_colors__description Rotate Colors
#pragma shaderfilter set Apply_To_Alpha_Layer__description Apply to Alpha
#pragma shaderfilter set Replace_Image_Color__description Replace Image Color
#pragma shaderfilter set Apply_To_Specific_Color__description Apply to Specific Color
#pragma shaderfilter set Color_To_Replace__description Color to Replace

uniform float scan_line_jitter_displacement = 0.33; // (displacement, threshold)
uniform int scan_line_jitter_threshold_percent = 95;
uniform float vertical_jump_amount;
uniform float vertical_speed;// (amount, speed)
uniform float horizontal_shake;
uniform float color_drift_amount;
uniform float color_drift_speed;// (amount, speed)
uniform int pulse_speed_percent = 25;
uniform int alpha_percent = 100;
uniform bool rotate_colors;
uniform bool Apply_To_Alpha_Layer = false;
uniform bool Replace_Image_Color;
uniform bool Apply_To_Specific_Color;
uniform float4 Color_To_Replace;

float nrand(float x, float y)
{
	float value = dot(float2(x, y), float2(12.9898 , 78.233 ));
	return frac(sin(value) * 43758.5453);
}

float4 render(float2 uv)
{
	float speed = float(pulse_speed_percent) * 0.01;
	float alpha = float(alpha_percent) * 0.01;
	float scan_line_jitter_threshold = float(scan_line_jitter_threshold_percent) * 0.01;
	float u = uv.x;
	float v = uv.y;
	float t = sin(builtin_elapsed_time * speed) * 2 - 1;
	float4 rgba = image.Sample(builtin_texture_sampler, uv);

	// Scan line jitter
	float jitter = nrand(v, t) * 2 - 1;
	jitter *= step(scan_line_jitter_threshold, abs(jitter)) * scan_line_jitter_displacement;

	// Vertical jump
	float jump = lerp(v, frac(v +  (t * vertical_speed)), vertical_jump_amount);

	// Horizontal shake
	float shake = ((t * (u + nrand(uv.x, uv.y))/2) - 0.5) * horizontal_shake;

	//// Color drift
	float drift = sin(jump + color_drift_speed) * color_drift_amount;

	float2 src1 = float2(rgba.x, rgba.z) * clamp(frac(float2(u + jitter + shake, jump)), -10.0, 10.0);
	float2 src2 = float2(rgba.y, rgba.w) * frac(float2(u + jitter + shake + drift, jump));

	if(rotate_colors)
	{
		// get general time number between 0 and 4
		float tx = (t + 1) * 2;
		// 3 steps  c1->c2, c2->c3, c3->c1
		//when between 0 - 1 only c1 rises then falls
		//(min(tx, 2.0) * 0.5)  range between 0-2 converted to 0-1-0
		src1.x = lerp(src1.x, rgba.x, clamp((min(tx, 2.0) * 0.5),0.0,0.5));
		//((min(max(1.0, tx),3.0) - 1) * 0.5)   range between 1-3 converted to 0-1-0
		src2.x = lerp(src2.x, rgba.y, clamp(((min(max(1.0, tx),3.0) - 1) * 0.5),0.0,0.5));
		//((min(2.0, tx) -2) * 0.5)  range between 2 and 4  converted to 0-1-0
		src1.y = lerp(src1.y, rgba.z, clamp(((min(2.0, tx) -2) * 0.5),0.0,0.5));

	}

    float4 color = rgba;
    float4 original_color = color;
    rgba = float4(src1.x, src2.x, src1.y, alpha);

    if (Apply_To_Alpha_Layer)
    {
        float4 luma = float4(dot(color, float4(0.30, 0.59, 0.11, 1.0)));
        if (Replace_Image_Color)
            color = luma;
        rgba = lerp(original_color, rgba * color, alpha);
    }

    if (Apply_To_Specific_Color)
    {
        color = original_color;
        color = (distance(color.rgb, Color_To_Replace.rgb) <= 0.075) ? rgba : color;
        rgba = lerp(original_color, color, alpha);
    }

    return rgba;
}
