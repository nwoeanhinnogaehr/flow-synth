#version 150 core

uniform sampler2D i_Texture;

in vec2 v_TexCoord;

out vec4 Target0;

void main() {
    Target0 = texture2D(i_Texture, v_TexCoord);
}
