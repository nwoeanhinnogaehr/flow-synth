#version 150 core

in vec2 a_Pos;
in vec3 a_Color;
in vec2 a_Translate;
in vec2 a_Scale;
out vec3 v_Color;
out vec2 v_Coord;

void main() {
    v_Color = a_Color;
    v_Coord = a_Pos;
    gl_Position = vec4(a_Scale*a_Pos + a_Translate, 0.0, 1.0);
}
