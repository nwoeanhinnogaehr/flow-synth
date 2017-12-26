#version 150 core

in vec3 a_Translate;
in vec2 a_TexCoord;

out vec2 v_TexCoord;

void main() {
    v_TexCoord = a_TexCoord;
    gl_Position = vec4(a_Translate, 1.0);
}
