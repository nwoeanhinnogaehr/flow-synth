#version 150 core

uniform float i_Time;

in vec2 v_Coord;
in vec3 v_Color;

out vec4 Target0;

void main() {
    Target0 = vec4(v_Color*sin(length(v_Coord)*32.0+i_Time*32.0), 1.0);
}
