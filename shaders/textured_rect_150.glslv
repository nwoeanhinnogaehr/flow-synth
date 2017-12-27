#version 150 core

uniform vec2 i_Resolution;

in vec3 a_Translate;
in vec2 a_TexCoord;

out vec2 v_TexCoord;

void main() {
    v_TexCoord = a_TexCoord;
    vec2 p = a_Translate.xy / i_Resolution * 2.0 - 1.0;
    gl_Position = vec4(p.x, -p.y, a_Translate.z, 1.0);
}
