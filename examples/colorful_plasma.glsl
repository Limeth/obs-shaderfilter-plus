/* Fondo de pantalla del telefono
 */
 
vec3 hsv2rgb(vec3 c){
  vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
  vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
  return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
}

vec4 render(vec2 p) {
  p += ( p / builtin_uv_size.xy ) - .5;
  vec2 direction = vec2(cos(builtin_elapsed_time), sin(builtin_elapsed_time));

  float sx = 0.15 * sin( 5.0 * p.x - builtin_elapsed_time - length(p)); 
  float dy = 1.0 / ( 10.0 * abs(p.y - sx ));
  vec3 c = hsv2rgb(vec3( ( (p.x + 0.1) * dy) * 0.5 + cos(dot(p, direction)) * 5.0, 0.4, dy));
  return vec4( c, 1.0 );
}
