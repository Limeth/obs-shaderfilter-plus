uniform float4x4 ViewProj;
uniform texture2d image;

uniform int   builtin_frame;
uniform float builtin_framerate;
uniform float builtin_elapsed_time;
uniform float builtin_elapsed_time_previous;
uniform float builtin_elapsed_time_since_shown;
uniform float builtin_elapsed_time_since_shown_previous;
uniform int2  builtin_uv_size;

sampler_state builtin_texture_sampler {
    Filter = Linear;
    AddressU = Border;
    AddressV = Border;
    BorderColor = 00000000;
};

struct BuiltinVertData {
    float4 pos : POSITION;
    float2 uv : TEXCOORD0;
};

BuiltinVertData builtin_shader_vertex(BuiltinVertData v_in)
{
    BuiltinVertData vert_out;
    vert_out.pos = mul(float4(v_in.pos.xyz, 1.0), ViewProj);
    vert_out.uv = v_in.uv;
    return vert_out;
}

float4 builtin_shader_fragment(BuiltinVertData v_in) : TARGET {
    return image.Sample(builtin_texture_sampler, v_in.uv);
}

technique Draw
{
    pass
    {
        vertex_shader = builtin_shader_vertex(v_in);
        pixel_shader = builtin_shader_fragment(v_in);
    }
}
