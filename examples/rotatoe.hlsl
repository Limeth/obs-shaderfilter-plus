// Rotation Effect By Charles Fettinger (https://github.com/Oncorporation)  10/2019
// Converted and made linux compatible by John Jerome 'Wishdream' Romero for obs-shaderfilter-plus 10/2020
// uv_scale and uv_offset aren't applied/ignored entirely from original
// rotate pixels ay not work may not work as intended

#pragma shaderfilter set speed_percent__description Speed (%)
#pragma shaderfilter set Axis_X__description X
#pragma shaderfilter set Axis_Y__description Y
#pragma shaderfilter set Axis_Z__description Z
#pragma shaderfilter set Angle_Degrees__Angle (Degrees)
#pragma shaderfilter set Rotate_Transform__description Rotate Transform
#pragma shaderfilter set Rotate_Pixels__description Rotate Pixels
#pragma shaderfilter set Rotate_Colors__description Rotate Colors
#pragma shaderfilter set center_width_percentage__description Rotation Center X (%)
#pragma shaderfilter set center_width_percentage__min 0
#pragma shaderfilter set center_width_percentage__max 100
#pragma shaderfilter set center_width_percentage__slider true
#pragma shaderfilter set center_height_percentage__description Rotation Center Y (%)
#pragma shaderfilter set center_height_percentage__min 0
#pragma shaderfilter set center_height_percentage__max 100
#pragma shaderfilter set center_height_percentage__slider true

uniform int speed_percent = 50; //<Range(-10.0, 10.0)>
uniform float Axis_X = 0.0;
uniform float Axis_Y = 0.0;
uniform float Axis_Z = 1.0;
uniform float Angle_Degrees = 45.0;
uniform bool Rotate_Transform = true;
uniform bool Rotate_Pixels = false;
uniform bool Rotate_Colors = false;
uniform int center_width_percentage = 50;
uniform int center_height_percentage = 50;

float3x3 rotAxis(float3 axis, float a) {
	float s=sin(a);
	float c=cos(a);
	float oc=1.0-c;

	float3 as=axis*s;

	float3x3 p=float3x3(axis.x*axis,axis.y*axis,axis.z*axis);
	float3x3 q=float3x3(c,-as.z,as.y,as.z,c,-as.x,-as.y,as.x,c);
	return p*oc+q;
}

BuiltinVertData builtin_shader_vertex(BuiltinVertData v_in)
{
	BuiltinVertData vert_out;
	vert_out.pos =  mul(float4(v_in.pos.xyz, 1.0), ViewProj);

	float speed = float(speed_percent) * 0.01;
	// circular easing variable
	float PI = 3.1415926535897932384626433832795; //acos(-1);
	float PI180th = 0.0174532925; //PI divided by 180
	float direction = abs(sin((builtin_elapsed_time - 0.001) * speed));
	float t = sin(builtin_elapsed_time * speed);
	float angle_degrees = PI180th * Angle_Degrees;

	// use matrix to transform rotation
	if (Rotate_Transform)
		vert_out.pos.xyz = mul(vert_out.pos.xyz,rotAxis(float3(Axis_X,Axis_Y,Axis_Z), (angle_degrees * t))).xyz;

	vert_out.uv  = v_in.uv;

	return vert_out;
}

float4 render(float2 uv)
{
	float4 rgba = image.Sample(builtin_texture_sampler, uv);

	float speed = float(speed_percent) * 0.01;
	// circular easing variable
	float PI = 3.1415926535897932384626433832795; //acos(-1);
	float PI180th = 0.0174532925; //PI divided by 180
	float direction = abs(sin((builtin_elapsed_time - 0.001) * speed));
	float t = sin(builtin_elapsed_time * speed);
	float angle_degrees = PI180th * Angle_Degrees;

	// use matrix to transform pixels
	if (Rotate_Pixels)
	{
		float2 center_pixel_coordinates = float2(float(center_width_percentage) * 0.01, float(center_height_percentage) * 0.01 );
		float3x3 rotate_axis = rotAxis(float3(Axis_X ,Axis_Y, Axis_Z), (angle_degrees * t));
		float3 rotate_uv = mul(float3(uv - center_pixel_coordinates, 0), rotate_axis);
		rgba = image.Sample(builtin_texture_sampler, rotate_uv.xy + center_pixel_coordinates);
	}
	if (Rotate_Colors)
		rgba.rgb = mul(rgba.rgb, rotAxis(float3(Axis_X,Axis_Y,Axis_Z), (angle_degrees * t))).xyz;

	return rgba;
}
