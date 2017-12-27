#version 150 core

uniform vec2 i_Resolution;

in vec2 a_Pos;
in vec3 a_Color;
in vec3 a_Translate;
in vec2 a_Scale;
out vec3 v_Color;
out vec2 v_Coord;

void main() {
    v_Color = a_Color;
    v_Coord = a_Pos;
    vec2 p = (a_Scale*a_Pos + a_Translate.xy) / i_Resolution * 2.0 - 1.0;
    gl_Position = vec4(p.x, -p.y, a_Translate.z, 1.0);
}
