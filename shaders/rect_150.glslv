#version 150 core

uniform float i_AspectRatio;

in vec2 a_Pos;
in vec3 a_Color;
in vec3 a_Translate;
in vec2 a_Scale;
out vec3 v_Color;
out vec2 v_Coord;

void main() {
    v_Color = a_Color;
    v_Coord = a_Pos;
    gl_Position = vec4((a_Scale*a_Pos + a_Translate.xy) / vec2(i_AspectRatio, 1.0), a_Translate.z, 1.0);
}
