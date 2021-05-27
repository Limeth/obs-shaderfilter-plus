/*
 * A sharp pixellation filter.
 * 
 * Author: SolarLune
 * Date Updated: 6/6/11
 * 
 * resolution_x = target resolution on the X axis. Defaults to 320.
 * resolution_y = target resolution on the Y axis. Defaults to 240.
 * 
 * A larger X-axis resolution would equal a less blocky picture. Note that the pixellation is locked to
 * whole numbers, so there's no way to get "1.5x" pixellation, so to speak. You should probably choose
 * a resolution that's both rather small as well as a resolution that is a whole division of what you're going
 * to be running the game at most likely (i.e. 320x240 on a 1280x960 game window, not 600x500 on a 800x600 game window)
 * 
 * https://code.google.com/archive/p/solarlune-game/source/default/source
 */

#pragma shaderfilter set pixelate__description Pixelate
#pragma shaderfilter set pixelate__default 1
#pragma shaderfilter set pixelate__min 1
#pragma shaderfilter set pixelate__max 100
#pragma shaderfilter set pixelate__slider true

uniform int pixelate;

vec4 render(vec2 uv) {
    int pixelate_x =  192 * -pixelate / 100 + 1;
    int pixelate_y =  108 * -pixelate / 100 + 1;

    vec2 pixel = vec2(1.0 / builtin_uv_size.x, 1.0 / builtin_uv_size.y);
    int target_x = int(ceil(builtin_uv_size.x / pixelate_x));
    int target_y = int(ceil(builtin_uv_size.y / pixelate_y));

    float dx = pixel.x * target_x;
    float dy = pixel.y * target_y;

    vec2 coord = vec2(dx * floor(uv.x / dx), dy * floor(uv.y / dy));

    coord += pixel * 0.5; // Add half a pixel distance so that it doesn't pull from the pixel's edges,
    // allowing for a nice, crisp pixellation effect

    coord.x = min(max(0.001, coord.x), 1.0);
    coord.y = min(max(0.001, coord.y), 1.0);

    return texture2D(image, coord);
}