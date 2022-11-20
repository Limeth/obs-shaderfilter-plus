
#pragma shaderfilter set border_top__slider  true
#pragma shaderfilter set border_top__max  1.00
#pragma shaderfilter set border_top__min  0.00
#pragma shaderfilter set border_top__step  0.01
uniform float border_top = 0.01;

#pragma shaderfilter set border_left__slider  true
#pragma shaderfilter set border_left__max  1.00
#pragma shaderfilter set border_left__min  0.00
#pragma shaderfilter set border_left__step  0.01
uniform float border_left = 0.01;

#pragma shaderfilter set border_right__slider  true
#pragma shaderfilter set border_right__max  1.00
#pragma shaderfilter set border_right__min  0.00
#pragma shaderfilter set border_right__step  0.01
uniform float border_right = 0.01;

#pragma shaderfilter set border_bottom__slider  true
#pragma shaderfilter set border_bottom__max  1.00
#pragma shaderfilter set border_bottom__min  0.00
#pragma shaderfilter set border_bottom__step  0.01
uniform float border_bottom = 0.01;

#pragma shaderfilter set corner_radius_top_left__slider  true
#pragma shaderfilter set corner_radius_top_left__max  1.00
#pragma shaderfilter set corner_radius_top_left__min  0.00
#pragma shaderfilter set corner_radius_top_left__step  0.01
uniform float corner_radius_top_left = 0.01;

#pragma shaderfilter set top_left_rounded__slider  true
#pragma shaderfilter set top_left_rounded__max  1.00
#pragma shaderfilter set top_left_rounded__min  0.00
#pragma shaderfilter set top_left_rounded__step  0.01
uniform float top_left_rounded = 0.01;

#pragma shaderfilter set corner_radius_top_right__slider  true
#pragma shaderfilter set corner_radius_top_right__max  1.00
#pragma shaderfilter set corner_radius_top_right__min  0.00
#pragma shaderfilter set corner_radius_top_right__step  0.01
uniform float corner_radius_top_right = 0.01;

#pragma shaderfilter set top_right_rounded__slider  true
#pragma shaderfilter set top_right_rounded__max  1.00
#pragma shaderfilter set top_right_rounded__min  0.00
#pragma shaderfilter set top_right_rounded__step  0.01
uniform float top_right_rounded = 0.01;

#pragma shaderfilter set corner_radius_bottom_right__slider  true
#pragma shaderfilter set corner_radius_bottom_right__max  1.00
#pragma shaderfilter set corner_radius_bottom_right__min  0.00
#pragma shaderfilter set corner_radius_bottom_right__step  0.01
uniform float corner_radius_bottom_right = 0.01;

#pragma shaderfilter set bottom_right_rounded__slider  true
#pragma shaderfilter set bottom_right_rounded__max  1.00
#pragma shaderfilter set bottom_right_rounded__min  0.00
#pragma shaderfilter set bottom_right_rounded__step  0.01
uniform float bottom_right_rounded = 0.01;

#pragma shaderfilter set corner_radius_bottom_left__slider  true
#pragma shaderfilter set corner_radius_bottom_left__max  1.00
#pragma shaderfilter set corner_radius_bottom_left__min  0.00
#pragma shaderfilter set corner_radius_bottom_left__step  0.01
uniform float corner_radius_bottom_left = 0.01;

#pragma shaderfilter set bottom_left_rounded__slider  true
#pragma shaderfilter set bottom_left_rounded__max  1.00
#pragma shaderfilter set bottom_left_rounded__min  0.00
#pragma shaderfilter set bottom_left_rounded__step  0.01
uniform float bottom_left_rounded = 0.01;


float4 render(float2 uv) {
    float4 pixel = image.Sample(builtin_texture_sampler, uv);
    int closedEdgeX = 0;
    int closedEdgeY = 0;
 if (uv.x < border_left || uv.x > 1-border_right){ 
    pixel.a = 0;
}
if (uv.y < border_top || uv.y > 1-border_bottom){ 
    pixel.a = 0;
}


float top_left_square = border_left+corner_radius_top_left*top_left_rounded;
float top_left_top_square = corner_radius_top_left+border_top;
float top_left_x_mov = -top_left_square;
float top_left_y_mov = +border_top+corner_radius_top_left;

if(uv.y < top_left_top_square  && uv.x < top_left_square){
	if (length(vec2((uv.x+top_left_x_mov)/(corner_radius_top_left*top_left_rounded),(uv.y-top_left_y_mov)/corner_radius_top_left)) >= 1) {
	     pixel.a =0;

}
}

float top_right_square = border_right+corner_radius_top_right*top_right_rounded;
float top_right_top_square = corner_radius_top_right+border_top;
float top_right_x_mov = 1-top_right_square;
float top_right_y_mov = +border_top+corner_radius_top_right;

if(uv.y < top_right_top_square  && uv.x > 1-top_right_square){
	if (length(vec2((uv.x-top_right_x_mov)/(corner_radius_top_right*top_right_rounded),(uv.y-top_right_y_mov)/corner_radius_top_right)) >= 1) {
	     pixel.a =0;

}
}


float bottom_right_square = border_right+corner_radius_bottom_right*bottom_right_rounded;
float bottom_right_bottom_square = corner_radius_bottom_right+border_bottom;
float bottom_right_x_mov = 1-bottom_right_square;
float bottom_right_y_mov = 1-border_bottom-corner_radius_bottom_right;

if(uv.y > 1-bottom_right_bottom_square  && uv.x > 1-bottom_right_square){
	if (length(vec2((uv.x-bottom_right_x_mov)/(corner_radius_bottom_right*bottom_right_rounded),(uv.y-bottom_right_y_mov)/corner_radius_bottom_right)) >= 1) {
	     pixel.a =0;

}
}

float bottom_left_square = border_left+corner_radius_bottom_left*bottom_left_rounded;
float bottom_left_bottom_square = corner_radius_bottom_left+border_bottom;
float bottom_left_x_mov = -bottom_left_square;
float bottom_left_y_mov = +border_bottom+corner_radius_bottom_left;

if(uv.y > 1-bottom_left_bottom_square  && uv.x < bottom_left_square){
	if (length(vec2((uv.x+bottom_left_x_mov)/(corner_radius_bottom_left*bottom_left_rounded),(1-(uv.y+bottom_left_y_mov))/corner_radius_bottom_left)) >= 1) {
	     pixel.a =0;

}
}

    return pixel;
}
