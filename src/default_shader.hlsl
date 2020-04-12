float4 mainImage(VertData v_in) : TARGET
{
    return image.Sample(textureSampler, v_in.uv);
}
