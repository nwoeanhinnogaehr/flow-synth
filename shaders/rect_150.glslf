#version 150 core

uniform float i_Time;

in vec2 v_Coord;
in vec3 v_Color;

out vec4 Target0;

void main() {
    float edge = step(0.02, min(v_Coord.x, min(v_Coord.y, min(abs(v_Coord.x-1.0), abs(v_Coord.y-1.0)))));
    Target0 = vec4(mix(vec3(0.0, 0.0, 1.0), v_Color, edge), 1.0);
}
