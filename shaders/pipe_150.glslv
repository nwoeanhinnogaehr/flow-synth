#version 150 core

uniform vec2 i_Resolution;

in vec3 a_Translate;
in vec4 a_Color;

out vec4 v_Color;

void main() {
    v_Color = a_Color;
    vec2 p = a_Translate.xy / i_Resolution * 2.0 - 1.0;
    gl_Position = vec4(p.x, -p.y, a_Translate.z, 1.0);
}
